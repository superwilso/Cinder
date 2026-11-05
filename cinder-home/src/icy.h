// icy.h — SHOUTcast/Icecast in-band metadata, for internet radio (main.cpp, radio_thread).
//
// A station asked for `Icy-MetaData: 1` answers with `icy-metaint: N`: after every N audio bytes
// comes one length byte L, then L*16 bytes of text ("StreamTitle='Artist - Song';", NUL-padded),
// then the next N audio bytes. IcySplit takes the stream in pieces of any size and gives back the
// audio bytes alone, and the title whenever a new one arrives. metaint 0 = no metadata: all audio.
// Tested by tools/icy_selftest.cpp.
#ifndef CINDER_ICY_H
#define CINDER_ICY_H
#include <cstddef>
#include <string>
#include <vector>

// "StreamTitle='Artist - Song';" -> "Artist - Song" (control characters become spaces).
inline std::string icy_title(const std::string& meta) {
    size_t a = meta.find("StreamTitle='");
    if (a == std::string::npos) return "";
    a += 13;
    size_t b = meta.find("';", a);
    std::string t = meta.substr(a, b == std::string::npos ? std::string::npos : b - a);
    for (char& c : t) if ((unsigned char)c < 0x20) c = ' ';
    return t;
}

struct IcySplit {
    long metaint;
    long until;         // audio bytes before the next length byte
    long left = -1;     // metadata bytes still to come; -1 = in audio
    std::string meta;
    explicit IcySplit(long m) : metaint(m > 0 ? m : 0), until(m > 0 ? m : 0) {}

    // Append the audio of b[0..n) to `audio`. Returns how many times the title changed (`title`
    // holds the latest); an empty or repeated title is not a change.
    int feed(const unsigned char* b, size_t n, std::vector<unsigned char>& audio, std::string& title) {
        int changed = 0;
        if (metaint == 0) { audio.insert(audio.end(), b, b + n); return 0; }
        size_t i = 0;
        while (i < n) {
            if (left < 0 && until > 0) {
                size_t k = (size_t)until < n - i ? (size_t)until : n - i;
                audio.insert(audio.end(), b + i, b + i + k);
                until -= (long)k;
                i += k;
            } else if (left < 0) {
                left = (long)b[i++] * 16;
                meta.clear();
                if (left == 0) { left = -1; until = metaint; }
            } else {
                size_t k = (size_t)left < n - i ? (size_t)left : n - i;
                meta.append((const char*)b + i, k);
                left -= (long)k;
                i += k;
                if (left == 0) {
                    left = -1;
                    until = metaint;
                    std::string t = icy_title(meta);
                    if (!t.empty() && t != title) { title = t; changed++; }
                }
            }
        }
        return changed;
    }
};
#endif
