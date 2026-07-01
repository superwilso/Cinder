// player_shim.cpp — C++ implementation of the cinder_audio.h C ABI. Wraps Sony's exported
// PlayerService client (libPlayerServiceClient.so). Built with clang -stdlib=libc++ and
// linked against the device libPlayerServiceClient/.../libc++ (same toolchain as cinder-home;
// see ../README.md). STATUS: built + linked into cinder-home/cinder-probe; control calls
// RE-verified; PlayStatus is read for the URI (+0x6c) — position/duration offsets pending the
// on-device hex dump (cinder_audio_dump_status, used by cinder-probe --discover).
#include "playerservice_abi.hpp"
#include "cinder_audio.h"

#include <memory>
#include <string>
#include <cstring>
#include <cstdio>

namespace ps = pst::services::playerservice;
namespace pl = pst::playservice;

namespace {
std::shared_ptr<ps::PlayController> g_ctrl;

inline int change_state(pl::playstate_t s) {
    if (!g_ctrl) return -1;
    return g_ctrl->ChangePlayState(s);
}
} // namespace

extern "C" {

int cinder_audio_init(const char* name) {
    ps::PlayerService* svc = ps::PlayerService::GetInstance();
    if (!svc) return -1;
    g_ctrl = svc->getPlayController(name ? name : "cinder");
    if (!g_ctrl) return -2;
    // NULL listener => poll mode (we read state via GetCurrentStatus, no PlayEventListener).
    g_ctrl->Connect(nullptr);
    return 0;
}

void cinder_audio_shutdown(void) {
    if (g_ctrl) g_ctrl->Disconnect();
    g_ctrl.reset();
}

int cinder_audio_play(void)  { return change_state(pl::playstate_t::Play); }
int cinder_audio_pause(void) { return change_state(pl::playstate_t::Pause); }
int cinder_audio_stop(void)  { return change_state(pl::playstate_t::Stop); }

int cinder_audio_next_track(void) { return g_ctrl ? g_ctrl->NextTrack() : -1; }
int cinder_audio_prev_track(void) { return g_ctrl ? g_ctrl->PrevTrack(nullptr) : -1; }
int cinder_audio_next_group(void) { return g_ctrl ? g_ctrl->NextGroup() : -1; }
int cinder_audio_prev_group(void) { return g_ctrl ? g_ctrl->PrevGroup(nullptr) : -1; }

int cinder_audio_seek_ms(int ms) {
    if (!g_ctrl) return -1;
    return g_ctrl->SeekTime(pl::media_origin_t::Begin, ms);
}

int cinder_audio_current_uri(char* buf, int cap) {
    if (!g_ctrl || !buf || cap <= 0) return -1;
    pl::PlayStatus st{};
    if (g_ctrl->GetCurrentStatus(st) != 0) return -2;
    // PlayStatus.uri is a real libc++ std::string at offset +0x6c — CONFIRMED by RE'ing
    // PlayerService::ConverPlayStatus (analysis/RE_playerservice_sound.md §1: dst+0x6c =
    // std::string assigned from the wire URI). GetCurrentStatus fills it, so it's a valid string
    // (empty SSO if no track; heap-allocated if the URI > ~22 chars).
    std::string* uri =
        reinterpret_cast<std::string*>(reinterpret_cast<char*>(&st) + 0x6c);
    int n = static_cast<int>(uri->size());
    if (n >= cap) n = cap - 1;
    if (n < 0) n = 0;
    std::memcpy(buf, uri->data(), static_cast<size_t>(n));
    buf[n] = '\0';
    // We model PlayStatus as an opaque blob (no dtor runs at scope exit), so a long (heap)
    // URI string would LEAK every poll (~1/s). Destruct it explicitly to free that heap.
    // (Safe for the empty-SSO case too — no-op.)
    uri->~basic_string();
    return n;
}

int cinder_audio_dump_status(char* buf, int cap) {
    if (!g_ctrl || !buf || cap < 8) return -1;
    pl::PlayStatus st{};
    if (g_ctrl->GetCurrentStatus(st) != 0) return -2;
    // Hex-dump the first 128 bytes of the filled struct (the int fields — playstate/position/
    // duration/track — live in 0..0x6c; the URI std::string is at +0x6c). 16 bytes per line with
    // the offset, so position/duration ms values can be matched by eye on device.
    const unsigned char* p = reinterpret_cast<const unsigned char*>(&st);
    int n = 0;
    for (int row = 0; row < 128; row += 16) {
        if (n > cap - 60) break;
        n += std::snprintf(buf + n, cap - n, "+0x%02x:", row);
        for (int i = 0; i < 16; ++i) n += std::snprintf(buf + n, cap - n, " %02x", p[row + i]);
        if (n < cap - 1) buf[n++] = '\n';
    }
    buf[n < cap ? n : cap - 1] = '\0';
    // Free the heap URI string at +0x6c (same leak fix as current_uri).
    auto* uri = reinterpret_cast<std::string*>(reinterpret_cast<char*>(&st) + 0x6c);
    uri->~basic_string();
    return n;
}

} // extern "C"
