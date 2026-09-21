# Walkman One — what it actually does, and what Cinder can take without flashing

Recovered 2026-08-17 from `artifacts/walkmanone/WalkmanOne.UPG` (v3.5, 2021-09-22, "for A50Series")
vs `artifacts/unpacked/stock`. Both system images extracted with `debugfs -R rdump` (no root, no
mount) and diffed file-by-file.

**Bottom line: almost all of it is extractable.** The headline "sound signature" is a THREE-BYTE
patch to a userspace shared library. The device already ships every binary needed to apply it.

## What Walkman One is

A **model swap**, not a region unlock:

| property | stock | Walkman One |
|---|---|---|
| `ro.product.device` / `.board` | `BBDMP5_linux` | `BBDMP2_linux` |
| `ro.sony.version` | 1.02 | 3.02 |
| `ro.sony.swid` | `03.01.E.1.02.00` | `01.20.E.1.02.00` |

`BBDMP5` is the A50 series; `BBDMP2` is a higher Hagoromo model. **The destination letter stays
`E`** in both — so nothing about the EU region is changed at the property level. The gain comes
from the device presenting as a different model, which makes the boot-time audio setup load that
model's tables.

Its installer (`0.bin` of the UPG) writes 8 partitions raw:

```
index_2 → mmcblk0p8 (16M)    index_6 → p14 AND p15 (5M, written twice = tee1/tee2)
index_3 → mmcblk0p9 (16M)    index_7 → p19  (838M)  ← android/system
index_4 → mmcblk0p10 (6M)    index_8 → p21  (512K)
index_5 → mmcblk0p12 (3M)    index_9 → p25  (8M)
```

System is 528 MB vs stock's 210 MB — it carries a lot of extra payload (see "Tuning packages").

## The three separable layers

### Layer 1 — the boot-time DAC programming (`load_sony_driver`)

The gate is one line, **identical in stock and W1**:

```sh
PRODDEV=`getprop ro.product.device`
shp=`nvpflag -x shp`;  shpfirst=`echo $shp | cut -c1-10`
/system/bin/dacdat auto $PRODDEV $midupper $shpfirst
...
dacdat limiter_500 $shpfirst
dacdat limiter_750 $shpfirst
dacdat limiter_31  $shpfirst
```

- `$PRODDEV` selects **which volume-table set** is loaded.
- `$shpfirst` is the **destination code held in NVP**, not in build.prop. Values seen in the same
  script's FM-tuner branch: `0x00000001`=UC, `0x00000306`=LA, else J/EE/CEW/CN/**E**.
- The **volume limiter is an argument derived from that region code** — it is not compiled in.

`/system/bin/dacdat` is **byte-identical between stock and W1** (`d3d20f167d8f53d8b643897818a6d38c`,
19508 bytes) and is present on the device. The tool that programs the DAC therefore already has
every capability W1 uses.

W1 also swaps the table files in `/system/usr/share/audio_dac/`:

| file | size | stock | W1 |
|---|---|---|---|
| `ov_1291.tbl`, `ov_dsd_1291.tbl`, `tc_1291.tbl` (+`_cew`) | 84950 / 13076 / 2888 | ✓ | ✗ |
| `ov_127x.tbl`, `ov_1280.tbl` (+`_cew`) | 84950 | ✗ | ✓ |
| `ov_dsd_127x.tbl`, `ov_dsd_1280.tbl` (+`_cew`) | 13076 | ✗ | ✓ |
| `tc_127x.tbl`, `tc_1280.tbl` | 2888 | ✗ | ✓ |
| `ncgain_*.tbl`, `ambgain*.tbl`, `ambient480_*` | 70 | ✓ | ✗ |

`ov` = output volume, `tc` = tone control, `_cew` = the Europe variant. The trailing number is the
**DAC variant**: the A50 is 1291, the ZX300 is 1280/127x. W1 drops the A50's noise-cancelling and
ambient tables (the ZX300 has no NC headphone support) and its 1291 tables, and ships 1280/127x in
their place. It also stops insmod-ing `cxd3778gf_dnc_core.ko` — the only CXD module that differs
between the two images.

> **CORRECTED 2026-08-17.** An earlier pass here claimed stock had NO `ov_*`/`tc_*` tables and that
> W1 introduced the concept. That was wrong — it came from a directory listing truncated before the
> `ov_*` entries. **Stock ships `ov_1291`, `ov_dsd_1291` and `tc_1291` and loads them at every
> boot.** The mechanism is not new; only the tables differ.
>
> This RESOLVES what was flagged as the open question. The stock kernel demonstrably accepts an
> `ov_*.tbl` upload, because stock does exactly that on every boot. No kernel change is needed.

### Layer 2 — the audio HAL, i.e. the actual "sound signature" (3 BYTES)

`/etc/.mod/adler/{normal,normal_nt,pv1,pv2}/libaudiohal-adleralsa.so`, all 155068 bytes:

```
normal      c8de2a65cf4f   ← BYTE-IDENTICAL to stock's live /vendor/sony/lib/libaudiohal-adleralsa.so
normal_nt   c8de2a65cf4f   ← same again
pv1         6baf1bf0dcf6   ← 3 bytes differ from normal
pv2         32c9f4359dd1   ← 3 bytes differ from normal
```

W1's own live HAL is also `c8de2a65cf4f`, i.e. it ships `normal` active and the mod swaps in
`pv1`/`pv2` from a settings file at boot.

`cmp -l` against `normal` — the changed bytes are ASCII digits inside string literals, not code:

