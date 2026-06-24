// playerservice_abi.hpp — hand-written declarations of Sony's exported PlayerService
// client API (libPlayerServiceClient.so), reconstructed from `nm -D | c++filt` on the
// extracted device lib (2026-06-24). No SDK headers exist; these declarations exist only
// so our calls link against the device .so with the correct C++/libc++ mangling.
//
// ABI: the std types are libc++ (std::__1::shared_ptr). Build with clang -stdlib=libc++.
// All PlayController/PlayerService methods below are NON-virtual exported member functions
// (called directly by mangled symbol — no vtable to reproduce, unlike easel::ApplicationBase).
//
// VERIFIED exported signatures (demangled):
//   pst::services::playerservice::PlayerService::GetInstance()
//   pst::services::playerservice::PlayerService::getPlayController(char const*)
//   PlayController::Connect(PlayEventListener*)            // accepts NULL -> poll mode
//   PlayController::Disconnect()
//   PlayController::ChangePlayState(playstate_t)           // 0,1,2 valid; 3-6 rejected
//   PlayController::NextTrack() / PrevTrack(PrevTrackOption const*)
//   PlayController::NextGroup() / PrevGroup(PrevGroupOption const*)   // album-level = shuffle-by-album
//   PlayController::SeekTime(media_origin_t, int)
//   PlayController::GetCurrentStatus(PlayStatus&)          // poll snapshot (no listener)
//   PlayController::SetTrackSequence(shared_ptr<TrackSequence> const&)
#pragma once
#include <memory>

namespace pst {
namespace playservice {

// ChangePlayState argument. Exact 0/1/2 <-> stop/play/pause mapping TBC on device
// (the wrapper rejects 3..6). We name them by best-known meaning; calibrate on device.
enum class playstate_t : int { Stop = 0, Play = 1, Pause = 2 };

// SeekTime origin (begin vs current). Calibrate exact values on device.
enum class media_origin_t : int { Begin = 0, Current = 1 };

// Opaque types we only pass by pointer/ref and never construct here.
class PlayEventListener;   // Connect() accepts NULL -> we poll instead of implementing this
struct PrevTrackOption;
struct PrevGroupOption;
class  TrackSequence;

// PlayStatus — a plain data struct filled by GetCurrentStatus(). It has NO exported
// accessors; fields are read by offset. Only the track URI offset is known so far
// (std::string @ +0x6c). The playstate / current-ms / total-ms offsets still need a
// Ghidra pass on GetCurrentStatus()/OnPlayStatusUpdated() — DO NOT read other fields until
// then. We model it as an opaque blob sized generously; the shim copies out the URI only.
struct PlayStatus {
    unsigned char _opaque[256];
};

} // namespace playservice

namespace services {
namespace playerservice {

class PlayController {
public:
    int  Connect(pst::playservice::PlayEventListener* listener);  // NULL = poll mode
    int  Disconnect();
    bool IsConnected() const;
    int  ChangePlayState(pst::playservice::playstate_t state);
    int  NextTrack();
    int  PrevTrack(const pst::playservice::PrevTrackOption* opt);
    int  NextGroup();
    int  PrevGroup(const pst::playservice::PrevGroupOption* opt);
    int  SeekTime(pst::playservice::media_origin_t origin, int ms);
    int  GetCurrentStatus(pst::playservice::PlayStatus& out);
    int  SetTrackSequence(const std::shared_ptr<pst::playservice::TrackSequence>& seq);
};

class PlayerService {
public:
    static PlayerService* GetInstance();
    std::shared_ptr<PlayController> getPlayController(const char* name);
};

} // namespace playerservice
} // namespace services
} // namespace pst
