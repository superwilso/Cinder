// hls.h — HLS (HTTP Live Streaming) for internet radio (main.cpp, radio_hls_thread): playlists,
// MPEG-TS audio and the AAC/MP3 sniff. Pure functions, no I/O; tested by tools/hls_selftest.cpp.
//
// A station's .m3u8 is either a master playlist (#EXT-X-STREAM-INF lines, one per bitrate) or a
// media playlist (#EXTINF segments, #EXT-X-MEDIA-SEQUENCE numbering the first). Live radio keeps
// adding segments at the end; the player fetches each new one and plays the audio inside. Segments
// are MPEG-TS (188-byte packets), "packed audio" (ADTS or MP3 frames, perhaps behind an ID3 tag), or
// fragmented MP4 (#EXT-X-MAP names the init segment; mp4_init and mp4_audio turn it into ADTS).
#ifndef CINDER_HLS_H
#define CINDER_HLS_H
#include <cstddef>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

namespace hls {

// `ref` from the playlist at `base`: a full URL, a host-absolute "/path", or relative to the
// playlist's directory (the playlist's own query string is not carried over).
inline std::string resolve(const std::string& base, const std::string& ref) {
    if (ref.compare(0, 7, "http://") == 0 || ref.compare(0, 8, "https://") == 0) return ref;
    const size_t scheme = base.find("://");
    if (scheme == std::string::npos) return ref;
    const size_t host_end = base.find_first_of("/?#", scheme + 3);
    const std::string origin = base.substr(0, host_end);
    if (!ref.empty() && ref[0] == '/')
        return ref.size() > 1 && ref[1] == '/' ? base.substr(0, scheme + 1) + ref : origin + ref;
    std::string dir = base.substr(0, base.find_first_of("?#"));
    const size_t slash = dir.rfind('/');
    dir = slash == std::string::npos || slash < scheme + 3 ? origin + "/" : dir.substr(0, slash + 1);
    return dir + ref;
}

struct Playlist {
    bool master = false;
    std::string variant;            // master: the stream to play (resolved)
    long seq = 0;                   // media: the first segment's sequence number
    double target = 6;              // media: #EXT-X-TARGETDURATION, seconds
    bool end = false;               // #EXT-X-ENDLIST: no more segments will come
    std::string problem;            // why it cannot be played, for the page
    std::string map;                // media: the fMP4 init segment (#EXT-X-MAP, resolved), "" for TS
    std::vector<std::string> segs;  // media: segment URLs (resolved), oldest first
};

// The attribute `key` of an #EXT-X-STREAM-INF line ("" when absent).
inline std::string attr(const std::string& line, const char* key) {
    const std::string k = std::string(key) + "=";
    size_t at = 0;
    while ((at = line.find(k, at)) != std::string::npos) {
        if (at == 0 || line[at - 1] == ',' || line[at - 1] == ':') break;
        at += k.size();
    }
    if (at == std::string::npos) return "";
    at += k.size();
    if (at < line.size() && line[at] == '"') {
        const size_t e = line.find('"', at + 1);
        return line.substr(at + 1, e == std::string::npos ? std::string::npos : e - at - 1);
    }
    return line.substr(at, line.find(',', at) - at);
}

// A master playlist chooses its highest-bitrate audio-only stream (radio tops out near 320 kbit/s,
// well inside what the player's Wi-Fi carries); a stream naming a video codec is passed over.
inline Playlist parse(const std::string& body, const std::string& base) {
    Playlist p;
    long best = -1;
    bool want_uri = false, take = false;
    size_t at = 0;
    while (at < body.size()) {
        size_t e = body.find('\n', at);
        if (e == std::string::npos) e = body.size();
        std::string l = body.substr(at, e - at);
        at = e + 1;
        while (!l.empty() && (l.back() == '\r' || l.back() == ' ' || l.back() == '\t')) l.pop_back();
        if (l.empty()) continue;
        if (l.compare(0, 18, "#EXT-X-STREAM-INF:") == 0) {
            p.master = true;
            const std::string codecs = attr(l, "CODECS");
            const bool video = codecs.find("avc") != std::string::npos || codecs.find("hvc") != std::string::npos ||
                               codecs.find("hev") != std::string::npos;
            const long bw = std::atol(attr(l, "BANDWIDTH").c_str());
            want_uri = true;
            take = !video && bw > best;
            if (take) best = bw;
        } else if (l.compare(0, 22, "#EXT-X-MEDIA-SEQUENCE:") == 0) {
            p.seq = std::atol(l.c_str() + 22);
        } else if (l.compare(0, 22, "#EXT-X-TARGETDURATION:") == 0) {
            p.target = std::atof(l.c_str() + 22);
        } else if (l == "#EXT-X-ENDLIST") {
            p.end = true;
        } else if (l.compare(0, 11, "#EXT-X-KEY:") == 0) {
            if (attr(l, "METHOD") != "NONE") p.problem = "This station is encrypted";
        } else if (l.compare(0, 17, "#EXT-X-BYTERANGE:") == 0) {  // parts of one file: on-demand, not radio
            p.problem = "This station's playlist is in byte ranges, not supported yet";
        } else if (l.compare(0, 11, "#EXT-X-MAP:") == 0) {
            p.map = resolve(base, attr(l, "URI"));
        } else if (l[0] != '#') {
            if (want_uri) {
                if (take) p.variant = resolve(base, l);
                want_uri = take = false;
            } else if (!p.master) {
                p.segs.push_back(resolve(base, l));
            }
        }
    }
    if (p.target < 1) p.target = 1;
    if (p.master && p.variant.empty() && p.problem.empty()) p.problem = "The station offers no audio-only stream";
    return p;
}

// The audio in one segment, appended to `out`. MPEG-TS: the payload of the first PID whose PES is
// an MPEG audio stream (stream_id 0xC0-0xDF; AAC and MP3 both use it), so a segment without its
// PAT/PMT still plays, and timed ID3 metadata (stream 0xBD/0xFC) is left out. `pid` carries the
// choice from one segment to the next (start at -1). Packed audio: the frames, minus any ID3 tags.
inline void audio(const unsigned char* b, size_t n, std::vector<unsigned char>& out, int* pid) {
    if (n >= 188 && b[0] == 0x47) {
        size_t at = 0;
        while (at + 188 <= n) {
            const unsigned char* p = b + at;
            if (p[0] != 0x47) { at++; continue; }  // lost sync: find the next packet
            at += 188;
            const int id = ((p[1] & 0x1F) << 8) | p[2];
            const bool start = (p[1] & 0x40) != 0;
            const int afc = (p[3] >> 4) & 3;
            size_t off = 4;
            if (afc & 2) off += 1 + (size_t)p[4];
            if (!(afc & 1) || off >= 188) continue;
            const unsigned char* q = p + off;
            size_t len = 188 - off;
            if (start && len >= 9 && q[0] == 0 && q[1] == 0 && q[2] == 1) {
                if (*pid < 0 && q[3] >= 0xC0 && q[3] <= 0xDF) *pid = id;
                if (id != *pid) continue;
                const size_t h = 9 + (size_t)q[8];
                if (h > len) continue;
                q += h;
                len -= h;
            } else if (id != *pid) {
                continue;
            }
            out.insert(out.end(), q, q + len);
        }
        return;
    }
    size_t at = 0;
    while (n - at >= 10 && std::memcmp(b + at, "ID3", 3) == 0) {
        const unsigned char* t = b + at;
        const size_t size = ((size_t)(t[6] & 0x7F) << 21) | ((size_t)(t[7] & 0x7F) << 14) |
                            ((size_t)(t[8] & 0x7F) << 7) | (size_t)(t[9] & 0x7F);
        const size_t whole = 10 + size + ((t[5] & 0x10) ? 10 : 0);
        if (whole > n - at) return;
        at += whole;
    }
    out.insert(out.end(), b + at, b + n);
}

// The length of the ADTS frame at b (0 if b is not one).
inline size_t adts_len(const unsigned char* b, size_t n) {
    if (n < 7 || b[0] != 0xFF || (b[1] & 0xF6) != 0xF0) return 0;
    const size_t len = ((size_t)(b[3] & 3) << 11) | ((size_t)b[4] << 3) | (b[5] >> 5);
    return len >= 7 ? len : 0;
}

// True when the stream is ADTS AAC: two frames in a row within the first 4 KB. Anything else is
// handed to the MP3 decoder, which finds its own sync.
inline bool is_adts(const unsigned char* b, size_t n) {
    for (size_t i = 0; i < n && i < 4096; i++) {
        const size_t len = adts_len(b + i, n - i);
        if (len && i + len < n && adts_len(b + i + len, n - i - len)) return true;
    }
    return false;
}

// ── Fragmented MP4 ───────────────────────────────────────────────────────────────────────────────
// The init segment's moov holds the codec (stsd ▸ mp4a ▸ esds); each media segment is moof+mdat
// pairs, the trun listing the size of every sample (one AAC frame each). Each frame goes out with
// an ADTS header made from the AudioSpecificConfig, so the player decodes it like an ADTS station.
struct Mp4 {
    int codec = 0;                  // 0 none, 1 AAC (ADTS fields below), 2 MP3 (frames as they are)
    int profile = 0, freq = 0, chans = 0;
    std::string problem;
};

inline unsigned long be(const unsigned char* b, int n) {
    unsigned long v = 0;
    for (int i = 0; i < n; i++) v = v << 8 | b[i];
    return v;
}

// The first box of `type` among the boxes in [b, b+n): its payload in *pl / *pn, its start in *at.
inline bool box(const unsigned char* b, size_t n, const char* type, const unsigned char** pl, size_t* pn,
                const unsigned char** at = nullptr) {
    size_t o = 0;
    while (n - o >= 8) {
        size_t size = be(b + o, 4), h = 8;
        if (size == 1) {
            if (n - o < 16 || be(b + o + 8, 4)) return false;  // over 4 GB: not a radio segment
            size = be(b + o + 12, 4);
            h = 16;
        } else if (size == 0) {
            size = n - o;
        }
        if (size < h || size > n - o) return false;
        if (std::memcmp(b + o + 4, type, 4) == 0) {
            *pl = b + o + h;
            *pn = size - h;
            if (at) *at = b + o;
            return true;
        }
        o += size;
    }
    return false;
}

// An MPEG-4 descriptor's tag and length at *p (advanced past the header); false at the end.
inline bool desc(const unsigned char*& p, const unsigned char* end, int* tag, size_t* len) {
    if (end - p < 2) return false;
    *tag = *p++;
    *len = 0;
    for (int i = 0; i < 4 && p < end; i++) {
        const unsigned char c = *p++;
        *len = *len << 7 | (c & 0x7F);
        if (!(c & 0x80)) return (size_t)(end - p) >= *len;
    }
    return false;
}

inline Mp4 mp4_init(const unsigned char* b, size_t n) {
    Mp4 m;
    const unsigned char *p, *q;
    size_t pn, qn;
    const char* path[] = { "moov", "trak", "mdia", "minf", "stbl", "stsd" };
    p = b;
    pn = n;
    for (const char* t : path)
        if (!box(p, pn, t, &p, &pn)) { m.problem = "The station's MP4 stream has no audio track"; return m; }
    if (pn < 16) { m.problem = "The station's MP4 stream has no audio track"; return m; }
    const unsigned char* e = p + 8;  // the first sample entry, after version/flags and the count
    const std::string kind((const char*)e + 4, 4);
    if (kind == "enca") { m.problem = "This station is encrypted"; return m; }
    if (kind != "mp4a") { m.problem = "This station sends " + kind + " audio, not supported yet"; return m; }
    const size_t en = be(e, 4);
    if (en < 36 || en > pn - 8 || !box(e + 36, en - 36, "esds", &q, &qn) || qn < 4) {
        m.problem = "The station's MP4 audio names no codec";
        return m;
    }
    const unsigned char *d = q + 4, *end = q + qn;
    int tag;
    size_t len;
    int oti = 0;
    const unsigned char* asc = nullptr;
    size_t asc_n = 0;
    while (desc(d, end, &tag, &len)) {
        if (tag == 3) {  // ES_Descriptor: ES_ID, flags, then optional fields; its children follow
            if (len < 3) break;
            end = d + len;
            const int f = d[2];
            d += 3 + ((f & 0x80) ? 2 : 0) + ((f & 0x20) ? 2 : 0);
            if ((f & 0x40) && d < end) d += 1 + *d;
            continue;
        }
        if (tag == 4) {  // DecoderConfigDescriptor: objectTypeIndication, 12 more bytes, then children
            if (len < 13) break;
            oti = d[0];
            d += 13;
            continue;
        }
        if (tag == 5) { asc = d; asc_n = len; break; }
        d += len;
    }
    if (oti == 0x69 || oti == 0x6B) { m.codec = 2; return m; }
    if (oti != 0x40 || asc_n < 2) { m.problem = "The station's MP4 audio is not AAC or MP3"; return m; }
    // AudioSpecificConfig: object type (5 bits, 31 escapes to 6 more), rate index, channels; HE-AAC
    // signalled explicitly (5 or 29) adds the output rate and then the core type. ADTS carries the
    // core (AAC-LC) type and rate; the decoder finds SBR and PS in the frames on its own.
    size_t bit = 0;
    auto get = [&](int k) {
        unsigned v = 0;
        for (int i = 0; i < k; i++, bit++)
            v = v << 1 | (bit / 8 < asc_n ? (asc[bit / 8] >> (7 - bit % 8)) & 1 : 0);
        return v;
    };
    auto aot = [&] { unsigned a = get(5); return a == 31 ? 32 + get(6) : a; };
    unsigned type = aot(), freq = get(4);
    if (freq == 15) get(24);
    const unsigned chans = get(4);
    if (type == 5 || type == 29) {
        if (get(4) == 15) get(24);
        type = aot();
    }
    if (type < 1 || type > 4 || freq > 12 || chans < 1 || chans > 7) {
        m.problem = "This station's kind of AAC is not supported";
        return m;
    }
    m.codec = 1;
    m.profile = (int)type - 1;
    m.freq = (int)freq;
    m.chans = (int)chans;
    return m;
}

// The audio in one fMP4 media segment, appended to `out` (ADTS for AAC). A segment the parser
// cannot follow adds what it could read before that point.
inline void mp4_audio(const Mp4& m, const unsigned char* b, size_t n, std::vector<unsigned char>& out) {
    if (m.codec == 0) return;
    size_t o = 0;
    while (o < n) {
        const unsigned char *moof, *at, *traf, *t, *trun, *mdat;
        size_t moof_n, traf_n, tn, trun_n, mdat_n;
        if (!box(b + o, n - o, "moof", &moof, &moof_n, &at)) return;
        const size_t moof_at = (size_t)(at - b);
        o = (size_t)(moof - b) + moof_n;
        if (!box(b + o, n - o, "mdat", &mdat, &mdat_n)) return;
        o = (size_t)(mdat - b) + mdat_n;
        if (!box(moof, moof_n, "traf", &traf, &traf_n)) continue;
        size_t base = moof_at, defsize = 0;
        if (box(traf, traf_n, "tfhd", &t, &tn) && tn >= 8) {
            const unsigned long f = be(t + 1, 3);
            size_t k = 8;
            if (f & 0x01) { if (tn >= k + 8) base = be(t + k + 4, 4); k += 8; }
            if (f & 0x02) k += 4;
            if (f & 0x08) k += 4;
            if ((f & 0x10) && tn >= k + 4) defsize = be(t + k, 4);
        }
        // Every trun in the traf (Apple's packager writes several); one without a data offset carries
        // on where the last one ended.
        size_t pos = (size_t)(mdat - b), tr = 0;
        while (box(traf + tr, traf_n - tr, "trun", &trun, &trun_n)) {
            tr = (size_t)(trun - traf) + trun_n;
            if (trun_n < 8) continue;
            const unsigned long f = be(trun + 1, 3), count = be(trun + 4, 4);
            size_t k = 8;
            if (f & 0x01) { if (trun_n < k + 4) continue; pos = base + (size_t)(long)(int)be(trun + k, 4); k += 4; }
            if (f & 0x04) k += 4;
            const size_t each = 4 * (((f >> 8) & 1) + ((f >> 9) & 1) + ((f >> 10) & 1) + ((f >> 11) & 1));
            for (unsigned long i = 0; i < count; i++, k += each) {
                if (k + each > trun_n) return;
                size_t s = defsize;
                if (f & 0x200) s = be(trun + k + ((f & 0x100) ? 4 : 0), 4);
                if (!s || pos > n || s > n - pos) return;
                if (m.codec == 1) {
                    const size_t l = s + 7;
                    if (l > 0x1FFF) return;
                    const unsigned char h[7] = { 0xFF, 0xF1,
                                                 (unsigned char)(m.profile << 6 | m.freq << 2 | m.chans >> 2),
                                                 (unsigned char)((m.chans & 3) << 6 | l >> 11), (unsigned char)(l >> 3),
                                                 (unsigned char)((l & 7) << 5 | 0x1F), 0xFC };
                    out.insert(out.end(), h, h + 7);
                }
                out.insert(out.end(), b + pos, b + pos + s);
                pos += s;
            }
        }
    }
}

}  // namespace hls
#endif