```
                    normal            pv1               pv2
ALSA out devices    hw:0,0 , hw:0,4   hw:0,0 , hw:0,0   hw:0,4 , hw:0,4
scaling_min_freq    1040000           1300000           1300000
```

So a "sound signature" is exactly two things:

1. **Which ALSA PCM device the output stream opens.** There are two path strings; `pv1` forces both
   to `hw:0,0`, `pv2` forces both to `hw:0,4`.
2. **The CPU clock floor held during playback** — `/sys/devices/system/cpu/cpu0/cpufreq/
   scaling_min_freq` written as 1300000 instead of 1040000. The standard "keep the core pinned so
   the audio thread never stalls" argument. It costs battery; see `reference_power_measurement`.

**Verified present on the device (2026-08-17):** card0 `sonysoccard` exposes PCM devices 0,1,2,3,4,5
— so both `hw:0,0` and `hw:0,4` exist on A50 hardware and `pv1`/`pv2` are meaningful here.

### Layer 2b — `/etc/.mod/anls/` is a CONSEQUENCE of the 3 bytes, not a fourth change

W1 carries a second directory with the same four variant names as `adler` — `anls/{normal,
normal_nt,pv1,pv2}` — which at first looks like the signature touching a second library. It is not.
Each holds 48 tiny text files, `delay_{dacmode,normal}_{level,spectrum}_<rate>_<bits>.txt`: the
**analyser/visualiser delay compensation** in samples.

Checked file by file:

* **`pv2` and `normal_nt` are byte-identical to `normal`.**
* **Only `pv1` differs**, and only in the `delay_normal_*` set. The big moves are at CD rates:

  | file | normal | pv1 |
  |---|---|---|
  | `delay_normal_level_44100_16` | 735 | **60** |
  | `delay_normal_level_48000_16` | 730 | **60** |
  | `delay_normal_level_88200_16` | 500 | 420 |
  | `delay_normal_level_*_32` | 350–475 | 385 |

That is exactly what the 3-byte patch predicts. `pv1` forces BOTH output path strings to `hw:0,0`,
where stock uses `hw:0,0` and `hw:0,4` — and `hw:0,4` is the CXD3778GF **low-power** playback
device. Leaving the low-power path drops ~675 samples (~15 ms at 44.1 kHz) of output latency, so
the delay tables that keep the spectrum display aligned with the sound had to be re-measured.

**So it changes when the bars move, not what you hear**, and the "sound signature is three bytes"
finding stands. It is also independent evidence for what `pv1` actually does: less buffering, lower
latency, which is the same reason it pins the CPU floor to 1.3 GHz.

### Layer 1b — the two GAIN table sets (`/etc/.mod/gain/`) — PLAIN FILES, RESOLVED

W1 also carries `/etc/.mod/gain/{gain_n,gain_l}/`, four `.tbl` files each, unencrypted and exactly
the sizes stock uses. Cross-checked against wampy's own cross-model md5 map
(`artifacts/repos/wampy/tunings/uniq.txt`), which settles what they actually are:

| file | md5 | is byte-identical to |
|---|---|---|
| `gain_n/ov_127x.tbl` | `bb5ccae7…` | **NW-A50 `ov_1291.tbl`** — your own stock curve |
| `gain_n/ov_dsd_127x.tbl` | `05858758…` | **NW-A50 `ov_dsd_1291.tbl`** |
| `gain_l/ov_127x.tbl` | `39a60adc…` | **NW-WM1A `ov_127x.tbl`** (= ZX300 `ov_1288`) |
| `gain_l/ov_dsd_127x.tbl` | `142c8a33…` | **NW-WM1A `ov_dsd_127x.tbl`** |

So Walkman One's two "gain modes" are: **normal = the A50's own volume curve, renamed to `127x` so
it loads under the `BBDMP2` model; "L" = the NW-WM1A's curve.** That is the whole feature. Both
tables are plain files, both are extracted to `artifacts/walkmanone/gain/`, and `dacdat ovt FILE`
is the loader — the same interface already recovered in Layer 1. Reachable without flashing, with
the same headphones-off caution as any volume-table change.

### Layer 3 — the "external tuning" packages — NOT REACHABLE, and here is the proof

Extracted from `/etc/.mod/tunings/` (`debugfs -R "rdump /etc/.mod/tunings …" 7.bin`). Each is a
Windows installer wrapping a nested `NW_WM_FW.UPG`: Bright and Neutral/Warm are 196720 bytes,
WM1Z is 192624. `SWUpdate.xml` targets **DMP-Z1** for Bright and **NW-WM1Z** for the other two
(an earlier note here said NW-WM1Z for all three — wrong).

**They cannot be unpacked.** In order:

1. **No known KAS decrypts them.** `upgtool -e` was run against all 24 models Rockbox knows,
   including `nw-wm1z` and `dmp-z1` (which share `2b07114f…`, the KAS the manifests point at).
   Every one returns `Signature Mismatch`. The Windows updater binaries carry no 64-hex KAS string
   either — they are Sony's own `WmFwUpdater.dll`, and the device does the validating.
2. **The cipher is a stream/CTR, not ECB.** All 24064 8-byte blocks of the common region are
   distinct at entropy 7.999, which structured plaintext under ECB could not produce.
3. **That makes the two same-size packages a two-time pad, and it still yields nothing.** Bright
   and Neutral/Warm are byte-identical except for **4000 bytes at 0xd0..0x106f** — one contiguous
   region, 8-byte aligned, everything after it identical. XOR-ing them cancels the keystream and
   gives `plainBright ⊕ plainNeutral` directly. That XOR is **99% non-zero, entropy-flat, and has
   no int16/int32 structure at any alignment** (0 of 1000 32-bit values below 256 in magnitude).
   Two plaintext coefficient tables would not XOR to noise; two independently compressed or keyed
   blobs would.

