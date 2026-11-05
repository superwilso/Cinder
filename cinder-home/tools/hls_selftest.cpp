// hls_selftest.cpp — src/hls.h: links resolve the way a browser would, master playlists choose an
// audio stream, media playlists list their segments, and the audio comes out of an MPEG-TS
// segment (with or without its program tables), out of packed audio behind an ID3 tag, and out of
// fragmented MP4 (built here box by box, as ffmpeg's and Apple's packagers lay them out) as ADTS.
// The playlists are copied from real stations' answers (BBC World Service and FIP, 2026-11-05).
#include <cstdio>
#include <cstring>
#include "../src/hls.h"

static int fails;
typedef std::vector<unsigned char> Bytes;
#define CHECK(c, what) do { if (c) std::printf("  ok   %s\n", what); else { std::printf("  FAIL %s\n", what); ++fails; } } while (0)

// One TS packet on `pid` carrying `n` bytes of `data`; `pes_id` > 0 starts a PES with that stream id.
// `af` pads with an adaptation field so the payload fills the packet exactly.
static void ts_packet(std::vector<unsigned char>& ts, int pid, int pes_id, const unsigned char* data, size_t n) {
    std::vector<unsigned char> payload;
    if (pes_id > 0) {
        const unsigned char h[] = { 0, 0, 1, (unsigned char)pes_id, 0, 0, 0x80, 0x80, 5, 0x21, 0, 1, 0, 1 };
        payload.assign(h, h + sizeof h);
    }
    payload.insert(payload.end(), data, data + n);
    const size_t room = 184;
    unsigned char p[188];
    std::memset(p, 0xFF, sizeof p);
    p[0] = 0x47;
    p[1] = (unsigned char)((pes_id > 0 ? 0x40 : 0) | ((pid >> 8) & 0x1F));
    p[2] = (unsigned char)(pid & 0xFF);
    if (payload.size() < room) {
        const size_t af = room - payload.size() - 1;  // adaptation field length byte + stuffing
        p[3] = 0x30;
        p[4] = (unsigned char)af;
        if (af) p[5] = 0;
        std::memcpy(p + 5 + af, payload.data(), payload.size());
    } else {
        p[3] = 0x10;
        std::memcpy(p + 4, payload.data(), room);
    }
    ts.insert(ts.end(), p, p + 188);
}

// An MP4 box: size, type, payload.
static Bytes mbox(const char* type, const Bytes& payload) {
    Bytes b = { 0, 0, 0, 0, (unsigned char)type[0], (unsigned char)type[1], (unsigned char)type[2], (unsigned char)type[3] };
    b.insert(b.end(), payload.begin(), payload.end());
    const size_t n = b.size();
    b[0] = (unsigned char)(n >> 24); b[1] = (unsigned char)(n >> 16); b[2] = (unsigned char)(n >> 8); b[3] = (unsigned char)n;
    return b;
}
static Bytes cat(std::initializer_list<Bytes> parts) {
    Bytes b;
    for (const Bytes& p : parts) b.insert(b.end(), p.begin(), p.end());
    return b;
}
static Bytes u32(unsigned long v) { return { (unsigned char)(v >> 24), (unsigned char)(v >> 16), (unsigned char)(v >> 8), (unsigned char)v }; }

// An init segment whose one track is `entry` ("mp4a" with this AudioSpecificConfig, or another codec).
static Bytes init_seg(const char* entry, const Bytes& asc, int oti = 0x40) {
    Bytes dsi = { 5, (unsigned char)asc.size() };
    dsi.insert(dsi.end(), asc.begin(), asc.end());
    Bytes dcd = { 4, 0x80, 0x80, 0x80, (unsigned char)(13 + dsi.size()), (unsigned char)oti, 0x15, 0, 0, 0,
                  0, 1, 0xF4, 0, 0, 1, 0xF4, 0 };  // a four-byte length, as ffmpeg writes it
    dcd.insert(dcd.end(), dsi.begin(), dsi.end());
    Bytes es = { 3, (unsigned char)(3 + dcd.size() + 3), 0, 1, 0 };
    es.insert(es.end(), dcd.begin(), dcd.end());
    es.insert(es.end(), { 6, 1, 2 });
    const Bytes esds = mbox("esds", cat({ u32(0), es }));
    const Bytes fields = { 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 16, 0, 0, 0, 0, 0xAC, 0x44, 0, 0 };
    const Bytes stsd = mbox("stsd", cat({ u32(0), u32(1), mbox(entry, cat({ fields, esds })) }));
    return cat({ mbox("ftyp", { 'i', 's', 'o', '6', 0, 0, 0, 0 }),
                 mbox("moov", cat({ mbox("mvhd", Bytes(100)),
                                    mbox("trak", cat({ mbox("tkhd", Bytes(84)),
                                                       mbox("mdia", mbox("minf", mbox("stbl", stsd))) })) })) });
}

