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

long h_GetBtStatus(void*, void*, void*, void*) {
    long long scripted = 0;
    int st = cinder_harness_scripted("BtCommon::GetBtStatus", &scripted)
                 ? (int)scripted
                 : (g_radio_on ? (g_connected ? 3 : 2) : 7);
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

// A connect request against a powered-down radio is ACCEPTED AND SILENTLY DROPPED. That is the
// device behaviour behind "Bluetooth doesn't connect automatically": the call returns success and
// nothing happens, so nothing anywhere logs a failure.
long h_Connect(void*, void*, void*, void*) {
    cinder_harness_record("BtXmit::RequestLastDeviceConnection", g_radio_on ? 1 : 0);
    if (g_radio_on && !g_paired.empty()) {
        g_connected = true;
        g_peer_name = g_paired[0].first;
        g_peer_addr = g_paired[0].second;
    }
    return 0;
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
    g_vt_xmit[5]    = h_GetConnectInformation;
    g_vt_xmit[6]    = h_Connect;
    g_vt_xmit[7]    = h_Connect;
    g_vt_xmit[8]    = h_Disconnect;
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

// ── the fake radio, as a test fixture ────────────────────────────────────────────────────────
void cinder_harness_bt_reset(void) {
    g_radio_on = false; g_connected = false;
    g_peer_name.clear(); g_peer_addr.clear(); g_paired.clear();
}

void cinder_harness_bt_set_radio(int on) { g_radio_on = on != 0; }

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
