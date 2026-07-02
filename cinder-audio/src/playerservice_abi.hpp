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
#include <functional>

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

// ── Play-by-track: NodeTrackSequence recipe (libPlayerServiceClientUtil.so) ──────────────────
// RE (2026-07-02): SetTrackSequence takes shared_ptr<TrackSequence>; Sony ships a concrete
// NodeTrackSequence<UriInfo> we reuse. The ctor + JSON builder are exported. Confirmed from the
// C1 ctor disasm @0xbcec: a SINGLE primary vtable is stored at object+0, so the TrackSequence
// base sits at offset 0 — the shared_ptr upcast needs NO pointer adjustment (we use the aliasing
// shared_ptr ctor with a plain reinterpret_cast). The ctor writes fields up to +0xb0 (176 B), so
// we reserve 0x100 (256 B). The JSON node schema (from .rodata): {"uri":..,"format":ext,"children":[..]}.
namespace pst { namespace services { namespace playerservice { namespace util {

struct UriInfo;            // opaque value type (never constructed here)
enum class UpdateReason : int;
template <class T> class Node;   // opaque node tree (owned via unique_ptr from the JSON builder)

// NodeJsonUtil<UriInfo,UriInfoPolicy>::ConvJsonStringToNode(std::string const&) -> unique_ptr<Node>
// Its C1 ctor stores TWO adjacent vtable pointers (strd @+0/+4 in the disasm) — a small
// multiple-inheritance object. Real footprint is those 8 bytes; we reserve 0x20 for slack and
// let Sony's exported ctor/dtor own the layout.
class UriInfoPolicy;
template <class T, class P> class NodeJsonUtil {
public:
    NodeJsonUtil();
    ~NodeJsonUtil();
    std::unique_ptr<Node<T>> ConvJsonStringToNode(const std::string& json);
private:
    unsigned char _reserved[0x20];
};

// NodeTrackSequence<UriInfo> — reserved-size shell. We only ever construct it in a 0x100 buffer
// and destroy it via the exported dtor (through the shared_ptr deleter). No fields modeled: the
// real ctor lays them out; we just give it room.
template <class T> class NodeTrackSequence {
public:
    NodeTrackSequence(std::unique_ptr<Node<T>> node, int startIndex,
                      std::function<void(UpdateReason, int)> cb);
    ~NodeTrackSequence();
private:
    unsigned char _reserved[0x100];
};

} } } } // namespace pst::services::playerservice::util
