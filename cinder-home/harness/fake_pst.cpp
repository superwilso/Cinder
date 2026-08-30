// fake_pst.cpp — the three Sony service clients main.cpp links against directly, faked.
//
// main.cpp does not call these through headers (there are none); it calls them through RAW VTABLE
// SLOT INDICES recovered from the device binaries — `bt_slot(obj, 3)` is GetBtStatus, and the
// `enum { VIDX_GetBtStatus = 3 };` above the call is the only thing that says so. That indirection
// is exactly why this fake is worth having: it is the one place off-device where the trace can say
// which METHOD the app reached for rather than which number.
//
// The slot->name map is GENERATED from those call sites (slotmap.h, gen_slotmap.py) rather than
// written out here. The hand-written first version had AddListener's two slots swapped between the
// two services, so a bring-up step that ran correctly was reported as missing — the harness has to
// be harder to be wrong about than the thing it is checking.
//
// The Bluetooth fake is stateful rather than constant, following the status enum main.cpp
// documents (7 = radio off, 2 = on/idle, 3 = connected). SetRfOnOff(true) makes the next
// GetBtStatus report on; a connect request against a powered-down radio is accepted and silently
// dropped, which is the device behaviour that made "Bluetooth never reconnects" so hard to see. A
// test can still pin any slot to a fixed answer with cinder_harness_script().
#include "harness.h"
#include "slotmap.h"   // generated from main.cpp by gen_slotmap.py — see there

#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#include <utility>

