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
class  TrackSequence;

// ── PrevTrackOption / PrevGroupOption — NOT OPAQUE, AND NOT OPTIONAL ────────────────────────────
//
// These were forward declarations, and both call sites passed `nullptr` for them. That is a hard
// crash, not a tolerated default: `PrevTrack` DEREFERENCES the pointer eight instructions in,
// before any null check, because there is no null check. From the device lib
// (libPlayerServiceClient.so @0x30c0, `objdump -dC`):
//
//     30c0:  push {r7, lr}
//     30c4:  sub  sp, #24
//     30c8:  ldr  r2, [r0, #52]      ; controller field  -> request word 0
//     30d4:  ldr  r2, [r1, #0]       ; opt->word0        -> request word 1   <-- r1 == the option
//     30da:  ldr  r1, [r1, #4]       ; opt->word1        -> request word 2
//     30e6:  blx  r3                 ; proxy vtable +0x3c
//
// `PrevGroup` (@0x31b8) is instruction-for-instruction the same shape. `NextTrack` (@0x3080) takes
// no argument at all and builds a one-word request through vtable +0x38 — which is why ▷▷ was
// always fine and only ◁ could kill the player.
//
// WHAT IT COST. Passing NULL faulted inside Sony's client, and a SIGSEGV inside a guarded call is
// exactly what `g_ipc_dead` is for: cinder-home unwinds, refuses ALL further Sony IPC for the boot,
// and the device becomes a UI that cannot pause, skip, sleep its panel or drive Bluetooth volume
// until it is restarted. Observed on device 2026-09-02 00:23 (cinderhome.log.1 @134.479):
//
//     GUARDED CALL FAULTED : sig=11 PC=0xb6b520d4 addr=(nil)
//       libPlayerServiceClient.so(...PlayController::PrevTrack(...PrevTrackOption const*)+0x13)
//     GUARD RECOVERED: carry_out: prev — Sony IPC is now DEAD for this boot.
//
// +0x13 is the Thumb-adjusted offset of `30d4` — the `ldr r2, [r1, #0]` above, to the instruction.
//
// LAYOUT. Eight bytes, read as two words at +0 and +4; nothing else is touched. The names are
// inferred, not measured: the client's own `OnPrevTrack(change_track_mode_t, bool)` callback takes
// exactly this pair, which is the natural fit for what the request carries. Zero-initialised is
// therefore the "no special request" case, and — whatever the fields turn out to mean — a
// well-formed 8-byte object cannot fault where a null pointer always does.
//
// DEVICE-UNVERIFIED as to SEMANTICS: what mode 0 selects is not yet measured. The crash it removes
// is device-verified from the log above.
struct PrevTrackOption {
    int mode;        // change_track_mode_t, by inference from OnPrevTrack's first parameter
    int flag;        // OnPrevTrack's trailing bool, widened to the word the request actually copies
};
struct PrevGroupOption {
    int mode;
    int flag;
};

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
    // VOID, not int: the disasm (analysis/G_player_ipc/player.c @0x13200) discards the response
    // slot, so there is no status to read. Declaring it int made the shell log a bogus "seek
    // REJECTED" off whatever r0 happened to hold.
    void SeekTime(pst::playservice::media_origin_t origin, int ms);
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
// OneTrackMode — the repeat-one control. MEASURED ON DEVICE 2026-08-26 by sweeping the value
// (cinder-probe --repeatsweep): On is 2, NOT 1. Values 0 and 1 both let the track run to the end
// and stop; 2 wrapped cleanly (318333/318333 -> 1000/318333, still playing, same URI).
//
// The old guess of 1 is why repeat-one had never worked: it was applied correctly, before
// SetTrackSequence, on both the sticky and the live path — the VALUE was simply wrong, and
// SetOneTrackMode is void so nothing ever complained. What 1 means is still unknown; only 0 (off)
// and 2 (on) are established. Do not assume 3+ are unused.
enum class OneTrackMode : int { Off = 0, On = 2 };

template <class T> class NodeTrackSequence {
public:
    NodeTrackSequence(std::unique_ptr<Node<T>> node, int startIndex,
                      std::function<void(UpdateReason, int)> cb);
    ~NodeTrackSequence();
    // Repeat-one. Non-virtual, exported, and a member of an object WE construct — so this is a
    // direct call, not a vtable reconstruction.
    void SetOneTrackMode(OneTrackMode);
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
