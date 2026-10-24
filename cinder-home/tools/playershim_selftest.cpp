// playershim_selftest — host test for cinder-audio/src/player_shim.cpp (docs/SHORTCOMINGS.md §A2).
//
// The Sony side (PlayController, NodeTrackSequence, NodeJsonUtil, Node) is defined here as a
// recording fake, as powershim_selftest does for the power client. Each pin is a lesson from the
// device, named in the shim:
//   * a play is SetTrackSequence, then Pause, then Play (Idle -> Play rebooted the player);
//   * the queue JSON escapes what a path can hold, and a rejected sequence is not kept;
//   * repeat-one is never sent while it is off (the verified play path stays byte for byte);
//   * ◁ and ◁◁ pass a real option, never null (Sony dereferences it first);
//   * a seek while playing is Pause, SeekTime, Play, and a seek while paused stays paused;
//   * shutdown is ClosePlayer before Disconnect (else the Music track leaks in hagodaemon);
//   * the listener's slot 3 is position, slot 2 state;
//   * an edit in place carries the playing track's node id to its new node at +0xc of a 32-byte
//     child, so Sony's EditNode finds it again without a MoveTo.
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <map>
#include <string>
#include <vector>

#include "../../cinder-audio/src/player_shim.cpp"

namespace {
std::vector<std::string> g_calls;
std::string g_json;
int g_connect_rc = 0, g_seq_rc = 0;
const void* g_listener_seen = nullptr;
bool g_found_by_id = false;
unsigned g_next_id = 1000;
const char* const kLongUri = "/contents/MUSIC/A long enough folder name/01 A long enough title.flac";

using NodeT = psu::Node<psu::UriInfo>;
struct Kids { char* begin; char* end; char* cap; };
NodeT* make_node(const std::vector<std::string>& uris) {
    auto* kids = static_cast<char*>(std::calloc(uris.size() + 1, 32));
    for (std::size_t i = 0; i < uris.size(); ++i) {
        unsigned id = g_next_id++;
        std::memcpy(kids + i * 32 + 0xc, &id, sizeof id);
        *reinterpret_cast<char**>(kids + i * 32 + 0x10) = strdup(uris[i].c_str());
    }
    auto* vec = new Kids{kids, kids + uris.size() * 32, kids + uris.size() * 32};
    auto* node = static_cast<NodeT*>(::operator new(0x20));
    std::memset(static_cast<void*>(node), 0, 0x20);
    *reinterpret_cast<Kids**>(node) = vec;
    return node;
}
Kids& kids_of(NodeT& n) { return **reinterpret_cast<Kids**>(&n); }
unsigned id_at(NodeT& n, int i) {
    unsigned id;
    std::memcpy(&id, kids_of(n).begin + i * 32 + 0xc, sizeof id);
    return id;
}
const char* uri_at(NodeT& n, int i) { return *reinterpret_cast<char**>(kids_of(n).begin + i * 32 + 0x10); }
int count_of(NodeT& n) { return static_cast<int>((kids_of(n).end - kids_of(n).begin) / 32); }

struct Seq { NodeT* root; int cur; };
std::map<const void*, Seq> g_seqs;
} // namespace

extern "C" void _ZN3pst8services13playerservice4util4NodeINS2_7UriInfoEED2Ev(void*) {}

// The framework pump is not exercised here (it needs Sony's looper); the shim still links its calls.
pst::core::Framework& pst::core::Framework::GetReference() { static char fw[0x40]; return *reinterpret_cast<Framework*>(fw); }
int pst::core::Framework::StartForApplication(std::function<void()>, bool) { return 0; }
bool pst::core::Framework::Pump(bool) { return true; }