**Conclusion.** The entire "external tuning" product is a **4000-byte encrypted blob** at a fixed
offset inside a 188 KB common wrapper. Without the KAS it is opaque, and brute-forcing it is not
tractable with `upgtool`'s keysig search (that search assumes a short ASCII key).

**So the marketing signature is HALF reachable, and we already have that half.** The HAL variant —
which ALSA device the stream opens plus the CPU clock floor — is reproduced byte-for-byte by
`cinder-home/deploy/cinder-signature.sh`. The paired "external tuning" is not, and no amount of
host-side work will change that. Anyone claiming Cinder can deliver "Bright" or "WM1Z" in full
would be claiming something this analysis shows is false.

### Layer 3 (original notes)

`/etc/.mod/tunings/{Bright,Neutral_&_Warm,WM1Z}_external_tuning/`, ~4.1 MB each. Each is a Windows
installer (`FirmwareUpdateTool.exe` + `WmFwUpdater.dll`) wrapping a **nested 192 KB
`NW_WM_FW.UPG`**. `SWUpdate.xml` targets `DevicePropertyProductInfo = NW-WM1Z`, version 3.02.

Per the bundled `Tunings_Info.txt`, these are a *second* step on top of the signature:

> 1. Change the sound signature in the settings file;
> 2. Restart the player…;
> 3. Apply the corresponding external tuning … by launching FirmwareUpdateTool.exe.
> … you would see the "External tuning not installed!" message if the external tuning would not be
> applied.

So the marketing names (Warm/Bright/Neutral/WM1Z) are the *pair* of a HAL variant and a flashed
tuning blob. **The 192 KB payloads are not yet unpacked** — that is the main open question, because
it decides whether the full signature is reachable without flashing or only the HAL half is.

## What Cinder can take, ranked by confidence

1. **The CPU clock floor — trivially, today.** One sysfs write on play/stop. Cinder already manages
   the pump cadence and knows the play state. No file swap, fully reversible, and it is one of only
   two things the paid signature patch actually does. Battery cost is real and measurable.
2. **The ALSA device choice — needs the HAL swap.** Cinder plays through Sony's PlayerService, so
   the `hw:0,N` string lives in the HAL, not in Cinder. Dropping `pv1`/`pv2` into
   `/vendor/sony/lib/libaudiohal-adleralsa.so` is a plain file replace with the same install
   discipline as `cinder-home` (keep a `.prev`). This is the highest-value / lowest-effort item.
3. **`dacdat` re-programming — UNBLOCKED, and the interface is explicit.** `dacdat`'s own usage:

   ```
   dacdat ovt FILE            --- output volume table      (ov_*.tbl)
   dacdat dgt FILE            --- device gain table
   dacdat tct FILE            --- tone control table       (tc_*.tbl)
   dacdat auto MODEL VOL_LIMIT
        MODEL     : BBDMP2_linux / BBDMP3_linux / BBDMP5_linux
        VOL_LIMIT : 0 / 10
   ```

   The **stock binary already accepts `BBDMP2_linux`** — Walkman One's model — and `VOL_LIMIT` is a
   bare `0` or `10`, which is the region cap expressed as an argument. All it lacks is the
   1280/127x tables, which are now staged in `/system/usr/share/audio_dac/` on the device
   (inert: `load_sony_driver` runs `dacdat auto $PRODDEV …` with `ro.product.device` still
   `BBDMP5_linux`, so boot keeps loading the 1291 set).

   **NOT YET RUN.** Loading a different output-volume table changes what every volume step does,
   and `VOL_LIMIT 0` removes a cap. That belongs to a deliberate session with headphones OFF, not
   to a background push.
4. **The external tuning blobs — CLOSED, negative.** A 4000-byte encrypted payload behind an
   unknown KAS; see Layer 3 above for the three independent lines of evidence. Not reachable.
5. **The WM1A volume curve — reachable, and the most interesting thing left.** `gain_l` is the
   NW-WM1A's own `ov_127x`/`ov_dsd_127x`, loadable with `dacdat ovt`. See Layer 1b.

## Do NOT confuse this with a region unlock

The limiter is `dacdat limiter_* $shpfirst` where `shp` is an **NVP flag**, not a property. Changing
it is a different, lower-level operation than anything above, and it raises the actual output
ceiling rather than changing tonality. Treat it as a separate decision with its own testing — the
A50's EU cap exists for hearing-safety reasons, and raising it changes what a given volume step
does to your ears, not just what the DAC reports.

## Gotchas

- The prior `analysis/5_stock_vs_w1_diff.txt` is a **sector-level** diff of the packed `.bin`s and
  is useless for feature work. Use the extracted filesystems.
- The UPG entry numbering shifts by one between stock and W1 (W1 inserts a file at index 0), so
  `N.bin` does not mean the same partition in both. Stock system = `6.bin`; W1 system = `7.bin`.
- Both system images are ext4 with the **same UUID** (`57f8f4bc-…`), so mounting both at once needs
  `-o nouuid` or, better, `debugfs` as used here.


## VERIFIED ON THE LIVE DEVICE — 2026-08-18

The ranking above was derived from the extracted filesystems. Checked against the running A55, and
it holds — with one finding that makes item 3 considerably better than it reads.

### Every volume table W1 uses ALREADY SHIPS IN STOCK FIRMWARE

