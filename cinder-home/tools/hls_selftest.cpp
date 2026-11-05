// hls_selftest.cpp — src/hls.h: links resolve the way a browser would, master playlists choose an
// audio stream, media playlists list their segments, and the audio comes out of an MPEG-TS
// segment (with or without its program tables) and out of packed audio behind an ID3 tag.
// The playlists are copied from real stations' answers (BBC World Service and FIP, 2026-11-05).
#include <cstdio>
#include <cstring>
#include "../src/hls.h"

static int fails;
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
    CHECK(!hls::parse("#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4\"\na.m4s\n", bbc).problem.empty(), "fMP4: says why");
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

    std::printf("%s\n", fails ? "HLS SELFTEST FAILED" : "hls selftest: all passed");
    return fails ? 1 : 0;
}