namespace pst { namespace services { namespace playerservice {
int PlayController::Connect(pst::playservice::PlayEventListener* l) {
    g_listener_seen = l;
    g_calls.push_back("Connect");
    return g_connect_rc;
}
int PlayController::Disconnect() { g_calls.push_back("Disconnect"); return 0; }
int PlayController::ClosePlayer() { g_calls.push_back("ClosePlayer"); return 0; }
bool PlayController::IsConnected() const { return true; }
int PlayController::ChangePlayState(pst::playservice::playstate_t s) {
    g_calls.push_back("State" + std::to_string(static_cast<int>(s)));
    return 0;
}
int PlayController::Suspend() { return 0; }
int PlayController::Resume() { return 0; }
int PlayController::NextTrack() { g_calls.push_back("Next"); return 0; }
int PlayController::PrevTrack(const pst::playservice::PrevTrackOption* o) {
    g_calls.push_back(o && o->mode == 0 && o->flag == 0 ? "Prev" : "Prev:bad");
    return 0;
}
int PlayController::NextGroup() { return 0; }
int PlayController::PrevGroup(const pst::playservice::PrevGroupOption* o) {
    g_calls.push_back(o && o->mode == 0 && o->flag == 0 ? "PrevGroup" : "PrevGroup:bad");
    return 0;
}
void PlayController::SeekTime(pst::playservice::media_origin_t origin, int ms) {
    g_calls.push_back("Seek" + std::to_string(static_cast<int>(origin)) + ":" + std::to_string(ms));
}
int PlayController::GetCurrentStatus(pst::playservice::PlayStatus& out) {
    new (reinterpret_cast<char*>(&out) + 0x6c) std::string(kLongUri);
    return 0;
}
int PlayController::SetTrackSequence(const std::shared_ptr<pst::playservice::TrackSequence>& seq) {
    g_calls.push_back(seq ? "SetTrackSequence" : "SetTrackSequence:null");
    return g_seq_rc;
}
PlayerService* PlayerService::GetInstance() { static char one; return reinterpret_cast<PlayerService*>(&one); }
std::shared_ptr<PlayController> PlayerService::getPlayController(const char*) {
    return std::make_shared<PlayController>();
}

namespace util {
template <class T> Node<T>& Node<T>::operator=(Node<T>&& o) { std::memcpy(static_cast<void*>(this), &o, 0x20); return *this; }
template <class T> void Node<T>::ConnectToParent(Node<T>*) {}
template <class T, class P> NodeJsonUtil<T, P>::NodeJsonUtil() {}
template <class T, class P> NodeJsonUtil<T, P>::~NodeJsonUtil() {}
template <class T, class P> std::unique_ptr<Node<T>> NodeJsonUtil<T, P>::ConvJsonStringToNode(const std::string& json) {
    g_json = json;
    // Each child's uri, unescaped enough for the paths below.
    std::vector<std::string> uris;
    for (std::size_t at = json.find("{\"uri\":\"", 1); at != std::string::npos; at = json.find("{\"uri\":\"", at + 1)) {
        std::size_t s = at + 8, e = s;
        while (json[e] != '"' || json[e - 1] == '\\') ++e;
        uris.push_back(json.substr(s, e - s));
    }
    return std::unique_ptr<NodeT>(make_node(uris));
}
template <class T> NodeTrackSequence<T>::NodeTrackSequence(std::unique_ptr<Node<T>> node, int start,
                                                          std::function<void(UpdateReason, int)>) {
    g_seqs[this] = Seq{node.release(), start};
}
template <class T> NodeTrackSequence<T>::~NodeTrackSequence() { g_seqs.erase(this); }
template <class T> void NodeTrackSequence<T>::SetOneTrackMode(OneTrackMode m) {
    g_calls.push_back("OneTrack" + std::to_string(static_cast<int>(m)));
}
template <class T> int NodeTrackSequence<T>::EditNode(std::function<int(Node<T>&, int)> fn) {
    Seq& s = g_seqs[this];
    const unsigned playing = id_at(*s.root, s.cur);
    fn(*s.root, 0);
    // Sony finds the current track again by id.
    s.cur = -1;
    for (int i = 0; i < count_of(*s.root); ++i) {
        if (id_at(*s.root, i) == playing) s.cur = i;
    }
    g_found_by_id = s.cur >= 0;
    return 0;
}
template <class T> bool NodeTrackSequence<T>::MoveTo(int i) { g_calls.push_back("MoveTo"); g_seqs[this].cur = i; return true; }
template <class T> void NodeTrackSequence<T>::GetCurrentIndexes(int* item, int*, unsigned* id) {
    Seq& s = g_seqs[this];
    if (item) *item = s.cur;
    if (id) *id = s.cur >= 0 ? id_at(*s.root, s.cur) : 0;
}
int psk::FileUtil::GetFormatFromFilename(const std::string& f) { return f.size() > 5 && f.substr(f.size() - 5) == ".flac" ? 9 : -1; }
} // namespace util
} } } // namespace pst::services::playerservice

static int fails = 0;
static void eq(const char* what, long got, long want) {
    std::printf("  %-4s %s (got %ld, want %ld)\n", got == want ? "ok" : "FAIL", what, got, want);
    if (got != want) fails = 1;
}
static void seq(const char* what, const std::vector<std::string>& want) {
    std::string g, w;
    for (auto& s : g_calls) g += s + " ";
    for (auto& s : want) w += s + " ";
    std::printf("  %-4s %s (got %s, want %s)\n", g == w ? "ok" : "FAIL", what, g.c_str(), w.c_str());
    if (g != w) fails = 1;
    g_calls.clear();
}
static void listener_slot(int slot, int a, int b) {
    void** vtbl = *static_cast<void** const*>(g_listener_seen);
    if (slot == 2) {
        pst::playservice::PlayStatus st{};
        reinterpret_cast<void (*)(const void*, unsigned, const pst::playservice::PlayStatus&)>(vtbl[2])(g_listener_seen, a, st);
    } else {
        reinterpret_cast<void (*)(const void*, int, int)>(vtbl[slot])(g_listener_seen, a, b);
    }
}

