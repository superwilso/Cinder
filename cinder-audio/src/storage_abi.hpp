// storage_abi.hpp — hand-written declaration of Sony's storage-manager client
// (libStorageMgrServiceFw.so → pst::services::StorageMgrServiceFwClient), reconstructed by
// offline RE of the stock 1.02 firmware. See analysis/RE_storagemgr.md for the disassembly this
// was read from.
//
// WHY CINDER NEEDS THIS AT ALL — the microSD library vanishes when you plug in a USB cable.
// Measured 2026-09-03 on the reference device. hagodaemon's own log, on every USB connect:
//
//     InformEnabledMscHost(enabled: [1], by_boot: [0])
//     transact ApiId: [Export] to storage: [Internal] is disabled
//     StorageStatus: storage[External0], status[Unmounting]
//     StorageStatus: storage[External0], status[UnmountExported]
//
// Internal storage is exempt (that exemption is why /contents and adb survive a cable), but the
// card is not: it is unmounted and handed to the USB mass-storage gadget as lun1. If Sony's media
// scanner is midway through indexing the card when that happens, the PARTIAL index is discarded —
// the run that prompted this work had reached 637 of 1130 tracks (checkpointed in
// /db/MTPDB.dat.scanning2) and MTPDB.dat came back with zero external rows. A COMPLETED index does
// survive the unmount, so the bug only bites while a scan is in flight — which is precisely when a
// card is new, or has just had music added. Every plug-in restarts the scan from zero, so a user
// who charges from a PC can keep a card unindexed indefinitely.
//
// THE LEVER. StorageMgrImpl::GetApiIdToUpdateMscHostEnabled (@0x1c20c) decides what to do when a
// USB host appears:
//
//     1c2cc:  ldrb  r0, [r9, #16]     ; msc_host_enabled_
//     1c2d0:  cbz   r0, 1c2dc         ;   ...unexport path if no host
//     1c2d2:  ldrb  r0, [r9, #21]     ; <-- AutoExportAsMsc
//     1c2d6:  cbz   r0, 1c2fa         ;   false -> return false, ApiId stays 6 (none)
//     1c2d8:  movs  r0, #2            ;   true  -> ApiId 2 = Export
//
// and `this+21` is exactly the byte GetSettingAutoExportAsMsc (@0x1bf68) hands back
// (`ldrb r0, [r5, #21]`). So clearing that one flag stops the Export transaction being raised at
// all, and both storages stay mounted across a cable. It is reversible.
//
// DO NOT ASSUME IT PERSISTS. StorageMgrServiceImpl::SetSettingAutoExportAsMsc (@0x1a050) does call
// DmpConfig::Set(key 2, …) — key 2 being FNC_MSC_AUTOEXPORT — and Start() (@0x1a0e8) does call
// DmpConfig::Get before pushing the value back in, so the round trip is all there in the code.
// MEASURED ANYWAY, and it came back ON: after a reboot on 2026-09-03 the setting read 1 again.
// That reboot was a kernel panic rather than a clean shutdown, so an unflushed NVP write is the
// likely explanation and the clean-shutdown case is UNVERIFIED — but the conclusion for callers is
// the same either way, and it is the safe one: **cinder-home re-applying this at every startup is
// load-bearing, not belt-and-braces.** Do not remove it on the grounds that the service persists
// the value; that was believed once and the device disagreed.
//
// Nothing is lost by turning it off: deliberate USB transfer still works, because Cinder enters
// mass storage itself (Settings ▸ USB mode → setprop sys.sony.config msc, an init-level path that
// does not go through StorageMgr at all). What goes away is the AUTOMATIC handover on any cable,
// including a charger.
//
// ENUMS. Read out of the .rodata run that GetStr(Storage const&) and mgr::GetStr(ApiId const&)
// index (@0x30d78 onward): `Internal External0 External1 Invalid` then
// `Mount Unmount Export Unexport Format Remove`. The ApiId run is self-checking — the disassembly
// above hard-codes 2 for the export case and 6 for "none", and Export is the third name in the run
// (index 2) with the run ending at Remove (index 5), so 6 is one past the end. Two independent
// readings of the same numbering agree.
//
// LIFETIME/SAFETY. The client is NOT constructed with `new`: the factory does it
// (StorageMgrServiceFwClientFactory::CreateInstance @0x2b928 → operator new(52), installs both
// vtables, then ServiceClientBase::Connect() on this+4). So unlike PowerMgrServiceClient there is
// no object to size and no heap-overflow question — we never allocate one. There is also no
// exported constructor to call even if we wanted to.
//
// These calls are asynchronous binder round-trips like every other pst client, so they need
// pst::core::Framework::Pump() running or they return uninitialised stack rather than failing
// (see the pump note in player_shim.cpp). cinder-home drives the pump; call these only after the
// easel lifecycle is up, and only from behind run_guarded.
#pragma once

