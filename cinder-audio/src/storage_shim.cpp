// storage_shim.cpp — implements cinder_storage.h over Sony's StorageMgrServiceFwClient.
//
// DLOPEN, NOT -l. Every other shim in this directory links its Sony library directly, and this one
// deliberately does not. libStorageMgrServiceFw.so is the storage manager's whole framework — it
// drags in libConnMgrService, libDmpConfig, libInitialObject and libhgrmutil behind it — and the
// binary it would become a NEEDED entry of is the Home app. A Home app that fails to start leaves
// the device with no launcher and the user recovering it by hand, which is a far worse outcome
// than "the SD card setting is greyed out". dlopen keeps the failure local: if the library is
// missing or unloadable, every call here returns -1 and Cinder carries on.
//
// The client's methods are looked up by mangled name and called through plain function pointers
// with the object as the first argument. That is the ARM AAPCS `this` register (r0) and it is
// exactly what the compiler would emit for a direct call — none of these are dispatched through
// the vtable at the call site, because we already hold the concrete type the factory built.
#include "storage_abi.hpp"
#include "cinder_storage.h"

#include <dlfcn.h>
#include <cstring>

namespace {

namespace sm = pst::services;
using IF = sm::IStorageMgrServiceFw;

// Mangled names, taken from `nm -D` on the device library. Spelled out rather than obtained by
// declaring the class and letting the compiler mangle, because a mismatch there fails at RUN time
// (dlsym returns null and the feature silently reports unavailable) rather than at link time,
// which is the wrong way round for something this easy to get wrong.
const char kFactory[] = "_ZN3pst8services32StorageMgrServiceFwClientFactory14CreateInstanceEv";
const char kGetAuto[] = "_ZN3pst8services25StorageMgrServiceFwClient25GetSettingAutoExportAsMscERKNS0_20IStorageMgrServiceFw32ReqMsg_GetSettingAutoExportAsMscERNS2_32RspMsg_GetSettingAutoExportAsMscE";
const char kSetAuto[] = "_ZN3pst8services25StorageMgrServiceFwClient25SetSettingAutoExportAsMscERKNS0_20IStorageMgrServiceFw32ReqMsg_SetSettingAutoExportAsMscERNS2_32RspMsg_SetSettingAutoExportAsMscE";
const char kMount[]   = "_ZN3pst8services25StorageMgrServiceFwClient5MountERKNS0_20IStorageMgrServiceFw14ReqMsg_OperateERNS2_14RspMsg_OperateE";
const char kExport[]  = "_ZN3pst8services25StorageMgrServiceFwClient17EnableExportAsMscERKNS0_20IStorageMgrServiceFw24ReqMsg_EnableExportAsMscERNS2_24RspMsg_EnableExportAsMscE";

typedef void* (*create_fn)(void);
typedef int (*get_auto_fn)(void*, const IF::ReqMsg_GetSettingAutoExportAsMsc*, IF::RspMsg_GetSettingAutoExportAsMsc*);
typedef int (*set_auto_fn)(void*, const IF::ReqMsg_SetSettingAutoExportAsMsc*, IF::RspMsg_SetSettingAutoExportAsMsc*);
typedef int (*operate_fn)(void*, const IF::ReqMsg_Operate*, IF::RspMsg_Operate*);
typedef int (*export_fn)(void*, const IF::ReqMsg_EnableExportAsMsc*, IF::RspMsg_EnableExportAsMsc*);

struct Api {
    void*       lib    = nullptr;
    void*       client = nullptr;
    get_auto_fn get_auto = nullptr;
    set_auto_fn set_auto = nullptr;
    operate_fn  mount    = nullptr;
    export_fn   export_msc = nullptr;
    bool        tried  = false;   // resolution attempted (successfully or not)
};

Api g_api;

// Resolve the library and build the client, once. Returns null if anything is missing.
//
// The client is cached for the life of the process. CreateInstance() connects to the service, so
// re-creating one per call would be a binder connect per toggle; and there is nothing to tear
// down on the way out, since cinder-home does not unload its Sony libraries.
Api* api() {
    if (g_api.tried) return g_api.client ? &g_api : nullptr;
    g_api.tried = true;

    // Plain soname first: cinder-home's rpath already covers /system/vendor/sony/lib. The
    // absolute path is the fallback for anything launched without that rpath (cinder-probe).
    g_api.lib = dlopen("libStorageMgrServiceFw.so", RTLD_NOW | RTLD_LOCAL);
    if (!g_api.lib) {
        g_api.lib = dlopen("/system/vendor/sony/lib/libStorageMgrServiceFw.so", RTLD_NOW | RTLD_LOCAL);
    }
    if (!g_api.lib) return nullptr;

    create_fn create = reinterpret_cast<create_fn>(dlsym(g_api.lib, kFactory));
    g_api.get_auto   = reinterpret_cast<get_auto_fn>(dlsym(g_api.lib, kGetAuto));
    g_api.set_auto   = reinterpret_cast<set_auto_fn>(dlsym(g_api.lib, kSetAuto));
    g_api.mount      = reinterpret_cast<operate_fn>(dlsym(g_api.lib, kMount));
    g_api.export_msc = reinterpret_cast<export_fn>(dlsym(g_api.lib, kExport));

    // All-or-nothing: a partial resolution means the library is not the one this was RE'd against,
    // and calling half of it is worse than reporting the feature missing.
    if (!create || !g_api.get_auto || !g_api.set_auto || !g_api.mount || !g_api.export_msc) {
        return nullptr;
    }

    // Connects to the storage service. If that faults because the service is down, the caller's
    // run_guarded unwinds before the assignment below and we simply retry on the next call — the
    // `tried` latch is already set, so a permanently absent service costs one dlopen, not one
    // per call. (g_api.client stays null, and api() returns null from here on.)
    g_api.client = create();
    return g_api.client ? &g_api : nullptr;
}

} // namespace