int main() {
    std::printf("connect\n");
    eq("init returns 0", cinder_audio_init("cinder"), 0);
    eq("with our listener, not NULL", g_listener_seen == &g_listener, 1);
    seq("one Connect", {"Connect"});

    std::printf("the listener, by slot\n");
    eq("no position before a callback", cinder_audio_position(nullptr, nullptr), 0);
    listener_slot(3, 5000, 200000);
    int cur = 0, total = 0;
    eq("slot 3 is the position", (cinder_audio_position(&cur, &total), cur), 5000);
    eq("and the duration", total, 200000);
    eq("a moving position is playing", cinder_audio_is_playing(), 1);
    listener_slot(2, 7, 0);
    eq("slot 2 is the state", cinder_audio_play_state(), 7);
    eq("two events", cinder_audio_listener_events(), 2);
    listener_slot(4, 0, 0);
    eq("slot 4 is the unmapped one", cinder_audio_listener_events(), 2);
    listener_slot(5, 0, 0);
    listener_slot(6, 0, 0);
    eq("slots 5 and 6 are next and previous", cinder_audio_listener_events(), 4);

    std::printf("play\n");
    const char* uris[] = {"/contents/MUSIC/a.flac", "/contents/MUSIC/Say \"hi\"\\b.flac", "/contents/MUSIC/c\x01.mp3"};
    eq("returns 0", cinder_audio_play_tracks(uris, 3, 1), 0);
    seq("sequence, then Pause, then Play; no repeat call", {"SetTrackSequence", "State1", "State2"});
    eq("the JSON", g_json == "{\"uri\":\"/\",\"format\":-1,\"children\":["
                            "{\"uri\":\"/contents/MUSIC/a.flac\",\"format\":9},"
                            "{\"uri\":\"/contents/MUSIC/Say \\\"hi\\\"\\\\b.flac\",\"format\":9},"
                            "{\"uri\":\"/contents/MUSIC/c\\u0001.mp3\",\"format\":-1}]}", 1);
    eq("a start past the end plays from 0", (cinder_audio_play_tracks(uris, 3, 9), g_seqs.begin()->second.cur), 0);
    g_calls.clear();
    cinder_audio_set_repeat_one(1);
    g_calls.clear();
    cinder_audio_play_tracks(uris, 3, 0);
    seq("repeat on is set before the service sees the sequence", {"OneTrack2", "SetTrackSequence", "State1", "State2"});
    cinder_audio_set_repeat_one(0);
    seq("and off reaches the live one", {"OneTrack0"});
    g_seq_rc = 99;
    eq("a rejected sequence is -3", cinder_audio_play_tracks(uris, 3, 0), -3);
    eq("and is not kept", g_nts == nullptr && g_seq == nullptr, 1);
    seq("no play after a reject", {"SetTrackSequence"});
    g_seq_rc = 0;

    std::printf("transport\n");
    cinder_audio_prev_track();
    cinder_audio_prev_group();
    seq("◁ and ◁◁ pass a zeroed option", {"Prev", "PrevGroup"});
    listener_slot(3, 6000, 200000);
    cinder_audio_seek_ms(30000);
    seq("a seek while playing pauses round it", {"State1", "Seek0:30000", "State2"});
    g_cb_moved_at_ms = 0;
    cinder_audio_seek_ms(1000);
    seq("a seek while paused stays paused", {"Seek0:1000"});
    char buf[128];
    eq("the URI out of PlayStatus +0x6c", cinder_audio_current_uri(buf, sizeof buf), static_cast<long>(std::strlen(kLongUri)));
    eq("as written", std::strcmp(buf, kLongUri), 0);

    std::printf("edit in place\n");
    cinder_audio_play_tracks(uris, 3, 1);
    g_calls.clear();
    const char* edited[] = {"/contents/MUSIC/new.flac", "/contents/MUSIC/a.flac", "/contents/MUSIC/Say \"hi\"\\b.flac"};
    eq("returns 0", cinder_audio_edit_sequence(edited, 3, 2), 0);
    eq("Sony found the playing track by its id", g_found_by_id, 1);
    seq("so no MoveTo was needed", {});
    eq("and it is the new list's entry 2", std::strcmp(uri_at(*g_seqs.begin()->second.root, 2), "/contents/MUSIC/Say \\\"hi\\\"\\\\b.flac"), 0);
    eq("a start outside the list changes nothing", cinder_audio_edit_sequence(edited, 3, 3), -1);

    std::printf("repeat all\n");
    eq("restarts the same list", cinder_audio_restart_sequence(), 0);
    eq("from the top", g_seqs.begin()->second.cur, 0);
    seq("as a play", {"SetTrackSequence", "State1", "State2"});

    std::printf("shutdown\n");
    cinder_audio_shutdown();
    seq("ClosePlayer before Disconnect", {"ClosePlayer", "Disconnect"});
    eq("nothing left to call", cinder_audio_next_track(), -1);
    eq("no sequence kept", g_seqs.empty(), 1);

    std::printf("a Connect that never succeeds\n");
    g_connect_rc = 5;
    eq("init is -3", cinder_audio_init("cinder"), -3);
    seq("after five tries", {"Connect", "Connect", "Connect", "Connect", "Connect"});
    eq("and holds no controller", cinder_audio_play(), -1);

    std::printf(fails ? "playershim_selftest: FAILED\n" : "playershim_selftest: all passed\n");
    return fails;
}
