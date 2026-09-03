# RE — StorageMgr, and why a USB cable deletes the microSD library

**Date:** 2026-09-03 · **Firmware:** 1.02 (`ro.sony.swid 03.01.E.1.02.00`) ·
**Library:** `/system/vendor/sony/lib/libStorageMgrServiceFw.so` (215,132 bytes)
**Status:** measured on the reference device, fix verified on hardware.

## The symptom

A third of the library was missing from the player. The user named one album — *Untrue* by
Burial — and the DB told the rest of the story.

`/db/MTPDB.dat` held **2326** audio rows. Every one of them was on internal storage, and the set
matched `/contents/MUSIC` exactly: 2326 files on disk, 2326 rows, **zero** difference in either
direction. So the internal index was not broken, or stale, or partial. It was perfect.

The `albums` table had **365** rows but only **244** were referenced by any track. The 121 orphans
were the missing albums — *Untrue*, *Nevermind*, *Back in Black*, *Midnight Marauders* — album rows
with no tracks behind them.

Two artefacts explained why:

- `/db/MTPDB.dat.scanning2` (75 bytes) contained a single path:
  `/data/mnt//external/MUSIC/Burial - Untrue/04 - Burial - Ghost Hardware.flac`
  — the scanner's checkpoint, frozen mid-album on the *external* storage.
- `/contents/MTPDB_copy.dat`, a backup taken earlier the same day, held **2963** audio rows of
  which **637 were under `external/`**. The live DB had none.

So the card had been indexed at least 637 tracks deep, and those rows had since been **deleted**.

## The cause, from Sony's own log

```
InformEnabledMscHost(enabled: [1], by_boot: [0])
transact ApiId: [Export] to storage: [Internal] is disabled
failed to enabled msc host commit ApiId: [Internal] to storage: [Export]
StorageStatus: storage[External0], status[Unmounting]
StorageStatus: storage[External0], status[Unmounted]
StorageStatus: storage[External0], status[UnmountExported]
```

Plugging in USB raises `MscHost`. Internal storage is exempt from the export — that exemption is
why `/contents` and adb survive a cable at all — but **External0 is not**. The card is unmounted
and the raw block device is handed to the mass-storage gadget:

```
/sys/devices/platform/mt_usb/musb-hdrc.0/gadget/lun1/file = /dev/block/mmcblk1
```

Unmounting External0 drops its whole object subtree from `MTPDB.dat`. The `albums`/`artists`
lookup rows are global and survive, which is the orphan residue.

**The partial/complete distinction matters and was measured, not assumed.** After a scan that ran
to completion undisturbed, the DB held 3456 audio rows (2326 internal + 1130 card) and those rows
**survived** a subsequent unmount-and-export — verified by pulling the DB with the card exported.
It is only an index that is still *in flight* that gets discarded. That is the whole bug: every
reconnect restarts the card scan from zero, so a card that is new, or has just gained music, can
stay unindexed indefinitely on a device that gets charged from a PC.

Two red herrings, both checked rather than waved away:

- **`Not exFAT or failed to access device /dev/block/mmcblk1p1`** appears on every attempt and is
  informational (severity `N`). The card is FAT32 — its boot sector reads `MSDOS5.0` with a FAT32
  BPB, 512 B sectors, 32 sectors/cluster, 62,545,824 total sectors — and the vfat path works fine:
  a manual `mount -t vfat -o ro` returned rc=0 first try, and the service itself reaches
  `status[Mounted]` whenever MSC is not holding the device.
- **Disk space.** `/db` is its own partition and had never been measured. It is **94 MB, 5 MB used,
  89 MB free**, and the DB grows ~1.6 KB per track (5,566,464 bytes for 3456 tracks). `images`
  stores *references* — `value` is the source path and `dataoffset`/`datasize` point inside the
  FLAC — not blobs, so art costs no space there. A full card scan adds about 2 MB. Space was never
  the constraint. The decisive argument is shape, not size: a disk-full error cannot retroactively
  *delete* the 637 rows the backup proves existed, and `UnmountExported` appears in the log at the
  exact moment the cable goes in.

## The lever

`StorageMgrImpl::GetApiIdToUpdateMscHostEnabled` @0x1c20c decides what to do when a USB host
appears:

```
1c2cc:  ldrb  r0, [r9, #16]     ; msc_host_enabled_
1c2d0:  cbz   r0, 1c2dc         ;   no host -> unexport path
1c2d2:  ldrb  r0, [r9, #21]     ; <-- AutoExportAsMsc
1c2d6:  cbz   r0, 1c2fa         ;   false -> return false, ApiId stays 6 (none)
1c2d8:  movs  r0, #2            ;   true  -> ApiId 2 = Export
```

