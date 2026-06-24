// player_shim.cpp — C++ implementation of the cinder_audio.h C ABI. Wraps Sony's exported
// PlayerService client (libPlayerServiceClient.so). Built with clang -stdlib=libc++ and
// linked against the device libPlayerServiceClient/.../libc++ (same toolchain as cinder-home;
// see ../README.md). STATUS: skeleton — ABI ground-truthed, not yet compiled/run (needs the
// libc++ toolchain + device libs, identical blockers to cinder-home).
#include "playerservice_abi.hpp"
#include "cinder_audio.h"

#include <memory>
#include <string>
#include <cstring>

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
    // PlayStatus.uri is a libc++ std::string at offset +0x6c (RE'd). Reading it this way is
    // LAYOUT-FRAGILE — must be validated on-device before trusting (the rest of PlayStatus's
    // layout is still unmapped; see playerservice_abi.hpp). If the offset is wrong this is UB,
    // so this read is the one thing to confirm first on hardware.
    const std::string* uri =
        reinterpret_cast<const std::string*>(reinterpret_cast<const char*>(&st) + 0x6c);
    int n = static_cast<int>(uri->size());
    if (n >= cap) n = cap - 1;
    std::memcpy(buf, uri->data(), static_cast<size_t>(n));
    buf[n] = '\0';
    return n;
}

} // extern "C"
