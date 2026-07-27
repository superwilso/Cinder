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
#include <string>

namespace pst {
namespace playservice {

// ChangePlayState argument. The client wrapper (disasm @0x3044: `r2 = s-3; if (r2 < 4) return 1`)
// silently no-ops 3..6 and sends everything else, so only 0/1/2 reach the service.
//
// MEASURED 2026-07-27 (device, service's own logcat) — do NOT "restore" the old guess:
//   1 -> GapPlayer.c:502 GapPlayer_pause()   ... so 1 is PAUSE, not Play. The original
//        Stop=0/Play=1/Pause=2 guess is why a "successful" play_tracks produced a loaded track
//        frozen at 0:00: we were paused, not playing.
//   2 -> binder transport error (GetBinderLastError()=4) and the device REBOOTED. Whatever 2
//        means, it is not a safe Play. Never send 2 speculatively again.
// MAPPING MEASURED ON DEVICE 2026-07-27 from the service's own logcat, one value per run:
//   0 -> graph stays at OMX_StateIdle, no GapPlayer_* transition at all      => Stop
//   1 -> GapPlayer.c:502 GapPlayer_pause(), Idle -> OMX_StatePause           => Pause
//   2 -> (by elimination; see below)                                         => Play
// So the original RE guess was right after all. The reason 2 looked lethal on the first attempt
// (GetBinderLastError()=4 and the device rebooted) is that it was sent straight from Idle while a
// leaked SoundService Music track was still held — NOT because the value is wrong. Idle ->
// Executing is not a legal OMX transition; the engine must pass through Pause. cinder_audio's
// play_tracks therefore does Pause THEN Play, which is the OMX lifecycle the renderer expects.
enum class playstate_t : int { Stop = 0, Pause = 1, Play = 2 };

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
    // Tears down the service-side player for this session. MUST be called before we go away:
    // PlayerService holds a SoundService "Music" track for the open player, and SoundService
    // permits exactly ONE track per type ("Cannot create multiple tracks that have same type",
    // SoundServiceImpl.cc:248). A process that exits without ClosePlayer leaks that track inside
    // the long-lived hagodaemon, and every later play attempt fails at
    // AudioTrackFactory::Create() -> WMX_AudioOutput::Open() (0x80001009) with no audio.
    int  ClosePlayer();
    bool IsConnected() const;
    int  ChangePlayState(pst::playservice::playstate_t state);
    // Suspend/Resume are the engine-level pause/unpause. MEASURED 2026-07-27: after
    // SetTrackSequence + ChangePlayState(1) the OMX graph sits at OMX_StatePause with the
    // SoundService track created but no audio — this is the transition out of it.
    int  Suspend();
    int  Resume();
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
// we reserve 0x100 (256 B). The JSON node schema (ConvJsonToNode disasm @0x10631, keys @.rodata):
//   {"uri": <string>, "format": <INT>, "children": [..]}   — "format" is read with asInt()!
// The format int comes from Sony's own mapper psk::FileUtil::GetFormatFromFilename (below);
// CreateTrack passes a "uri" through UNTOUCHED when it starts with '/' (or http://, https://,
// mediastore://) — our absolute /contents paths qualify — otherwise it prefixes the parent path.
namespace pst { namespace services { namespace playerservice { namespace util {

struct UriInfo;            // opaque value type (never constructed here)
enum class UpdateReason : int;

// Node<UriInfo> — the tree the JSON builder returns, owned via unique_ptr<Node> whose
// default_delete needs a COMPLETE type: we ship a sized shell + the exported dtor (D1 @0x9d71,
// non-virtual delete — exactly what Sony's own unique_ptr<Node, default_delete> does).
// Real footprint: ConvJsonToNode allocates 32 B per node (movs r0,#32 before _Znwj @0x106ae).
template <class T> class Node {
public:
    ~Node();
private:
    Node() /* never constructed by us */;
    unsigned char _reserved[0x20];
};

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

// psk::FileUtil — Sony's filename→format mapper (exported, T @0x60a5). STATIC member (the
// disasm reads the string from r0 directly, no this). Returns the format int the JSON schema
// wants, or -1 for unsupported/relative paths (it requires a leading '/'). Its extension
// table (@.rodata 0x13c00): wav mp3 m4a mpa mp4 3gp 3gpp 3g2 3gpp2 aac flac aif aiff aifc
// afc dsf dff wma asf wmv oma aa3 ape ogg — everything the device decodes.
namespace psk {
class FileUtil {
public:
    static int GetFormatFromFilename(const std::string& filename);
};
} // namespace psk

} } } } // namespace pst::services::playerservice::util