`this+21` is exactly the byte `StorageMgrImpl::GetSettingAutoExportAsMsc` @0x1bf68 returns
(`ldrb r0, [r5, #21]`) and `SetSettingAutoExportAsMsc` @0x1bf20 writes (`strb r0, [r5, #21]`).
Clear it and the Export transaction is never raised.

The ApiId numbering is self-checking. `mgr::GetStr(ApiId const&)` indexes the .rodata run
`Mount Unmount Export Unexport Format Remove` at 0x30da5, so Export is index 2 — matching the
hard-coded `#2` above — and the run ends at Remove (5), making the `#6` default one past the end,
i.e. "none". `GetStr(Storage const&)` indexes `Internal External0 External1 Invalid` at 0x30d78.

**It persists.** `StorageMgrServiceImpl::SetSettingAutoExportAsMsc` @0x1a050 calls the in-memory
setter through vtable slot 10, then `DmpConfig::Set(key 2, value, false)` — key 2 being
`FNC_MSC_AUTOEXPORT`, the only export-related key in `libDmpConfig.so`. So the choice survives a
reboot on its own. Cinder re-applies it at startup anyway, because a factory reset or a trip
through stock firmware puts the stock value back, and the symptom when it returns is "some albums
vanished", which nobody would connect to a USB cable.

`err` is 0 on success — read off the service side rather than guessed: `StorageMgrServiceFw::
GetSettingAutoExportAsMsc` @0x17b34 stores the impl's return into `rsp+0`, and the impl returns 0
after a successful read, 1 when there is no impl.

## The client ABI

`StorageMgrServiceFwClientFactory::CreateInstance()` @0x2b928 does `operator new(52)`, installs the
primary vtable at +0 and the secondary at +4, and calls `ServiceClientBase::Connect()` on `this+4`.
There is no exported constructor, so the factory is the only way in — which is simpler than
`PowerMgrServiceClient`, where the object had to be sized before it could be allocated safely.

Message sizes come from the client's own `SizeOf*` helpers, each of which returns a constant rather
than computing from the value, so they are exact. Offsets come from the matching `Write*`/`Read*`:

| Message | Size | Layout |
|---|---|---|
| `ReqMsg_Operate` | 4 | `{ int storage; }` — `Alloc(4)`, one word from +0 |
| `RspMsg_Operate` | 4 | `{ int err; }` |
| `ReqMsg_SetSettingAutoExportAsMsc` | 4 | `{ bool enable; }` — `Alloc(1)`, `strb` from +0 |
| `RspMsg_SetSettingAutoExportAsMsc` | 4 | `{ int err; }` |
| `ReqMsg_GetSettingAutoExportAsMsc` | 4 | (unused) |
| `RspMsg_GetSettingAutoExportAsMsc` | 8 | `{ int err; bool enabled; }` — `Get(4)`→+0, `Get(1)`→+4 |
| `ReqMsg_EnableExportAsMsc` | 4 | `{ bool enable; }` |
| `RspMsg_EnableExportAsMsc` | 4 | `{ int err; }` |

Like every pst client these are asynchronous binder round-trips and need `Framework::Pump()`
running, or they return uninitialised stack instead of failing.

## Verified on device

`cinder-probe --storage` (read-only), then `off`, then `mount`, 2026-09-03:

```
storage: AutoExportAsMsc = 1 (on — a USB cable takes the card away)
storage: lun1   = /dev/block/mmcblk1
storage: SetSettingAutoExportAsMsc(0) -> 0
storage: AutoExportAsMsc now = 0 (wanted 0) OK
storage: Mount(External0) -> 0
storage: mount  /dev/block/mmcblk1p1 /contents_ext vfat rw,noexec,...
storage: lun1   = (empty)
```

and hagodaemon agreed — `StorageStatus: storage[External0], status[Mounted]` — with the USB cable
still connected and adb still up. That combination was not reachable before: the card and the PC
could not both have the device at once.

## Implementation note

`storage_shim.cpp` **dlopens** `libStorageMgrServiceFw.so` instead of linking it, unlike every
other shim in that directory. The library pulls in `libConnMgrService`, `libDmpConfig`,
`libInitialObject` and `libhgrmutil` behind it, and the binary it would become a `NEEDED` entry of
is the Home app. A Home app that will not start leaves the device with no launcher and the user
recovering it by hand — much worse than a greyed-out setting. dlopen keeps the blast radius to
"this feature reports unavailable".

## Related

- `cinder-audio/src/storage_abi.hpp` — the declarations, with the disassembly inline.
- `analysis/H_mediastore/RE_findings.md` — the media store / scanner side.
- `cinder-home/src/probe.cpp` `--storage` — the device-side test.