> **WRONG — corrected 2026-09-13.** A stock NW-A50's `/system/usr/share/audio_dac/` holds **55**
> files and only the A50's own set: `ov_1291`, `ov_1291_cew`, `ov_dsd_1291`, `ov_dsd_1291_cew`,
> `tc_1291`. That is the device listing on 2026-09-13, the extracted stock rootfs
> (`artifacts/rootfs_mnt`), and the listing in Wampy's `MAKING_OF_VOLUME_TABLES.md`. The six extra
> files below were on the reference device because an earlier session had copied them in from the
> Walkman One image — `ov_127x`'s md5 `39a60adc…` is W1's `gain_l/ov_127x.tbl` — and a copy that
> keeps the source's mtime makes "dated 2019-07-31" prove nothing. By 2026-09-13 they were gone, and
> `cinder-voltable wm1a` failed on every boot. The md5 and equality notes below still describe the
> files themselves; what is wrong is that a stock player has them.

`/system/usr/share/audio_dac/` on the stock device, 61 files, **all dated 2019-07-31 — Sony's own
build stamp, not anything staged by us**:

```
ov_1291.tbl  bb5ccae7b1a147b3507cb787cda522a6   <- the A50 set, what boot loads today
ov_1280.tbl  5bf930c0209cbe4b7ba871e74e6b2b30   <- BBDMP2, i.e. Walkman One's model
ov_127x.tbl  39a60adc7240be8deab95c39becf4419   <- the NW-WM1A's own curve
```

Three genuinely different files (distinct md5s, same 84950 bytes). Same story for the DSD and tone
tables, with a detail worth noting:

```
ov_dsd_1291 == ov_dsd_1280   but ov_dsd_127x differs     -> the WM1A has its own DSD curve
tc_1280     == tc_127x       but tc_1291     differs     -> the A50 has a DIFFERENT tone-control
                                                            table from BOTH higher models
```

So switching sets changes the volume curve **and** the tone-control table together. And it needs
**nothing from Walkman One at all** — the payload is Sony's, sitting inert on every stock A55,
reachable only because `load_sony_driver` passes `ro.product.device` = `BBDMP5_linux`.

### `dacdat` applies at RUNTIME, through a proc node — no flash, no init hook

```
$ readelf -d dacdat | grep NEEDED     -> libasound
$ strings dacdat                      -> /proc/icx_audio_cxd3778gf_data
                                         /proc/icx_audio_dnc_data
$ ls /proc/icx_audio_cxd3778gf_data/  -> ovt  ovt_dsd  dgt  tct  tct_ng/nh/sg/...
                                         limiter_31/500/750  ainc_*  ambgain
                                         b_nc_gain  u_ncgain_*  c_nw750_nml_0..7  i_data
```

It is a directory of per-table proc nodes, `-rw------- root root`, that the codec driver reads. That
means the tables can be loaded **at any time, by any root process** — and Cinder's launcher already
runs as root, exactly as it already runs `cinder-signature.sh`. No model swap, no partition write,
no boot-order dependency.

`dacdat`'s usage also lists more than this document recorded: `bncgt`/`uncgt` (NC gain tables),
`idata` (IRAM), per-headphone `cnw500n`/`cnw750n`/`cnc31n` CRAM sets, `ambgain`, and a **third**
model in `auto` — `BBDMP2_linux / BBDMP3_linux / BBDMP5_linux`.

### So: what of Walkman One is and is not reachable

| layer | status |
|---|---|
| HAL "sound signature" — ALSA device + CPU floor | **PORTED.** `cinder-signature.sh`, installed |
| CPU clock floor on its own | **FREE, unwired.** `scaling_min_freq` is already `0666` |
| Volume / DSD / tone tables (`ov_*`, `tc_*`) | **FULLY REACHABLE, and needs no W1 files.** All three sets ship in stock; `dacdat` is byte-identical to W1's and applies at runtime |
| `dacdat auto BBDMP2_linux` (whole model profile) | **Reachable.** The stock binary accepts the model |
| External tuning blobs (Bright / Neutral & Warm / WM1Z) | **CLOSED, negative.** 4000-byte encrypted payload, unknown KAS, and the two-time-pad XOR is noise |

**Still NOT RUN, deliberately.** Loading a different output-volume table changes what every volume
step does to your ears, and `VOL_LIMIT 0` removes the EU cap. That is a session with headphones OFF
and a way back (`dacdat ovt ov_1291.tbl` restores stock), not a background push.

---

## MEASURED ON A LIVE WALKMAN ONE PLAYER — 2026-09-21: the updater key

This was the one layer that had never been touched with hardware: **can Cinder install on top of
Walkman One?** It cannot, and the reason is one field.

**The model swap rewrites NVP, and the KAS goes with it.** An NW-A55 running Walkman One answers:

```
# nvpstr kas
e8d171a5d92f35eed9658c03fb9f86a169591659851fd7c49525f587a70b526c
# nvpstr mid
128G
```

That KAS is `upgtool`'s **`nw-wm1a`** entry, not the NW-A50's `dd49de9d…`. `mid` (model id) says
`128G`, which is not an A50 either. So the player is not pretending to be another model at the
property level only — the *firmware identity NVP holds*, which is what the updater reads, has been
replaced. The earlier note that W1 "changes `fwpchk` key/IV by one byte so stock UPGs refuse" is the
same fact seen from the other side.

**What that does to an install, exactly.** A `.UPG` is encrypted with the model's KAS. Sony's updater
refuses one it cannot decrypt, and it does so **silently**: no error screen, no log, no partial write.
The player boots the updater, fails, and reboots into the firmware it already had. Every observable
afterwards says "the installer did nothing":