namespace {

// Same layout as main.cpp's (anonymous-namespace) copy — this is the object the app hands us to
// fill in. It is redeclared rather than shared because main.cpp keeps it private on purpose.
struct BtPairedDeviceInformation {
    std::vector<unsigned char> addr;
    unsigned                   cod;
    std::vector<unsigned char> key;
    std::string                name;
    unsigned char              f0, f1;
    unsigned char              pad[6];
};

enum { SVC_COMMON = 0, SVC_XMIT = 1, SVC_UAC = 2, NSLOT = 48 };

const char* slot_name(int svc, int slot) {
    const CinderSlotName* t = svc == SVC_COMMON ? kBtCommonSlots
                            : svc == SVC_XMIT   ? kBtXmitSlots
                                                : kUacSlots;
    for (const CinderSlotName* p = t; p->name; ++p) if (p->slot == slot) return p->name;
    // Unnamed slot: still traced, because "the app called a slot nobody has mapped" is a finding.
    static char buf[3][NSLOT][32];
    if (!buf[svc][slot][0])
        std::snprintf(buf[svc][slot], sizeof buf[svc][slot], "%s::slot%d",
                      svc == SVC_COMMON ? "BtCommon" : svc == SVC_XMIT ? "BtXmit" : "Uac", slot);
    return buf[svc][slot];
}

// ── the fake radio's state ───────────────────────────────────────────────────────────────────
bool        g_radio_on  = false;
bool        g_connected = false;
std::string g_peer_name;
std::vector<unsigned char> g_peer_addr;
std::vector<std::pair<std::string, std::vector<unsigned char> > > g_paired;

// Every slot is this shape. The real methods have many different signatures, but they are all
// integer/pointer arguments in registers, so one recording thunk can stand in for all of them; the
// handful whose OUT-parameters the app actually reads are overridden below.
typedef long (*Slot)(void*, void*, void*, void*);

template <int SVC, int SLOT>
long thunk(void*, void*, void*, void*) {
    const char* n = slot_name(SVC, SLOT);
    cinder_harness_record(n, 0);
    long long v = 0;
    cinder_harness_scripted(n, &v);
    return (long)v;
}

// ── the slots whose behaviour the app depends on ─────────────────────────────────────────────

// AN IDLE RADIO REPORTS 3, NOT 2. This fake used to answer 2 for "on, nothing connected", which
// made `st == 3` a safe route test off-device and a wrong one on it. Measured 2026-08-26, 0.61 s
// after powering an idle radio with nothing in range:
//
//   btwho: GetBtStatus=3  AvSrc=2  Avrcp=1
//   btwho: GetConnectInformation rc=0 addr=(none) name=''
//
// So 3 does not mean "connected" — the ADDRESS means connected. 2 is still reachable (the device
// does report it), it simply is not the only idle answer, and a fake that only ever produced the
// convenient one hid a real defect for weeks. Answer the awkward value by default.
long h_GetBtStatus(void*, void*, void*, void*) {
    long long scripted = 0;
    int st = cinder_harness_scripted("BtCommon::GetBtStatus", &scripted)
                 ? (int)scripted
                 : (g_radio_on ? 3 : 7);
    cinder_harness_record("BtCommon::GetBtStatus", st);
    return st;
}

long h_SetRfOnOff(void* /*self*/, void* p, void*, void*) {
    const bool on = p && *reinterpret_cast<const bool*>(p);
    cinder_harness_record("BtCommon::SetRfOnOff", on ? 1 : 0);
    g_radio_on = on;
    if (!on) g_connected = false;   // powering the radio down drops the link
    return 0;
}

// Fills the caller's address+name. THE ADDRESS IS THE SIGNAL, NOT THE RETURN VALUE — the real
// service returns 0 (transaction OK) even on a live link, and reading the return as the answer is
// a bug this project actually shipped. The fake reproduces that: 0 always, address only when
// connected.
long h_GetConnectInformation(void*, void* pa, void* pn, void*) {
    cinder_harness_record("BtXmit::GetConnectInformation", g_connected ? 1 : 0);
    if (pa) {
        std::vector<unsigned char>* addr = reinterpret_cast<std::vector<unsigned char>*>(pa);
        addr->clear();
        if (g_connected) *addr = g_peer_addr;
    }
    if (pn) {
        std::string* name = reinterpret_cast<std::string*>(pn);
        name->clear();
        if (g_connected) *name = g_peer_name;
    }
    return 0;
}

// Returns TRUE on success here (main.cpp gates the list replacement on the return value, unlike
// GetConnectInformation) — the asymmetry is real and worth having a fake that keeps it.
long h_GetPairedDeviceInfo(void*, void* pl, void*, void*) {
    cinder_harness_record("BtCommon::GetPairedDeviceInfo", (long long)g_paired.size());
    long long scripted = 0;
    if (cinder_harness_scripted("BtCommon::GetPairedDeviceInfo", &scripted) && !scripted) return 0;
    if (pl) {
        std::vector<BtPairedDeviceInformation>* list =
            reinterpret_cast<std::vector<BtPairedDeviceInformation>*>(pl);
        list->clear();
        for (size_t i = 0; i < g_paired.size(); i++) {
            BtPairedDeviceInformation d;
            d.name = g_paired[i].first;
            d.addr = g_paired[i].second;
            d.cod = 0x240404;   // audio/video, wearable headset — what a WH-1000XM4 reports
            d.f0 = d.f1 = 1;
            std::memset(d.pad, 0, sizeof d.pad);
            list->push_back(d);
        }
    }
    return 1;
}

// ── the connect path, and the mode that jams it ──────────────────────────────────────────────
//
// `SetConnectRetryMode(true, …)` puts the real transmitter into a state where **every** connect
// request is refused: rc=0, and nothing reaches the air. Measured 2026-08-26 with an HCI capture
// running, same address minutes apart:
//
//   retry OFF -> RequestConnection(AC:80:0A:56:A9:91) rc=1   CMD Create Connection -> AC:80:…:91
//   retry ON  -> RequestConnection(AC:80:0A:56:A9:91) rc=0   nothing on the air at all
//
// Modelling it here is the point of this fake existing. cinder-home armed that mode on every drop
// and then issued connects into it for a week, and nothing off-device could see it: slots 6 and 7
// both pointed at one handler that always succeeded, so a request that the device would have
// refused came back linked.
//
// rc is accept/reject on this path — **1 = accepted** — not the transaction-status 0 that
// GetConnectInformation returns. The two conventions live side by side in the same client.
bool g_retry_mode = false;

bool connect_refused() { return g_retry_mode; }

// Bring the link up if anything could. A connect against a powered-down radio is ACCEPTED AND
// SILENTLY DROPPED — the device behaviour behind "Bluetooth doesn't connect automatically".
void connect_to(size_t idx) {
    if (!g_radio_on || idx >= g_paired.size()) return;
    g_connected = true;
    g_peer_name = g_paired[idx].first;
    g_peer_addr = g_paired[idx].second;
}

// slot 7 — zero-arg. The service picks the last device itself; the fake picks paired[0].
long h_ConnectLast(void*, void*, void*, void*) {
    const bool refused = connect_refused();
    cinder_harness_record("BtXmit::RequestLastDeviceConnection", refused ? 0 : 1);
    if (refused) return 0;
    connect_to(0);
    return 1;
}

// slot 6 — addressed. Records WHICH device was asked for, so a test can tell the two calls apart;
// they were indistinguishable while both slots shared one handler.
long h_RequestConnection(void*, void* pa, void*, void*) {
    const std::vector<unsigned char>* addr =
        reinterpret_cast<const std::vector<unsigned char>*>(pa);
    long long which = -1;
    if (addr)
        for (size_t i = 0; i < g_paired.size(); i++)
            if (g_paired[i].second == *addr) { which = (long long)i; break; }
    const bool refused = connect_refused();
    cinder_harness_record("BtXmit::RequestConnection", refused ? -2 : which);
    if (refused) return 0;
    if (which >= 0) connect_to((size_t)which);
    return 1;
}

// slot 3 — the A2DP source's own state machine. MEASURED on device 2026-08-26:
//   0 radio down   1 disconnected   2 idle   3 CONNECTING (a page is on the air)   4/5 connected
// The ladder reads it to avoid asking for a connect while one is already in flight (which the
// service refuses with rc=0). Script it to 3 to hold the fake radio in "connecting".
long h_GetAvSrcConnectionStatus(void*, void*, void*, void*) {
    long long scripted = 0;
    int st = cinder_harness_scripted("BtXmit::GetAvSrcConnectionStatus", &scripted)
                 ? (int)scripted
                 : (!g_radio_on ? 0 : (g_connected ? 5 : 2));
    cinder_harness_record("BtXmit::GetAvSrcConnectionStatus", st);
    return st;
}

long h_SetConnectRetryMode(void*, void* p, void*, void*) {
    const bool on = p && *reinterpret_cast<const bool*>(p);
    cinder_harness_record("BtXmit::SetConnectRetryMode", on ? 1 : 0);
    const bool changed = (on != g_retry_mode);
    g_retry_mode = on;
    return changed ? 1 : 0;   // rc is a STATE-CHANGED flag on this one, not success
}

long h_GetConnectRetryMode(void*, void*, void*, void*) {
    cinder_harness_record("BtXmit::GetConnectRetryMode", g_retry_mode ? 1 : 0);
    return g_retry_mode ? 1 : 0;
}

long h_Disconnect(void*, void*, void*, void*) {
    cinder_harness_record("BtXmit::RequestDisconnection", 0);
    g_connected = false;
    return 0;
}

struct FakeObj { Slot* vptr; };

template <int SVC, size_t... I>
void fill(Slot* v, std::index_sequence<I...>) {
    Slot tmp[] = { thunk<SVC, (int)I>... };
    for (size_t i = 0; i < sizeof...(I); i++) v[i] = tmp[i];
}

Slot     g_vt_common[NSLOT], g_vt_xmit[NSLOT], g_vt_uac[NSLOT];
FakeObj  g_obj_common, g_obj_xmit, g_obj_uac;
bool     g_built = false;

void build() {
    if (g_built) return;
    g_built = true;
    fill<SVC_COMMON>(g_vt_common, std::make_index_sequence<NSLOT>());
    fill<SVC_XMIT>(g_vt_xmit, std::make_index_sequence<NSLOT>());
    fill<SVC_UAC>(g_vt_uac, std::make_index_sequence<NSLOT>());
    g_vt_common[3]  = h_GetBtStatus;
    g_vt_common[4]  = h_SetRfOnOff;
    g_vt_common[20] = h_GetPairedDeviceInfo;
    g_vt_xmit[3]    = h_GetAvSrcConnectionStatus;
    g_vt_xmit[5]    = h_GetConnectInformation;
    g_vt_xmit[6]    = h_RequestConnection;      // addressed — NOT the same call as slot 7
    g_vt_xmit[7]    = h_ConnectLast;            // zero-arg
    g_vt_xmit[8]    = h_Disconnect;
    g_vt_xmit[27]   = h_SetConnectRetryMode;
    g_vt_xmit[28]   = h_GetConnectRetryMode;
    g_obj_common.vptr = g_vt_common;
    g_obj_xmit.vptr   = g_vt_xmit;
    g_obj_uac.vptr    = g_vt_uac;
}

} // namespace

