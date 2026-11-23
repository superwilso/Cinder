// airplay_meta.h — shairport-sync's metadata pipe, read.
//
// With `-M -g --metadata-pipename=PATH` the AirPlay receiver writes what the sender says about its
// song into a FIFO, one item at a time (shairport-sync 5.5.2, metadata/pipe.c):
//
//   <item><type>636f7265</type><code>6d696e6d</code><length>8</length>
//   <data encoding="base64">
//   SG9sb2NlbmU=</data></item>
//
// type and code are four-character codes in hex; data is base64 in lines of 76. The song's names
// come as type `core`: `minm` title, `asar` artist, `asal` album; its cover as `ssnc` `PICT`
// (JPEG or PNG bytes); `ssnc` `pbeg` starts a new play session. Where the song is: `ssnc` `prgr`,
// "start/now/end" RTP stamps at 44.1 kHz, when the sender says; its length alone: `core` `astm`
// (milliseconds, 4 bytes big-endian). Pure and host-tested
// (tools/airplay_meta_selftest.cpp); main.cpp's AirPlay reader feeds it.
#pragma once

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <string>

namespace apmeta {

constexpr uint32_t fourcc(const char* s) {
    return (uint32_t(uint8_t(s[0])) << 24) | (uint32_t(uint8_t(s[1])) << 16) |
           (uint32_t(uint8_t(s[2])) << 8) | uint32_t(uint8_t(s[3]));
}
constexpr uint32_t CORE = fourcc("core"), SSNC = fourcc("ssnc");
constexpr uint32_t MINM = fourcc("minm"), ASAR = fourcc("asar"), ASAL = fourcc("asal");
constexpr uint32_t PICT = fourcc("PICT"), PBEG = fourcc("pbeg");
constexpr uint32_t PRGR = fourcc("prgr"), ASTM = fourcc("astm");

// Standard base64; whitespace skipped. False on any other character.
inline bool b64_decode(const char* p, size_t n, std::string& out) {
    out.clear();
    uint32_t acc = 0;
    int bits = 0;
    for (size_t i = 0; i < n; i++) {
        const char c = p[i];
        int v;
        if (c >= 'A' && c <= 'Z') v = c - 'A';
        else if (c >= 'a' && c <= 'z') v = c - 'a' + 26;
        else if (c >= '0' && c <= '9') v = c - '0' + 52;
        else if (c == '+') v = 62;
        else if (c == '/') v = 63;
        else if (c == '=' || c == '\n' || c == '\r' || c == ' ') continue;
        else return false;
        acc = (acc << 6) | uint32_t(v);
        bits += 6;
        if (bits >= 8) {
            bits -= 8;
            out.push_back(char((acc >> bits) & 0xff));
        }
    }
    return true;
}

struct Item {
    uint32_t type = 0, code = 0;
    std::string data;
};

// Turns the byte stream into items, however the reads split it.
class Parser {
public:
    // A cover is the largest item; past this the stream is not what we think it is.
    static constexpr size_t kMax = 4u << 20;

    template <class F>
    void feed(const char* p, size_t n, F&& on_item) {
        buf_.append(p, n);
        for (;;) {
            const size_t s = buf_.find("<item>");
            if (s == std::string::npos) {
                // Keep a tail that could be the start of a split "<item>".
                if (buf_.size() > 8) buf_.erase(0, buf_.size() - 8);
                return;
            }
            if (s) buf_.erase(0, s);
            const size_t e = buf_.find("</item>");
            if (e == std::string::npos) {
                if (buf_.size() > kMax) buf_.clear();
                return;
            }
            Item it;
            if (parse(buf_.data(), e, it)) on_item(it);
            buf_.erase(0, e + 7);
        }
    }

private:
    std::string buf_;

    static bool hex_field(const char* s, size_t n, const char* open, const char* close, uint32_t& v) {
        const std::string str(s, n);
        const size_t a = str.find(open);
        if (a == std::string::npos) return false;
        const size_t b = str.find(close, a);
        if (b == std::string::npos) return false;
        const std::string h = str.substr(a + std::string(open).size(), b - a - std::string(open).size());
        if (h.empty() || h.size() > 8) return false;
        char* end = nullptr;
        v = uint32_t(std::strtoul(h.c_str(), &end, 16));
        return end && *end == '\0';
    }

    static bool parse(const char* s, size_t n, Item& it) {
        if (!hex_field(s, n, "<type>", "</type>", it.type)) return false;
        if (!hex_field(s, n, "<code>", "</code>", it.code)) return false;
        const std::string str(s, n);
        static const char kData[] = "<data encoding=\"base64\">";
        const size_t a = str.find(kData);
        if (a == std::string::npos) return true;   // an item without data (pbeg, pend, …)
        const size_t from = a + sizeof kData - 1;
        const size_t b = str.find("</data>", from);
        if (b == std::string::npos) return false;
        return b64_decode(str.data() + from, b - from, it.data);
    }
};

// What Now Playing shows.
struct Song {
    std::string title, artist, album;
    std::string pict;   // the cover's bytes; empty = none
    int64_t pos_ms = -1;  // where the song is, as of the item that said so; -1 = not said
    int64_t dur_ms = 0;   // its length; 0 = not known
};

enum : int { NAMES = 1, PICTURE = 2, PROGRESS = 4 };

// Applies one item; says what changed (NAMES, PICTURE, both, or 0).
inline int apply(Song& s, const Item& it) {
    if (it.type == CORE) {
        std::string* f = it.code == MINM ? &s.title : it.code == ASAR ? &s.artist
                       : it.code == ASAL ? &s.album : nullptr;
        if (it.code == ASTM && it.data.size() == 4) {
            const uint32_t ms = (uint32_t(uint8_t(it.data[0])) << 24) | (uint32_t(uint8_t(it.data[1])) << 16) |
                                (uint32_t(uint8_t(it.data[2])) << 8) | uint32_t(uint8_t(it.data[3]));
            s.dur_ms = ms;
            return PROGRESS;
        }
        if (!f || *f == it.data) return 0;
        *f = it.data;
        if (f == &s.title) {   // a new song starts at its beginning until the sender says otherwise
            s.pos_ms = 0;
            return NAMES | PROGRESS;
        }
        return NAMES;
    }
    if (it.type == SSNC && it.code == PRGR) {
        unsigned long a = 0, b = 0, c = 0;
        if (std::sscanf(it.data.c_str(), "%lu/%lu/%lu", &a, &b, &c) != 3) return 0;
        // RTP stamps wrap at 2^32: the differences, in 32 bits, are right across the wrap.
        s.pos_ms = int64_t(uint32_t(b - a)) * 1000 / 44100;
        s.dur_ms = int64_t(uint32_t(c - a)) * 1000 / 44100;
        return PROGRESS;
    }
    if (it.type == SSNC && it.code == PICT) {
        s.pict = it.data;
        return PICTURE;
    }
    if (it.type == SSNC && it.code == PBEG) {   // a new session: nothing is known yet
        const int was = (s.title.empty() && s.artist.empty() && s.album.empty() ? 0 : NAMES) |
                        (s.pict.empty() ? 0 : PICTURE) | (s.pos_ms < 0 && s.dur_ms == 0 ? 0 : PROGRESS);
        s = Song{};
        return was;
    }
    return 0;
}

}  // namespace apmeta
