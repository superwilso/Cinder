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
