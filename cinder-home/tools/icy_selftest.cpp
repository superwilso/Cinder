// icy_selftest.cpp — src/icy.h: a stream with in-band titles comes apart into audio and titles,
// whatever size the pieces arrive in.
#include <cstdio>
#include <cstring>
#include "../src/icy.h"

static int fails;
#define CHECK(c, what) do { if (c) std::printf("  ok   %s\n", what); else { std::printf("  FAIL %s\n", what); ++fails; } } while (0)

int main() {
    std::puts("== icy");
    const long M = 16;
    std::vector<unsigned char> audio_in, stream;
    const char* titles[] = { "StreamTitle='A - One';", "", "StreamTitle='B - Two';StreamUrl='';" };
    for (int block = 0; block < 6; block++) {
        for (long i = 0; i < M; i++) { unsigned char c = (unsigned char)(block * 31 + i); audio_in.push_back(c); stream.push_back(c); }
        const char* t = titles[block % 3];
        size_t len = std::strlen(t), blocks = (len + 15) / 16;
        stream.push_back((unsigned char)blocks);
        for (size_t k = 0; k < blocks * 16; k++) stream.push_back(k < len ? (unsigned char)t[k] : 0);
    }
    for (size_t piece : { (size_t)1, (size_t)3, (size_t)7, (size_t)16, (size_t)17, (size_t)1000 }) {
        IcySplit s(M);
        std::vector<unsigned char> out;
        std::string title;
        int changes = 0;
        for (size_t at = 0; at < stream.size(); at += piece) {
            size_t n = piece < stream.size() - at ? piece : stream.size() - at;
            changes += s.feed(stream.data() + at, n, out, title);
        }
        char what[96];
        std::snprintf(what, sizeof what, "pieces of %zu: the audio comes out whole and in order", piece);
        CHECK(out == audio_in, what);
        std::snprintf(what, sizeof what, "pieces of %zu: titles A, B, A, B (an empty block changes nothing)", piece);
        CHECK(changes == 4 && title == "B - Two", what);
    }
    IcySplit none(0);
    std::vector<unsigned char> out;
    std::string t;
    none.feed(stream.data(), 10, out, t);
    CHECK(out.size() == 10, "no metaint: every byte is audio");
    CHECK(icy_title("StreamTitle='x\x01y';") == "x y", "control characters become spaces");
    CHECK(icy_title("nothing") == "", "no StreamTitle, no title");
    std::printf("%d failed\n", fails);
    return fails != 0;
}