extern "C" {

// The mangled names main.cpp declares by hand (it has no pst headers either).
void* _ZN3pst8services28BtCommonServiceClientFactory14CreateInstanceEv(void) {
    build();
    cinder_harness_record("pst:BtCommonServiceClientFactory::CreateInstance", 0);
    long long v = 0;
    if (cinder_harness_scripted("pst:BtCommonServiceClientFactory::CreateInstance", &v) && !v)
        return nullptr;   // "hagodaemon is not up yet" — the boot race, on demand
    return &g_obj_common;
}

void* _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv(void) {
    build();
    cinder_harness_record("pst:BtTransmitterServiceClientFactory::CreateInstance", 0);
    long long v = 0;
    if (cinder_harness_scripted("pst:BtTransmitterServiceClientFactory::CreateInstance", &v) && !v)
        return nullptr;
    return &g_obj_xmit;
}

void* _ZN3pst8services40UsbDeviceAudioPlayerServiceClientFactory14CreateInstanceEv(void) {
    build();
    cinder_harness_record("pst:UsbDeviceAudioPlayerServiceClientFactory::CreateInstance", 0);
    long long v = 0;
    if (cinder_harness_scripted("pst:UsbDeviceAudioPlayerServiceClientFactory::CreateInstance", &v) && !v)
        return nullptr;
    return &g_obj_uac;
}

// ── MediaStore / MediaScanner: the library rescan ───────────────────────────────────────────
//
// main.cpp binds these by their exact mangled names (see its `media_rescan`), so the harness has to
// define the same names to link. They are recorded rather than simulated: what a scenario needs to
// assert is that a rescan was ASKED FOR, and that it was configured before it was asked — the scan
// itself happens inside a Sony service that has no host equivalent.
//
// The object layout matters in one place. MediaScanner's ctor takes the INNER proxy, which main.cpp
// reads from the singleton at +4; so the fake singleton must have a non-null pointer there or the
// real code's NULL guard fires and the rescan silently does nothing on the host.
static void* g_ms_proxy_slot[4];   // [1] is the "+4" the real code reads
static bool  g_ms_configured = false;

void* _ZN3pst8services10mediastore17MediaStoreService11GetInstanceEv(void) {
    cinder_harness_record("pst:MediaStoreService::GetInstance", 0);
    long long v = 0;
    if (cinder_harness_scripted("pst:MediaStoreService::GetInstance", &v) && !v)
        return nullptr;   // service not up — the same race the DB retry ladder exists for
    g_ms_proxy_slot[1] = (void*)&g_ms_proxy_slot[2];   // non-null proxy at singleton+4
    return &g_ms_proxy_slot[0];
}

void _ZN3pst8services10mediastore17MediaStoreService9SetConfigERKNSt3__112basic_stringIcNS3_11char_traitsIcEENS3_9allocatorIcEEEESB_SB_RKNS3_6vectorIS9_NS7_IS9_EEEE(
        void*, const void*, const void*, const void*, const void*) {
    g_ms_configured = true;
    cinder_harness_record("pst:MediaStoreService::SetConfig", 0);
}

void* _ZN3pst8services12mediascanner12MediaScannerC1EP18IMediaStoreService(void* self, void*) {
    cinder_harness_record("pst:MediaScanner::ctor", 0);
    return self;
}

int _ZN3pst8services12mediascanner12MediaScanner4ScanEPNS1_21IMediaScannerListenerENS_12mediascanner10language_tE(
        void*, void*, int) {
    // 20 unconfigured, 0 accepted — the device's own answer, and the distinction the whole feature
    // turned on. A scenario that scans without configuring first should see the failure, not a pass.
    const int rc = g_ms_configured ? 0 : 20;
    cinder_harness_record("pst:MediaScanner::Scan", rc);
    return rc;
}

// ── the fake radio, as a test fixture ────────────────────────────────────────────────────────
void cinder_harness_bt_reset(void) {
    g_radio_on = false; g_connected = false; g_retry_mode = false;
    g_peer_name.clear(); g_peer_addr.clear(); g_paired.clear();
}

void cinder_harness_bt_set_radio(int on) { g_radio_on = on != 0; }

// Leave the service's retry mode armed before the app starts — what a probe session, or a Cinder
// from before 2026-08-26, leaves behind. The mode is STICKY on the device and outlives the
// process, so an app that does not reconcile it comes back to a radio that refuses every connect.
void cinder_harness_bt_set_retry_mode(int on) { g_retry_mode = on != 0; }
int  cinder_harness_bt_retry_mode(void) { return g_retry_mode ? 1 : 0; }

// Add a device to the radio's pairing table. `addr_last` is the final byte of a synthetic
// AC:80:0A:56:A9:xx address, so two fixtures are distinguishable.
void cinder_harness_bt_add_paired(const char* name, int addr_last) {
    unsigned char a[6] = {0xAC, 0x80, 0x0A, 0x56, 0xA9, (unsigned char)addr_last};
    g_paired.push_back(std::make_pair(std::string(name ? name : ""),
                                      std::vector<unsigned char>(a, a + 6)));
}

int cinder_harness_bt_connected(void) { return g_connected ? 1 : 0; }
int cinder_harness_bt_radio_on(void)  { return g_radio_on ? 1 : 0; }

} // extern "C"
