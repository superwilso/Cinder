// storageshim_selftest — host test for cinder-audio/src/storage_shim.cpp (docs/SHORTCOMINGS.md §A2).
//
// The shim reaches Sony's storage manager through dlopen/dlsym by mangled name, so the fake is the
// loader: it answers the five names with recording functions and nothing else. Pinned: the message
// sizes the client's SizeOf* helpers return, each entry reaching its own method with the right
// payload, a non-zero `err` read as failure, one client for every call, and a library missing any
// one of the names (not the one this was read from) answering -1 everywhere rather than half-working.
#include <cstddef>
#include <cstdio>
#include <cstring>
#include <dlfcn.h>  // the real declarations first, so the macros below rename only the shim's calls

static void* fake_dlopen(const char*, int);
static void* fake_dlsym(void*, const char*);
#define dlopen fake_dlopen
#define dlsym fake_dlsym
#include "../../cinder-audio/src/storage_shim.cpp"
#undef dlopen
#undef dlsym

static_assert(sizeof(IF::ReqMsg_Operate) == 4 && sizeof(IF::RspMsg_Operate) == 4, "Operate");
static_assert(sizeof(IF::ReqMsg_SetSettingAutoExportAsMsc) == 4, "SetSetting req");
static_assert(sizeof(IF::RspMsg_GetSettingAutoExportAsMsc) == 8, "GetSetting rsp");
static_assert(offsetof(IF::RspMsg_GetSettingAutoExportAsMsc, enabled) == 4, "enabled at +4");

namespace {
int g_client;          // the object the factory hands out
int g_creates = 0;
bool g_auto = true;    // Sony's default: export on any cable
int g_err = 0;         // what the service answers in rsp.err
int g_mounted = -1;
int g_export = -1;
const char* g_missing = nullptr;  // a name the fake library does not have

void* create() { ++g_creates; return &g_client; }
int get_auto(void* self, const IF::ReqMsg_GetSettingAutoExportAsMsc*, IF::RspMsg_GetSettingAutoExportAsMsc* rsp) {
    if (self != &g_client) return 1;
    rsp->err = g_err;
    rsp->enabled = g_auto;
    return 0;
}
int set_auto(void* self, const IF::ReqMsg_SetSettingAutoExportAsMsc* req, IF::RspMsg_SetSettingAutoExportAsMsc* rsp) {
    if (self != &g_client) return 1;
    if (!g_err) g_auto = req->enable;
    rsp->err = g_err;
    return 0;
}
int mount(void* self, const IF::ReqMsg_Operate* req, IF::RspMsg_Operate* rsp) {
    if (self != &g_client) return 1;
    g_mounted = req->storage;
    rsp->err = g_err;
    return 0;
}
int export_msc(void* self, const IF::ReqMsg_EnableExportAsMsc* req, IF::RspMsg_EnableExportAsMsc* rsp) {
    if (self != &g_client) return 1;
    g_export = req->enable;
    rsp->err = g_err;
    return 0;
}
} // namespace

static void* fake_dlopen(const char*, int) { return &g_client; }
static void* fake_dlsym(void*, const char* name) {
    if (g_missing && std::strcmp(name, g_missing) == 0) return nullptr;
    if (!std::strcmp(name, kFactory)) return reinterpret_cast<void*>(&create);
    if (!std::strcmp(name, kGetAuto)) return reinterpret_cast<void*>(&get_auto);
    if (!std::strcmp(name, kSetAuto)) return reinterpret_cast<void*>(&set_auto);
    if (!std::strcmp(name, kMount)) return reinterpret_cast<void*>(&mount);
    if (!std::strcmp(name, kExport)) return reinterpret_cast<void*>(&export_msc);
    return nullptr;
}

static int fails = 0;
static void eq(const char* what, long got, long want) {
    std::printf("  %-4s %s (got %ld, want %ld)\n", got == want ? "ok" : "FAIL", what, got, want);
    if (got != want) fails = 1;
}

int main() {
    std::printf("auto export\n");
    eq("reads Sony's default, on", cinder_storage_get_auto_export(), 1);
    eq("set off returns 0", cinder_storage_set_auto_export(0), 0);
    eq("reads back off", cinder_storage_get_auto_export(), 0);
    eq("any non-zero is on", (cinder_storage_set_auto_export(5), cinder_storage_get_auto_export()), 1);

    std::printf("mount and export\n");
    eq("mount the card returns 0", cinder_storage_mount(1), 0);
    eq("the card is External0", g_mounted, 1);
    g_mounted = -1;
    eq("Invalid is refused here", cinder_storage_mount(3), -1);
    eq("a negative storage is refused", cinder_storage_mount(-1), -1);
    eq("neither reached Sony", g_mounted, -1);
    eq("export off returns 0", cinder_storage_export_as_msc(0), 0);
    eq("the payload is the bool", g_export, 0);

    std::printf("a service that answers an error\n");
    g_err = 1;
    eq("get is -1", cinder_storage_get_auto_export(), -1);
    eq("set is -1", cinder_storage_set_auto_export(0), -1);
    eq("mount is -1", cinder_storage_mount(0), -1);
    eq("export is -1", cinder_storage_export_as_msc(1), -1);
    g_err = 0;
    eq("one client for every call", g_creates, 1);

    std::printf("a library missing one name\n");
    const char* const names[] = {kFactory, kGetAuto, kSetAuto, kMount, kExport};
    for (const char* name : names) {
        g_api = Api{};
        g_missing = name;
        g_mounted = -1;
        eq("every entry is -1, nothing reached",
           (cinder_storage_get_auto_export() == -1) + (cinder_storage_set_auto_export(1) == -1) +
               (cinder_storage_mount(1) == -1) + (cinder_storage_export_as_msc(1) == -1) + (g_mounted == -1),
           5);
    }

    std::printf(fails ? "storageshim_selftest: FAILED\n" : "storageshim_selftest: all passed\n");
    return fails;
}
