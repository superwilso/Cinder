// hls.h — HLS (HTTP Live Streaming) for internet radio (main.cpp, radio_hls_thread): playlists,
// MPEG-TS audio and the AAC/MP3 sniff. Pure functions, no I/O; tested by tools/hls_selftest.cpp.
//
// A station's .m3u8 is either a master playlist (#EXT-X-STREAM-INF lines, one per bitrate) or a
// media playlist (#EXTINF segments, #EXT-X-MEDIA-SEQUENCE numbering the first). Live radio keeps
// adding segments at the end; the player fetches each new one and plays the audio inside. Segments
// are MPEG-TS (188-byte packets) or "packed audio" (ADTS or MP3 frames, perhaps behind an ID3 tag).
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
        } else if (l.compare(0, 11, "#EXT-X-MAP:") == 0) {
            p.problem = "This station sends MP4 audio, not supported yet";
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

}  // namespace hls
#endif
