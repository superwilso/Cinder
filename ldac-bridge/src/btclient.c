// btclient.c — control-plane shim for Sony's BtTransmitterServiceClient.
//
// We cannot use the C++ client class normally (Sony builds with clang/libc++; the
// methods are virtual and only the factory symbol is exported). Instead we call
// the exported factory to get the object, then invoke its virtual methods through
// the vtable by INDEX, with the AAPCS thiscall convention (this -> r0). This avoids
// any compiler C++-ABI dependency in our code.
//
// The object from CreateInstance (decompiled, FUN @0x1e840) is 0x34 bytes with the
// IBtTransmitterService vtable at word[0] and a ServiceClientBase subobject vtable
// at word[1]; CreateInstance already calls Connect().
//
// >>> TODO (on-device RE): the VTABLE INDICES below are placeholders. Extract them
// with a Ghidra vtable dump of the BtTransmitterServiceClient primary vtable and map
// each slot to its method via the per-function log strings
// ("BtTransmitterServiceClient::SetLdac" etc.). Until then this shim is non-functional.
#include "btclient.h"
#include <stdio.h>

// Exported factory (mangled). Ghidra typed it void; it actually returns the client*.
extern void *_ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv(void);

// Vtable indices into the BtTransmitterServiceClient primary vtable, extracted via
// analysis/E_usbdac_ldac/ghidra/DumpVtable.java (vptr = group_base+8 confirmed
// against CreateInstance; slot 0 = first virtual after the [0,typeinfo] header).
enum {
    VIDX_SetCurrentSource       = 12, // SetCurrentSource(const bool&)
    VIDX_SetLdacSoundQuality    = 18, // SetLdacSoundQuality(const enum&)
    VIDX_SetLdac                = 20, // SetLdac(const bool&)
    VIDX_GetCapabilities        = 25,
    VIDX_GetSocketName          = 29, // GetSocketName() -> std::string
    VIDX_SetEnableLowLatency    = 38,
    // NotifyOpenAudio/CloseAudio/PcmPreferredSize are NOT client vtable methods —
    // the server opens the audio socket internally (FUN_00019aa0). The producer
    // just connects to GetSocketName() and writes; the open is triggered by the
    // SetLdac/SetCurrentSource path (confirm on-device). So we no longer call them.
    VIDX_NotifyOpenAudio        = -1,
    VIDX_NotifyCloseAudio       = -1,
    VIDX_NotifyPcmPreferredSize = -1,
};

struct bt_client { void *obj; };

// vtable[idx] of a C++ object whose first word is the primary vtable pointer.
static inline void *vslot(void *obj, int idx) {
    void **vtbl = *(void ***)obj;
    return vtbl[idx];
}

void btclient_set_current_source(bt_client_t *c, bool on) {
    typedef void (*fn)(void *self, const bool *arg);
    bool v = on;
    ((fn)vslot(c->obj, VIDX_SetCurrentSource))(c->obj, &v); // SetCurrentSource(const bool&)
}

bt_client_t *btclient_create(void) {
    void *obj = _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv();
    if (!obj) return NULL;
    static struct bt_client c;   // single instance for now
    c.obj = obj;
    return &c;
}

void btclient_destroy(bt_client_t *c) { (void)c; /* TODO: vtable dtor / ServiceClientBase teardown */ }

void btclient_set_ldac(bt_client_t *c, bool on) {
    if (VIDX_SetLdac < 0) { fprintf(stderr, "btclient: SetLdac vidx TODO\n"); return; }
    typedef void (*fn)(void *self, const bool *arg);
    bool v = on;
    ((fn)vslot(c->obj, VIDX_SetLdac))(c->obj, &v);   // SetLdac(const bool&)
}

void btclient_set_ldac_quality(bt_client_t *c, bt_ldac_quality_t q) {
    if (VIDX_SetLdacSoundQuality < 0) { fprintf(stderr, "btclient: SetLdacSoundQuality vidx TODO\n"); return; }
    typedef void (*fn)(void *self, const int *arg);
    int v = (int)q;
    ((fn)vslot(c->obj, VIDX_SetLdacSoundQuality))(c->obj, &v);
}

void btclient_notify_open_audio(bt_client_t *c) {
    if (VIDX_NotifyOpenAudio < 0) { fprintf(stderr, "btclient: NotifyOpenAudio vidx TODO\n"); return; }
    typedef void (*fn)(void *self);
    ((fn)vslot(c->obj, VIDX_NotifyOpenAudio))(c->obj);
}

void btclient_notify_close_audio(bt_client_t *c) {
    if (VIDX_NotifyCloseAudio < 0) return;
    typedef void (*fn)(void *self);
    ((fn)vslot(c->obj, VIDX_NotifyCloseAudio))(c->obj);
}

uint16_t btclient_pcm_preferred_size(bt_client_t *c) {
    // NotifyPcmPreferredSize(const uint16_t&) tells the SERVER our chunk; the server
    // may also impose its own ("Over read pcm size MAX"). Default until RE'd.
    (void)c;
    return 0;
}

void btclient_get_socket_name(bt_client_t *c, char *out, size_t outlen) {
    out[0] = '\0';
    if (VIDX_GetSocketName < 0) { fprintf(stderr, "btclient: GetSocketName vidx TODO\n"); return; }
    // Returns libc++ std::string by value (sret): hidden result ptr is arg0, this is arg1.
    // libc++ layout (12 bytes): if (bytes[0] & 1)==0 -> short: size=bytes[0]>>1, data=&bytes[1];
    // else long: cap=word[0], size=word[1], data=word[2].
    typedef void (*fn)(void *ret, void *self);
    unsigned char s[12] = {0};
    ((fn)vslot(c->obj, VIDX_GetSocketName))(s, c->obj);
    const char *data; size_t n;
    if ((s[0] & 1) == 0) { n = s[0] >> 1; data = (const char *)&s[1]; }
    else { unsigned int *w = (unsigned int *)s; n = w[1]; data = (const char *)(uintptr_t)w[2]; }
    if (n >= outlen) n = outlen - 1;
    for (size_t i = 0; i < n; i++) out[i] = data[i];
    out[n] = '\0';
    // NOTE: a long-form string is heap-owned by libc++; we copy out immediately. We do
    // not free it here (the std::string dtor would; acceptable small leak for a daemon).
}
