// airplay_meta_selftest.cpp — src/airplay_meta.h: shairport-sync's metadata pipe comes apart into
// the sender's title, artist, album and cover, whatever size the reads are. The stream is written
// here exactly as shairport-sync 5.5.2's metadata/pipe.c writes it (hex codes, base64 in 76-char
// lines), so a change in how we read it shows up as a failure, not as a blank Now Playing.
#include <cstdio>
#include <cstring>
#include <string>

#include "../src/airplay_meta.h"

static int fails;
#define CHECK(c, what) do { if (c) std::printf("  ok   %s\n", what); else { std::printf("  FAIL %s\n", what); ++fails; } } while (0)

static std::string b64(const std::string& in) {
    static const char t[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    std::string out;
    size_t i = 0;
    for (; i + 2 < in.size(); i += 3) {
        const unsigned v = (unsigned char)in[i] << 16 | (unsigned char)in[i + 1] << 8 | (unsigned char)in[i + 2];
        out += t[v >> 18]; out += t[(v >> 12) & 63]; out += t[(v >> 6) & 63]; out += t[v & 63];
    }
    if (i + 1 == in.size()) {
        const unsigned v = (unsigned char)in[i] << 16;
        out += t[v >> 18]; out += t[(v >> 12) & 63]; out += "==";
    } else if (i + 2 == in.size()) {
        const unsigned v = (unsigned char)in[i] << 16 | (unsigned char)in[i + 1] << 8;
        out += t[v >> 18]; out += t[(v >> 12) & 63]; out += t[(v >> 6) & 63]; out += '=';
    }
    return out;
}

// One item, as metadata_process() writes it: 57-byte groups -> 76-char base64 lines.
static std::string item(const char* type, const char* code, const std::string& data) {
    char head[160];
    std::snprintf(head, sizeof head, "<item><type>%x</type><code>%x</code><length>%u</length>",
                  apmeta::fourcc(type), apmeta::fourcc(code), (unsigned)data.size());
    std::string s = head;
    if (!data.empty()) {
        s += "\n<data encoding=\"base64\">\n";
        for (size_t i = 0; i < data.size(); i += 57) s += b64(data.substr(i, 57));
        s += "</data>";
    }
    return s + "</item>\n";
}

int main() {
    std::puts("== airplay metadata");
    std::string cover(5000, '\0');
    for (size_t i = 0; i < cover.size(); i++) cover[i] = char(i * 7 + 3);   // binary, every byte value
    const std::string stream = item("ssnc", "pbeg", "") + item("ssnc", "mdst", "1234") +
        item("core", "asal", "Bon Iver, Bon Iver") + item("core", "asar", "Bon Iver") +
        item("core", "minm", "Holocene") + item("ssnc", "mden", "1234") + item("ssnc", "PICT", cover);

    for (size_t chunk : {size_t(1), size_t(7), size_t(76), size_t(4096), stream.size()}) {
        apmeta::Parser p;
        apmeta::Song song;
        int names = 0, pictures = 0;
        for (size_t i = 0; i < stream.size(); i += chunk) {
            p.feed(stream.data() + i, std::min(chunk, stream.size() - i), [&](const apmeta::Item& it) {
                const int c = apmeta::apply(song, it);
                names += (c & apmeta::NAMES) != 0;
                pictures += (c & apmeta::PICTURE) != 0;
            });
        }
        char what[96];
        std::snprintf(what, sizeof what, "reads of %zu bytes: title, artist, album and the cover's bytes", chunk);
        CHECK(song.title == "Holocene" && song.artist == "Bon Iver" && song.album == "Bon Iver, Bon Iver" &&
              song.pict == cover && names == 3 && pictures == 1, what);
    }

    {
        apmeta::Parser p;
        apmeta::Song song;
        int last = 0;
        const std::string two = item("core", "minm", "One") + item("ssnc", "pbeg", "");
        p.feed(two.data(), two.size(), [&](const apmeta::Item& it) { last = apmeta::apply(song, it); });
        CHECK(song.title.empty() && song.pos_ms == -1 && last == (apmeta::NAMES | apmeta::PROGRESS),
              "a new session (pbeg) clears the names and the position, and says so");
    }
    {
        apmeta::Parser p;
        apmeta::Song song;
        int got = 0;
        // 10 s into a 200 s song, the stamps wrapping past 2^32 on the way (shairport-sync's
        // "start/now/end" at 44.1 kHz), then a new title that starts at 0.
        const unsigned a = 4294967000u, b = a + 441000u, c = a + 8820000u;
        char prgr[64];
        std::snprintf(prgr, sizeof prgr, "%u/%u/%u", a, b, c);
        const std::string ms("\x00\x03\x0d\x40", 4);   // 200000 ms
        const std::string items = item("ssnc", "prgr", prgr);
        p.feed(items.data(), items.size(), [&](const apmeta::Item& it) { got |= apmeta::apply(song, it); });
        CHECK(song.pos_ms == 10000 && song.dur_ms == 200000 && got == apmeta::PROGRESS,
              "prgr gives the position and length, across the RTP wrap");
        const std::string next = item("core", "astm", ms) + item("core", "minm", "Next");
        got = 0;
        p.feed(next.data(), next.size(), [&](const apmeta::Item& it) { got |= apmeta::apply(song, it); });
        CHECK(song.pos_ms == 0 && song.dur_ms == 200000 && got == (apmeta::NAMES | apmeta::PROGRESS),
              "astm gives the length; a new title starts at 0");
    }
    {
        apmeta::Parser p;
        apmeta::Song song;
        int got = 0;
        const std::string junk = std::string("garbage<item><type>zz</type></item>\n") + item("core", "minm", "Ok");
        p.feed(junk.data(), junk.size(), [&](const apmeta::Item& it) { got += apmeta::apply(song, it); });
        CHECK(song.title == "Ok" && got == (apmeta::NAMES | apmeta::PROGRESS), "junk before an item, and a broken item, are skipped");
    }
    {
        apmeta::Parser p;
        int items = 0;
        const std::string big = "<item><type>73736e63</type><code>50494354</code>" + std::string(apmeta::Parser::kMax + 10, 'A');
        p.feed(big.data(), big.size(), [&](const apmeta::Item&) { items++; });
        const std::string next = item("core", "minm", "After");
        apmeta::Song song;
        p.feed(next.data(), next.size(), [&](const apmeta::Item& it) { apmeta::apply(song, it); items++; });
        CHECK(items == 1 && song.title == "After", "an item that never ends is dropped at 4 MB, and reading goes on");
    }
    std::printf("airplay metadata: %s\n", fails ? "FAILED" : "all passed");
    return fails ? 1 : 0;
}