// One moof+mdat holding `frames`. sizes_in_trun: the trun lists each size and the data offset (ffmpeg);
// otherwise tfhd gives one default size and the data starts the mdat.
static Bytes fragment(const std::vector<Bytes>& frames, bool sizes_in_trun) {
    Bytes trun, tfhd;
    if (sizes_in_trun) {
        tfhd = mbox("tfhd", cat({ u32(0x020000), u32(1) }));
        trun = cat({ u32(0x000201), u32(frames.size()), u32(0) });
        for (const Bytes& f : frames) trun = cat({ trun, u32(f.size()) });
    } else {
        tfhd = mbox("tfhd", cat({ u32(0x020010), u32(1), u32(frames[0].size()) }));
        trun = cat({ u32(0), u32(frames.size()) });
    }
    Bytes moof = mbox("moof", cat({ mbox("mfhd", cat({ u32(0), u32(7) })), mbox("traf", cat({ tfhd, mbox("trun", trun) })) }));
    if (sizes_in_trun) {  // data_offset: from the moof's start to the first frame
        const Bytes off = u32(moof.size() + 8);
        const size_t at = moof.size() - 4 * frames.size() - 4;
        std::copy(off.begin(), off.end(), moof.begin() + (long)at);
    }
    Bytes data;
    for (const Bytes& f : frames) data = cat({ data, f });
    return cat({ moof, mbox("mdat", data) });
}