namespace pst {
namespace services {

// The message structs. Sizes are the client's own SizeOf* helpers, each of which returns a
// constant — they are not computed from the value, so these are exact:
//   ReqMsg_Operate 4, RspMsg_Operate 4, ReqMsg_/RspMsg_EnableExportAsMsc 4,
//   ReqMsg_/RspMsg_SetSettingAutoExportAsMsc 4, ReqMsg_GetSettingAutoExportAsMsc 4,
//   RspMsg_GetSettingAutoExportAsMsc 8.
// Field offsets come from the matching Write*/Read* helpers (which byte each one touches).
class IStorageMgrServiceFw {
public:
    // GetStr(Storage const&) @0x1b298 indexes "Internal External0 External1 Invalid".
    enum Storage : int {
        kInternal  = 0,
        kExternal0 = 1,   // the microSD slot -> /contents_ext, /dev/block/mmcblk1p1
        kExternal1 = 2,
        kInvalid   = 3,
    };

    // Mount/Unmount take the storage and give back an error code. WriteReqMsg_Operate (@0x2cea8)
    // Alloc(4)s and copies req+0 as one word; ReadRspMsg_Operate (@0x2cef0) Get(4)s into rsp+0.
    struct ReqMsg_Operate { int storage; };
    struct RspMsg_Operate { int err; };

    // WriteReqMsg_EnableExportAsMsc mirrors the Set below: one byte on the wire, 4 in the struct.
    struct ReqMsg_EnableExportAsMsc { bool enable; char _pad[3]; };
    struct RspMsg_EnableExportAsMsc { int err; };

    // WriteReqMsg_SetSettingAutoExportAsMsc (@0x2d93c) does Alloc(1) and `strb req+0` — the bool
    // is the whole payload — while SizeOfReqMsg (@0x2d90c) returns 4, so the struct is padded.
    struct ReqMsg_SetSettingAutoExportAsMsc { bool enable; char _pad[3]; };
    struct RspMsg_SetSettingAutoExportAsMsc { int err; };

    // ReadRspMsg_GetSettingAutoExportAsMsc (@0x2dc6c) does Get(4) -> rsp+0, then Get(1) -> rsp+4.
    struct ReqMsg_GetSettingAutoExportAsMsc { int _reserved; };
    struct RspMsg_GetSettingAutoExportAsMsc { int err; bool enabled; char _pad[3]; };
};

// `err` is 0 on success. Read off the service side rather than guessed: StorageMgrServiceFw::
// GetSettingAutoExportAsMsc (@0x17b34) stores the impl's return into rsp+0, and the impl
// (@0x1bf68) returns 0 after the read and 1 when there is no impl to read from. The same shape
// holds for the setter (@0x1a050): 0 once DmpConfig::Set has been reached, 1 if it bailed early.
inline bool storage_err_ok(int err) { return err == 0; }

} // namespace services
} // namespace pst