extern "C" {

int cinder_storage_get_auto_export(void) {
    Api* a = api();
    if (!a) return -1;
    IF::ReqMsg_GetSettingAutoExportAsMsc req;
    IF::RspMsg_GetSettingAutoExportAsMsc rsp;
    std::memset(&req, 0, sizeof(req));
    std::memset(&rsp, 0, sizeof(rsp));
    a->get_auto(a->client, &req, &rsp);
    if (!sm::storage_err_ok(rsp.err)) return -1;
    return rsp.enabled ? 1 : 0;
}

int cinder_storage_set_auto_export(int on) {
    Api* a = api();
    if (!a) return -1;
    IF::ReqMsg_SetSettingAutoExportAsMsc req;
    IF::RspMsg_SetSettingAutoExportAsMsc rsp;
    std::memset(&req, 0, sizeof(req));
    std::memset(&rsp, 0, sizeof(rsp));
    req.enable = (on != 0);
    a->set_auto(a->client, &req, &rsp);
    return sm::storage_err_ok(rsp.err) ? 0 : -1;
}

int cinder_storage_mount(int storage) {
    Api* a = api();
    if (!a) return -1;
    if (storage < IF::kInternal || storage > IF::kExternal1) return -1;
    IF::ReqMsg_Operate req;
    IF::RspMsg_Operate rsp;
    std::memset(&req, 0, sizeof(req));
    std::memset(&rsp, 0, sizeof(rsp));
    req.storage = storage;
    a->mount(a->client, &req, &rsp);
    return sm::storage_err_ok(rsp.err) ? 0 : -1;
}

int cinder_storage_export_as_msc(int on) {
    Api* a = api();
    if (!a) return -1;
    IF::ReqMsg_EnableExportAsMsc req;
    IF::RspMsg_EnableExportAsMsc rsp;
    std::memset(&req, 0, sizeof(req));
    std::memset(&rsp, 0, sizeof(rsp));
    req.enable = (on != 0);
    a->export_msc(a->client, &req, &rsp);
    return sm::storage_err_ok(rsp.err) ? 0 : -1;
}

} // extern "C"