int main() {
    std::puts("== hls: links");
    const std::string bbc = "https://as-hls-ww-live.akamaized.net/pool_87948813/live/ww/bbc_world_service/"
                            "bbc_world_service.isml/bbc_world_service-audio%3d96000.norewind.m3u8";
    CHECK(hls::resolve(bbc, "bbc_world_service-audio=96000-280292706.ts") ==
              "https://as-hls-ww-live.akamaized.net/pool_87948813/live/ww/bbc_world_service/bbc_world_service.isml/"
              "bbc_world_service-audio=96000-280292706.ts", "relative to the playlist's folder");
    const std::string fip = "https://stream.radiofrance.fr/fip/fip_hifi.m3u8?id=radiofrance";
    CHECK(hls::resolve(fip, "/accs3/fip/a.ts?id=radiofrance") == "https://stream.radiofrance.fr/accs3/fip/a.ts?id=radiofrance",
          "host-absolute path");
    CHECK(hls::resolve(fip, "fip_lofi.m3u8?id=radiofrance") == "https://stream.radiofrance.fr/fip/fip_lofi.m3u8?id=radiofrance",
          "relative, the playlist's query left behind");
    CHECK(hls::resolve(fip, "//cdn.example.org/x.ts") == "https://cdn.example.org/x.ts", "scheme-relative");
    CHECK(hls::resolve("http://example.org", "a.ts") == "http://example.org/a.ts", "a bare host");
    CHECK(hls::resolve(fip, "http://other.example/y.ts") == "http://other.example/y.ts", "a full URL is kept");

    std::puts("== hls: playlists");
    hls::Playlist m = hls::parse(
        "#EXTM3U\n#EXT-X-VERSION:3\n"
        "#EXT-X-STREAM-INF:PROGRAM-ID=0,BANDWIDTH=107000,AVERAGE-BANDWIDTH=105500,CODECS=\"mp4a.40.2\"\n"
        "fip_lofi.m3u8?id=radiofrance\n"
        "#EXT-X-STREAM-INF:PROGRAM-ID=0,BANDWIDTH=280000,AVERAGE-BANDWIDTH=268000,CODECS=\"mp4a.40.2\"\n"
        "fip_hifi.m3u8?id=radiofrance\n"
        "#EXT-X-STREAM-INF:PROGRAM-ID=0,BANDWIDTH=185000,AVERAGE-BANDWIDTH=178000,CODECS=\"mp4a.40.2\"\n"
        "fip_midfi.m3u8?id=radiofrance\n"
        "#EXT-X-STREAM-INF:BANDWIDTH=900000,CODECS=\"avc1.4d401f,mp4a.40.2\"\n"
        "video.m3u8\n", fip);
    CHECK(m.master && m.variant == "https://stream.radiofrance.fr/fip/fip_hifi.m3u8?id=radiofrance",
          "master: highest audio-only bitrate, video passed over");
    CHECK(m.problem.empty(), "master: nothing wrong");
    hls::Playlist v = hls::parse("#EXTM3U\r\n#EXT-X-STREAM-INF:BANDWIDTH=1,CODECS=\"avc1\"\r\nv.m3u8\r\n", fip);
    CHECK(v.master && v.variant.empty() && !v.problem.empty(), "master with only video: says why");

    hls::Playlist p = hls::parse(
        "#EXTM3U\n#EXT-X-VERSION:3\n## Created with Unified Streaming Platform  (version=1.13.5-30103)\n"
        "#EXT-X-MEDIA-SEQUENCE:280292706\n#EXT-X-INDEPENDENT-SEGMENTS\n#EXT-X-TARGETDURATION:6\n"
        "#USP-X-TIMESTAMP-MAP:MPEGTS=778323360,LOCAL=2026-11-05T10:08:32Z\n"
        "#EXT-X-PROGRAM-DATE-TIME:2026-11-05T10:08:32Z\n"
        "#EXTINF:6.4, no desc\nbbc_world_service-audio=96000-280292706.ts\n"
        "#EXTINF:6.4, no desc\nbbc_world_service-audio=96000-280292707.ts\n", bbc);
    CHECK(!p.master && p.seq == 280292706 && p.target == 6 && !p.end, "media: sequence and target");
    CHECK(p.segs.size() == 2 && p.segs[1].find("280292707.ts") != std::string::npos, "media: segments in order");
    CHECK(hls::parse("#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"k\"\na.ts\n", bbc).problem == "This station is encrypted",
          "encrypted: refused");
    CHECK(hls::parse("#EXTM3U\n#EXT-X-KEY:METHOD=NONE\na.ts\n", bbc).problem.empty(), "METHOD=NONE: fine");
    const hls::Playlist f4 = hls::parse("#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4\"\na.m4s\n", fip);
    CHECK(f4.problem.empty() && f4.map == "https://stream.radiofrance.fr/fip/init.mp4" && f4.segs.size() == 1,
          "fMP4: the init segment is resolved");
    CHECK(!hls::parse("#EXTM3U\n#EXT-X-MAP:URI=\"m.mp4\",BYTERANGE=\"616@0\"\n#EXTINF:6,\n#EXT-X-BYTERANGE:121090@616\nm.mp4\n", fip)
               .problem.empty(), "byte ranges: says why");
    CHECK(hls::parse("#EXTM3U\na.aac\n#EXT-X-ENDLIST\n", bbc).end, "ENDLIST seen");

    std::puts("== hls: audio out of segments");
    // An ADTS-like payload split across PES packets, with a second stream (timed ID3, 0xBD) mixed in.
    std::vector<unsigned char> es;
    for (int i = 0; i < 700; i++) es.push_back((unsigned char)(i * 7 + 1));
    for (int with_tables = 0; with_tables < 2; with_tables++) {
        std::vector<unsigned char> ts;
        if (with_tables) { const unsigned char pat[] = { 0, 0, 0xB0, 13 }; ts_packet(ts, 0, 0, pat, sizeof pat); }
        const unsigned char id3[] = { 'I', 'D', '3', 4 };
        ts_packet(ts, 0x102, 0xBD, id3, sizeof id3);
        ts_packet(ts, 0x101, 0xC0, es.data(), 170);                // first PES: header + 170 bytes
        ts_packet(ts, 0x101, 0, es.data() + 170, 184);             // continuation, full
        ts_packet(ts, 0x102, 0xBD, id3, sizeof id3);
        ts_packet(ts, 0x101, 0, es.data() + 354, 100);             // continuation, padded
        ts_packet(ts, 0x101, 0xC0, es.data() + 454, 170);          // second PES
        ts_packet(ts, 0x101, 0, es.data() + 624, 76);
        std::vector<unsigned char> out;
        int pid = -1;
        hls::audio(ts.data(), ts.size(), out, &pid);
        CHECK(pid == 0x101 && out == es, with_tables ? "TS with tables: the audio stream, whole" : "TS without tables: the audio stream, whole");
    }
    {
        std::vector<unsigned char> seg = { 'I', 'D', '3', 4, 0, 0, 0, 0, 0, 3, 'a', 'b', 'c' };
        seg.insert(seg.end(), es.begin(), es.begin() + 50);
        std::vector<unsigned char> out;
        int pid = -1;
        hls::audio(seg.data(), seg.size(), out, &pid);
        CHECK(out.size() == 50 && std::memcmp(out.data(), es.data(), 50) == 0, "packed audio: the ID3 tag is dropped");
    }

    std::puts("== hls: AAC or MP3");
    // Two real ADTS headers (Radio Paradise's aac-128, 2026-11-05: FF F1 50 80 2A E1 = 343 bytes).
    std::vector<unsigned char> a(800, 0);
    const unsigned char h[] = { 0xFF, 0xF1, 0x50, 0x80, 0x2A, 0xE1, 0x6C };
    a.insert(a.begin() + 3, h, h + 7);
    const size_t len = hls::adts_len(a.data() + 3, a.size() - 3);
    CHECK(len == 343, "ADTS frame length");
    std::memcpy(a.data() + 3 + len, h, 7);
    CHECK(hls::is_adts(a.data(), a.size()), "two ADTS frames in a row: AAC");
    std::vector<unsigned char> one(a.begin(), a.begin() + 3 + (long)len);
    CHECK(!hls::is_adts(one.data(), one.size()), "a lone sync word is not enough");
    const unsigned char mp3[] = { 0xFF, 0xFB, 0x90, 0x64, 0, 0, 0, 0, 0xFF, 0xFB, 0x90, 0x64 };
    CHECK(!hls::is_adts(mp3, sizeof mp3), "MP3 frames are not ADTS");

    std::puts("== hls: fragmented MP4");
    {
        const Bytes lc = init_seg("mp4a", { 0x12, 0x10 });  // AAC-LC, 44.1 kHz, stereo
        const hls::Mp4 m = hls::mp4_init(lc.data(), lc.size());
        CHECK(m.problem.empty() && m.codec == 1 && m.profile == 1 && m.freq == 4 && m.chans == 2, "AAC-LC 44.1 kHz stereo");
        const hls::Mp4 he = hls::mp4_init(init_seg("mp4a", { 0x2B, 0x92, 0x08, 0x00 }).data(),
                                          init_seg("mp4a", { 0x2B, 0x92, 0x08, 0x00 }).size());
        CHECK(he.codec == 1 && he.profile == 1 && he.freq == 7 && he.chans == 2, "HE-AAC (explicit SBR): its AAC-LC core at 22.05 kHz");
        const Bytes v2 = init_seg("mp4a", { 0xEB, 0x09, 0x88, 0x00 });  // HE-AAC v2 (29), 24 kHz core, 1 ch, 48 kHz out
        const hls::Mp4 ps = hls::mp4_init(v2.data(), v2.size());
        CHECK(ps.codec == 1 && ps.profile == 1 && ps.freq == 6 && ps.chans == 1, "HE-AAC v2: core 24 kHz mono");
        const Bytes mp3 = init_seg("mp4a", {}, 0x6B);
        CHECK(hls::mp4_init(mp3.data(), mp3.size()).codec == 2, "MP3 in MP4");
        const Bytes opus = init_seg("Opus", {});
        CHECK(hls::mp4_init(opus.data(), opus.size()).problem == "This station sends Opus audio, not supported yet", "Opus: says why");
        const Bytes enc = init_seg("enca", { 0x12, 0x10 });
        CHECK(hls::mp4_init(enc.data(), enc.size()).problem == "This station is encrypted", "encrypted: says why");
        const Bytes junk = { 1, 2, 3, 4, 5, 6, 7, 8, 9 };
        CHECK(!hls::mp4_init(junk.data(), junk.size()).problem.empty(), "not MP4: says why");

        const std::vector<Bytes> fr = { Bytes(371, 0x21), Bytes(380, 0x42), Bytes(366, 0x63) };
        const Bytes seg = cat({ mbox("styp", { 'm', 's', 'd', 'h' }), fragment(fr, true), fragment({ Bytes(300, 0x77), Bytes(300, 0x78) }, false) });
        Bytes out;
        hls::mp4_audio(m, seg.data(), seg.size(), out);
        size_t at = 0, frames = 0;
        bool same = true;
        const unsigned char fill[] = { 0x21, 0x42, 0x63, 0x77, 0x78 };
        const size_t sizes[] = { 371, 380, 366, 300, 300 };
        while (at < out.size() && frames < 5) {
            const size_t l = hls::adts_len(out.data() + at, out.size() - at);
            if (l != sizes[frames] + 7 || out[at + 2] != (1 << 6 | 4 << 2 | 0) || (out[at + 3] >> 6) != 2) { same = false; break; }
            for (size_t i = 7; i < l; i++) same = same && out[at + i] == fill[frames];
            at += l;
            frames++;
        }
        CHECK(same && frames == 5 && at == out.size(), "five frames out of two fragments, each behind its ADTS header");
        CHECK(hls::is_adts(out.data(), out.size()), "and the player takes it for ADTS");
        {  // several truns in one traf, as Apple's packager writes them: the second without a data offset
            Bytes moof = fragment({ Bytes(200, 0x11) }, true);
            const Bytes trun2 = mbox("trun", cat({ u32(0x000200), u32(1), u32(150) }));
            Bytes mdat = mbox("mdat", cat({ Bytes(200, 0x11), Bytes(150, 0x22) }));
            Bytes mf(moof.begin(), moof.end() - 8 - 200);  // the moof alone
            mf = cat({ mf, trun2 });
            auto grow = [](Bytes& b, size_t at, size_t by) { size_t v = hls::be(b.data() + at, 4) + by;
                b[at] = (unsigned char)(v >> 24); b[at + 1] = (unsigned char)(v >> 16); b[at + 2] = (unsigned char)(v >> 8); b[at + 3] = (unsigned char)v; };
            grow(mf, 0, trun2.size());                 // moof
            grow(mf, 8 + 16, trun2.size());            // traf, after mfhd
            const size_t off = 8 + 16 + 8 + 16 + 8 + 8;  // the first trun's data offset
            grow(mf, off, trun2.size());
            const Bytes two = cat({ mf, mdat });
            Bytes o2;
            hls::mp4_audio(m, two.data(), two.size(), o2);
            CHECK(o2.size() == 7 + 200 + 7 + 150 && o2[7] == 0x11 && o2[7 + 200 + 7] == 0x22, "two truns in one traf");
        }
        Bytes cut;
        hls::mp4_audio(m, seg.data(), seg.size() - 400, cut);
        CHECK(cut.size() == 3 * 7 + 371 + 380 + 366, "a cut-off segment gives the whole frames before the cut");
        Bytes none;
        hls::mp4_audio(hls::Mp4(), seg.data(), seg.size(), none);
        CHECK(none.empty(), "no codec, no output");
    }

    std::printf("%s\n", fails ? "HLS SELFTEST FAILED" : "hls selftest: all passed");
    return fails ? 1 : 0;
}