| checked after the attempt | result |
|---|---|
| `/contents/cinder_home_install.log` | present, all steps succeeded — the staging half is not the half that failed |
| `/contents/cinderhome.log` | **absent** — the payload never ran, so there was nothing to log |
| `/system/vendor/unknown321/bin` | **absent** — nothing was installed |
| `.appcfg` | still `command: HgrmMediaPlayerApp`, dated 2019-07-31 (Sony's build stamp) |
| running Home app | `HgrmMediaPlayerApp` — stock |
| uptime | 1:51 — so the reboot *did* happen; the updater ran and gave up |

The host side is a red herring worth naming: `do_fw_upgrade` printed `An error occured during
request / Trying alternative firmware upgrade command... / ok upgrade command sent.` That is the
normal `0x80` → `0` fallback in the upgrade command, it happens on a stock A50 too, and the package
was accepted for transfer. The rejection is one layer further in, inside the updater.

**The fix, and its status.** `cinder-home/tools/pack_upg.sh <channel> <model>` now takes a model,
validates it against the list `upgtool -n -m '?'` prints, and writes a suffixed artefact so the
stock package is never clobbered:

```
cinder_home_install.upg          sealed nw-a50    (releases ship this one)
cinder_home_install.nw-wm1a.upg  sealed nw-wm1a   (a Walkman One player)
```

Verified off-device: the WM1A package extracts with the WM1A key and yields the real payload
(`#!/bin/sh` / `install_cinderhome.sh`), and yields nothing with the A50 key. **Not yet verified on
hardware** — nobody has watched it install.

**Two things this suggests for the installer**, neither built:

1. **Read the key before flashing.** The player's own KAS is readable, so an installer that knows the
   model can say "this player expects `nw-wm1a`; the package here is `nw-a50`" instead of succeeding
   into silence. Over MSC that means reading NVP off the raw device rather than over adb, which is
   the part that needs work.
2. **This failure mode is not specific to Walkman One.** *Any* model-swapped or mutated player drops
   a stock-keyed package without a word, and the symptom is always "installing Cinder does nothing".
   It is a strong first hypothesis for that report.

---

## MEASURED ON THE LIVE WALKMAN ONE PLAYER — 2026-09-21 (second session): the whole mechanism

The morning's session settled *why an install fails* (the key). This one settles **what Walkman One
actually is on a running player** — and it is smaller and far more reproducible than the
firmware-image diff suggested.

Player: NW-A55, Walkman One installed 2026-04-21, `ro.sony.version` 3.02,
`ro.sony.swid` 01.20.E.1.02.00, `ro.product.device` `BBDMP2_linux`. Everything below is read-only
adb on that player; nothing was written.

### 1. The model swap is a flashed NVP image, and W1 ships three of them

`/etc/.mod/conf_a`, `conf_b`, `conf_c` are **15,728,640 bytes each — exactly the size of
`mmcblk0p22` (`/emmc@nvp`)**. They are whole NVP partition images. `/opt2/stock/` holds the
player's own pre-install pair:

| file | size | is |
|---|---|---|
| `/opt2/stock/conf_bk` | 15,728,640 | **the player's original NVP** (`mmcblk0p22`) |
| `/opt2/stock/nv_bk` | 5,242,880 | **the player's original NVRAM** (`mmcblk0p3`) |

Identity strings carved out of each:

| image | models present | `mid` | KAS |
|---|---|---|---|
| `conf_bk` (your stock A55) | `NW-A50S` | `64G` | `dd49de9d…` = **nw-a50** |
| `conf_a` | `NW-A50S`, `NW-WM1Z` | `128G` | `e8d171a5…` = **nw-wm1a** |
| `conf_b` | `NW-A50S`, `NW-WM1Z`, **`DMP-Z1`** | `128G` | `e8d171a5…` = **nw-wm1a** |
| `conf_c` | `NW-A50S`, `NW-WM1Z` | `128G` | `e8d171a5…` = **nw-wm1a** |

**All three Walkman One images carry the same KAS.** That is the practically important line in this
whole note: the packaging key does not depend on which sound signature the owner chose, so **one
`nw-wm1a`-sealed package covers every Walkman One A50 installation**, not just this player's.

`conf_a` is the image in use here — it differs from the live NVP by **3,480 bytes**, where `conf_b`
and `conf_c` differ by ~29 KB. `/opt2/sig` holds the plain string `wm1z`.

### 2. Which NVP fields were rewritten, by name

`nvp stat zone` prints the kernel's own **named** zone table (93 nodes) to the kernel log — it is
captured in the Sony-files repo as `device/nw-a55/fw-3.02-walkmanone/nvp/zone_map.txt`. The fields
that matter here, read with `nvpstr`:

```
kas e8d171a5…26c   key and signature      -> nw-wm1a   (stock A55: dd49de9d… = nw-a50)
mid 128G           model id               -> not an A50 (stock: 64G)
fpi NW-WM1Z        firmware update Product Identification
ufn NW_WM_FW       update file name
ser 5018758        serial number          (unchanged, matches the adb serial)
pcd 17060730       product code           (unchanged — the real 2017 build date)
```

**`kas` and `fpi` are different fields and W1 rewrites both, to different models.** That resolves
checklist item 16.1: the KAS does not name a family here; it names the key the updater decrypts
with, while `fpi` names the product Sony's *own* update packages check against. Setting `fpi` to
`NW-WM1Z` is precisely what lets a WM1Z-targeted package install on an A55 — the tuning packages'
`SWUpdate.xml` lists `NW-WM1Z` and `NW-WM1A` and nothing else.

The zone table also names three fields this project has wanted for months and never had a name for:
`rflcountry`, `rflsku`, and **`europe vol regulation flag`** (Area 2, zone 7, 4 bytes). The EU
volume cap is an NVP zone with a name.

### 3. The settings file is the whole feature list

W1 generates `/contents/CFW/settings.txt` on first boot and a settings processor reads it at every
boot, logging to `/contents/CFW/boot_log.txt`. Eight settings, and that is all of Walkman One:

| key | meaning | values |
|---|---|---|
| `SIG` | sound signature | 0 Neutral, 1 Warm (Midnight v2), 2 Bright (Dawn v2.1), 3 WM1Z |
| `REG` | region / destination | J, U, U2, U3, CA, CEV, CE7, CEW, CEW2, CN, KR, E, MX, E2, MX3, TW |
| `REM` | show the RMT-NWS20 Bluetooth-remote option under any region | 0 / 1 |
| `PMV` | Plus-mode version | 1 / 2 |
| `PMD` | boot into Plus mode by default (Hold-UP inverts it) | 0 / 1 |
| `GMD` | gain mode | 0 normal, 1 lower |
| `DIM` | DAC initialisation mode | 0 / 1 |
| `COL` | Home-screen icon colour | 0 `#DDDDDD`, 1 `#FFD2B0`, 2 `#FF6757`, 3 `#B1CFE5`, 4 `#AED1B3` |

This player: `SIG=3 REG=MX3 REM=0 PMV=2 PMD=0 GMD=0 DIM=0 COL=0`.

`REG` is a real destination write, not a property — the boot log reports "Settings region is
[MX3], device region is [MX3]", i.e. it compares against the device and rewrites when they differ.
That is the same `shp`/destination surface `load_sony_driver` feeds to `dacdat limiter_*`.

### 4. The external tunings are installed here — and that changes what "unreachable" means

`boot_log.txt` records `The WM1Z external tuning installation was successful!` and later boots
confirm `The [WM1Z] external tuning is installed.`

Re-tested against the player's *actual* key, because §"Layer 3" above predates knowing it:

```
upgtool -m nw-wm1a -e   WM1Z.UPG / Bright.UPG / Neutral.UPG   -> Signature Mismatch
upgtool -m nw-wm1z -e   … -> Signature Mismatch
upgtool -m nw-a50  -e   … -> Signature Mismatch
```

So **Layer 3's negative result stands for the packages**: they cannot be opened host-side, and the
nw-wm1a key does not open them either. (`<Product>` in their manifests reads *"Walkman One — WM1Z
External Tuning"*, so these are MrWalkman's repackaging, not Sony originals.)

**What is new is that the installed result is on the player and is small.** `conf_a` → live NVP is
3,480 bytes, and several of those runs are the NVP's own slot rotation rather than content: the
95-byte and 628-byte records at `0x020000`/`0x0200d8` reappear verbatim at `0x054000`/`0x0540d8`
with the two images swapped. Setting the rotated pairs aside leaves roughly **1 KB of genuinely new
record content**, in two zones, plus the per-device data any player would differ by.

**This is a bound, not an identification.** That ~1 KB still mixes the tuning with this player's own
serial/Bluetooth/calibration data, and nothing here proves the tuning lives in NVP at all. The
experiment that settles it costs one more player-state: dump NVP, change `SIG`, let the processor
re-apply, dump again — the delta between two tunings on the *same* player cancels all the
per-device data. Until that is run, treat "the tuning is a small NVP delta" as the leading
hypothesis and not a finding.

### 5. The UI is stock's, to the byte

Both player binaries are `HgrmMediaPlayerApp`, **the same 9,055,736 bytes**, different md5
(stock `609961954aed…`, W1 `deb923000c89…`) — an in-place patch, not a rebuild.

Carving every embedded PNG out of both (796 each):

> **741 images share an md5. Exactly one differs:** a 480×800 full-screen image — the **Power Off
> screen**, whose WALKMAN logo is white in stock and **orange** in Walkman One.

Carving the embedded QML source out of both (239 blobs each): **12 differ, and every one of them is
a size**.

| blob | stock | W1 |
|---|---|---|
| status-bar block (`sound_quality_info`) | `height: 80`, topMargin 12, sub-topMargin 4, inner 26 | `height: 70`, topMargin 5, 3, inner 15 |
| list rows | 88, 84, 72, 72, 56 | 78, 89, 40, 40, 40 |
| small elements | 28, 28, 22×18 | 12, 20, 15×12 |
| menu | `menuBottomSpace: 106` | `menuBottomSpace: 90` |

The patch keeps every blob's **byte length** identical — `menuBottomSpace: 106` → `90` loses a
character, and the next line gains a leading space to pay for it. That is the signature of a binary
patcher editing strings in place, and it is why the file size matches stock exactly.

**So Walkman One's UI is Sony's UI, tightened.** It adds no screen, removes no screen, and changes
no icon. Anything that reproduces the stock A50 look reproduces the Walkman One look, plus a
recoloured power-off logo, twelve metric tweaks, and a runtime icon tint (`COL`).

### 6. Cinder's install on this player is staged and correct

At the time of writing, `/contents/NW_WM_FW.UPG` is md5 `b6674bffac79f52a141e558892d4b0b6`, which is
**byte-identical to `cinder-home/dist/dev/cinder_home_install.nw-wm1a.upg`**, and the staged
`/contents/cinder-home` matches `dist/dev/cinder-home` (`4e4b6d67…`). `cinderhome-launch.sh` is not
staged and does not need to be — `install_cinderhome.sh` writes it into
`/system/vendor/unknown321/bin/` from a heredoc inside the payload.

`/contents/cinderhome.log` and `/system/vendor/unknown321/bin` are both still absent, so **the
package has not yet been run**. Checklist 16.2 is prepared, not done.

### 7. One operational note for anyone repeating this

With the USB cable connected, the stock/W1 player parks on the **USB Mass Storage** screen and that
screen is modal: taps on the bottom icon row do nothing, there is no back gesture out of it, and MSC
re-arms itself after a short idle (`Auto Activate USB Mass Storage`). A UI tour therefore cannot be
driven over adb with the cable in. Touch injection itself works fine — the panel is
`himax-hx8526-icx` on `/dev/input/event1`, **protocol A**, raw range 960×1600 (2× the 480×800
screen), and a contact needs `ABS_MT_TOUCH_MAJOR`/`WIDTH_MAJOR`/`POSITION_X`/`POSITION_Y` followed
by `SYN_MT_REPORT` then `SYN_REPORT`. A tap built only from `ABS_MT_POSITION_*` + `BTN_TOUCH` is
silently ignored.

Framebuffer capture is free and needs no tool: `/dev/graphics/fb0`, 480×800, 32bpp **BGRA**,
1,536,000 bytes for the visible buffer (`virtual_size` reports 480×2400 — three buffers).

---

## MEASURED ON THE LIVE WALKMAN ONE PLAYER — 2026-09-21 (third session): the boot script, and Cinder running on top

This session answered the two questions the earlier ones left open: **what runs Walkman One**, and
**why Cinder would not start under it**. Both turned out to be smaller than the theories about them.

### Walkman One is a 1452-line shell script

`/sbin/boot_complete.sh` — 43,869 bytes, header `# Walkman One / Settings Processor and Boot Script
v3 / for A50/40/30 / 2021-09-22 / MrWalkman`. It is not a daemon, not a patched binary: it is one
`sh` script that init runs once per boot. Everything the mod does to a running player, it does from
there.

It lives on the **ramdisk**, so it cannot be edited persistently without repacking the boot image.

### The boot order, which is the part that matters

From W1's `init.rc`, `on boot`:

```
exec /bin/sh /sbin/boot_complete.sh     <- BLOCKS init until it returns
start adbd
start sshd
...
exec /bin/sh /system/bin/bootswitcher.sh
  -> setprop sys.sony.bootmode 1
     -> on property:sys.sony.bootmode=1: class_start hagoromo
        -> hagoromo2 = hagodaemon appmgrservice   (user system)
           -> appmgr execs the Home app
```

Three consequences worth keeping:

1. **`adbd` starts BEFORE the Home app.** A Home-app boot loop therefore still leaves an adb window
   on every cycle — which is the opposite of what we assumed during the 2026-09-21 loop.
2. **`/system/bin/bootswitcher.sh` is the last hook upstream of appmgr**, and unlike `/sbin` it is on
   persistent `/system`. That makes it the only place to put a safety net that is strictly *below*
   the thing it rescues. See "The boot guard" below.
3. `boot_complete.sh` blocks init, so **a hang in it is a worse brick than anything it prevents**.

### What it re-applies on every boot

`/system` is mounted **rw** (stock mounts it `ro`) and remounted **ro** at the end of the script.
In between, the payload under `/system/etc/.mod/` is copied into place:

| `.mod` source | destination | what it is |
|---|---|---|
| `adler/$MODE/libaudiohal-adleralsa.so` | `/system/vendor/sony/lib/` | the "sound signature" |
| `anls/$MODE/*` | `/system/vendor/sony/etc/audioanalyzer_params/` | analyser tuning |
| `gain/gain_n` or `gain_l/*` | `/usr/share/audio_dac/` | `GMD`, "Gain mode" |
| `lang/$SIG/{nt,nr,pv1,pv2,np_*}/*` | `/system/vendor/sony/translations/` | `rm` first, then copy |
| `conf_a`, `conf_b`, `conf_c` | NVP | the three model config images |
| `tunings/*`, `stockrevert/*` | `/contents/CFW/` | copied out for the user |

`$MODE` and `$SIG` come from the settings file, so the signature is a **whole-file swap from a
per-mode variant set**, re-done every boot — not a patch applied once at install time.

### State lives in /opt2

W1 mounts `option2` separately (`mount ext4 /emmc@option2 /opt2 rw`) and drops it from the
`mount_partition` list; it is remounted `ro` at the end of the script.

* `boot_count` — incremented **only when the script reaches its end**, so it is a clean
  "did the firmware finish booting" counter, and a reliable loop detector.
* `sig` — the signature name in force (`wm1z` here).
* `stock/conf_bk` — **15,728,640 bytes, exactly the NVP partition**.
* `stock/nv_bk` — **5,242,880 bytes, exactly NVRAM**.

So W1 keeps a full backup of both the NVP and NVRAM it overwrites. That is the supported way back,
and it is on the device rather than in the installer.

### The settings file, including a key Wampy does not know

`/contents/CFW/settings.txt`, parsed with `awk -F "=" '/^KEY/ {print $2}'`. Keys: `SIG` `REG` `REM`
`PMV` `PMD` `GMD` `DIM` `COL` — and **`ADB`**, which `w1.cpp` logs as "unexpected key" and which
older generated settings files omit entirely:

```
ADB=1   -> setprop persist.sys.sony.icx.adb 1
ADB=2   -> setprop persist.sys.sony.icx.adb 0
ADB=0 or absent -> leave the device as it is
```

That is the supported way to keep adb across W1 boots, and it is what makes developing against
Walkman One practical at all.

The script's own log is `/contents/CFW/boot_log.txt`. On this player it reports the WM1Z signature
selected (`SIG=3`) but **the WM1Z external tuning NOT applied** — `Normal (no tuning) mode
initialized` — so a player can be "on" a signature without the tuning being in force.

### Other init.rc deltas vs stock 1.02

* `icx_syslog` is **never started** — the service definition survives, the `start` does not. This is
  why appmgr's side of a failure is invisible on W1. `setprop ctl.start icx_syslog` brings it back.
* `load_sony_driver` is removed as a service; its `insmod`s moved into `boot_complete.sh`.
* `/system` mounted `rw`, `option2` mounted separately, scheduler and block-queue tuning added.
* **`init.hagoromo.rc` is byte-for-byte the stock one.** The Home-app launch path is unchanged.

### FM radio: the chip is present and answering

W1 identifies as NW-WM1Z, a model with no tuner, so the stock FM UI never loads. The hardware is
untouched:

```
insmod /system/lib/modules/radio-si4708icx.ko    # rc 0, creates /dev/radio0
regmon Si4708icx:*
DEVICEID -> 0x00001242    # Silicon Labs Si4708 — matches Wampy's reference dump exactly
CHIPID   -> 0x00001000    # powerdown (a powered-up part reads 0x1093)
POWERCFG -> 0x00002000    # ENABLE clear
```

`libTunerPlayerService.so` is present (79,916 bytes, md5 `2e3123c7482197e43b625f30c8810daf`) and
`hagoromo28` starts `TunerPlayerService`, so the service is running; whether it is the mock shipped
to chipless models still needs a diff against stock 1.02's copy. The ALSA control the audio path
needs is there too: `numid=26 'analog input device'`, items `off`, `tuner`, …

Wampy's `MAKING_OF_FM.md` documents the rest and the two traps: the player sets power state to `mem`
on a power press (hold a **wakeup source**), and a service flips `analog input device` back to `off`
on a timer — which must be **polled**, because that mixer is driven by `ioctl`, not filesystem
events, so `amixer sevents`/`monitor.c` see nothing. The UI gap Wampy works around by drawing its
own FM screen is a screen **Cinder can simply own**; `cinder-fm` already exists in the tree.

### Cinder runs on Walkman One — and the blocker was ours

**`ps` on a W1 boot: `system 830 495 /system/vendor/unknown321/bin/cinder-home`.** The full easel
handshake completes (`ToInitialize → ToPostInitialize → ToActivate → OnForeground`),
`/data/cinder/bootcount` reads `0` (cinder-home's own "painted and proved healthy" signal), the
cable pass is spent, and the log is clean.

**The cause had nothing to do with Walkman One.** `install_cinderhome.sh` created its state
directory with `mkdir -p /data/cinder` as root under **umask 077**, leaving it **`0700 root:root`**.
The launcher and cinder-home run as **uid 100** — which on this device *is* the user `system`
(`/etc/passwd`: `system:…:100:100:`), because appmgr's service line `hagoromo2` is `user system` and
the Home app inherits it. So the launcher could not:

* `rm` `cable_pass_once` — **deleting a file needs write permission on the directory** — so
  `CABLE_PASS_SPENT` stayed `0`, and with a cable always connected **the rung-0 cable escape fired on
  every boot**, `exec`ing Sony's player;
* create `bootcount`, or write its breadcrumb to `/data/cinder/cinderhome.log`.

`/contents` is not mounted that early, so the second breadcrumb path failed too.

**The trap worth remembering:** the resulting state — Sony's app running, no `bootcount`, no
breadcrumb, an unspent cable pass — is *indistinguishable from "appmgr never exec'd the launcher"*.
That wrong conclusion was drawn twice. **A launcher that ran and took an escape looks exactly like
one that never ran, unless it can write somewhere.** A probe script that logs only to
`/data/cinder` or `/contents` proves nothing at boot; use `/var/log` (init makes it `0777`) or
`/tmp`.

Fix: `chown 100:100` + `chmod 0755` on the directory **and** on `cable_pass_once`. Note that
`chmod 755` alone — the fix already applied to `$VT_DIR` higher up in the same installer — is not
enough here, because this directory must be *written*, not merely read.

### Two hypotheses tested and killed, so they are not retried

* **ABI.** All 13 Sony libraries Cinder links export identical symbols on 3.02 and 1.02;
  `libeaselcore`, `libeaselcui`, `libpstcore` and `libappmgrservice` are byte-identical.
* **`.appcfg.real` inside appmgr's scan directory.** appmgr `readdir_r()`s a hardcoded
  `/system/vendor/sony/bin` for `.appcfg` files and `execvp()`s the `command:` it finds; the
  installer's backup sits in that same directory declaring the same `name:`. Plausible, so the
  backup was moved to `/system/vendor/unknown321/` and the boot retried — **no change**. appmgr
  does honour an absolute `command:`.

### The boot guard

`/system/bin/cinder-guard.sh`, called from a six-line hook in `/system/bin/bootswitcher.sh` placed
immediately before `setprop sys.sony.bootmode`. Shipped as
[`cinder-home/deploy/cinder-guard.sh`](../cinder-home/deploy/cinder-guard.sh); see that file's header
for the rationale and the install steps.

It exists because the launcher's own bad-boot counter can only advance **if appmgr execs the
launcher** — so it cannot rescue a failure that happens earlier, and in a loop below it the player
has to be recovered with wbrt. The guard counts unproven boots in `/db/cinder-guard/count` and, at
3, restores `.appcfg.real` over `.appcfg` so Sony's app comes back on its own. It depends on init,
`/db` and `/data` only.

It was installed before the Cinder install on this session and behaved correctly on every boot,
including logging the diagnostic that framed the investigation. It is **not** wired into the
installer: it has only a handful of boots behind it, and a script that init blocks on is not
something to enable for everyone on that evidence.
