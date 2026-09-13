# RE — the region volume tables

**Date:** 2026-09-04 · **Status:** measured on hardware, with a control experiment.
**Result: WITHDRAWN 2026-09-13** — see [the amendment](#amended-2026-09-13--the-sweep-could-not-see-the-difference)
at the end. The sweep below reads the one register the two files agree on, so its "identical"
rows say nothing about the region difference. The original text is kept as it was written.

## The hypothesis

`/bin/load_sony_driver` runs `dacdat auto $PRODDEV $midupper $shpfirst`, where `shpfirst` comes from
the NVP flag `nvpflag -x shp`. This unit reads **`0x00000006`** and its `swid` is `03.01.E.1.02.00`
— a European unit. Sony ships every model's volume table twice:

```
ov_1291.tbl        ov_1291_cew.tbl        84950 bytes each
ov_dsd_1291.tbl    ov_dsd_1291_cew.tbl    13076
```

and `dacdat`'s string table carries both paths for every model, so it picks between them at boot.
The two files differ in **7576 bytes**, in a repeating 13-byte record, and at every difference the
`_cew` value is **lower** — e.g. `0xb3ef` → `0xa200`, about −10%. That is exactly the shape of a
region-restricted output table, and it looked like the EU cap.

## Two dead ends on the way

**The limiter files are not it.** `limiter_500.bin`, `limiter_750.bin`, `limiter_31.bin` and all
three `_cew` counterparts are **3 bytes each and byte-identical to one another** (`0a 0f 9f`).
Whatever they are, they carry no region difference at all.

**`ovt` cannot be read back.** `/proc/icx_audio_cxd3778gf_data/ovt` is write-only in practice — a
read returns nothing — so the live table cannot simply be compared against the two candidates. (An
early attempt appeared to read 85184 bytes; that was adb's LF→CRLF mangling, not data.
`adb exec-out` returns 0 bytes, which is the truth.)

**`dacdat`'s selection was not recovered.** Its path strings are at file offsets 0x390c/0x3968 with
**no literal-pool references** — they are reached PC-relatively — so the branch that picks `_cew`
was not read out of the binary. This was left unresolved because measurement turned out to answer
the question directly.

## The measurement

Every volume step moves the codec's analogue attenuator directly (`0x49 PHV_L` / `0x4b PHV_R`), so
reading PHV back across the range **is** the curve — objective, silent, nothing playing. That is
what `cinder-probe --volcurve` does, and it reproduces the previously documented stock curve exactly.

> **Amended 2026-09-08** — PHV is only half of it. Volume is split between the analogue attenuator
> and a digital one (`CODEC_SDIN2VOL`, `0x29`), and in the PHV flat zone at vol 40–60 the digital
> stage is still moving: measured at the jack, that zone gives 1 dB across 50–70 rather than nothing.
> The 100–120 zone is the genuinely dead one — both stages pin there. So a PHV-only sweep
> over-reports the lower dead zone and under-reports the upper. Acoustic measurements of both curves,
> and the correction in full, are in `RE_headphone_amp_modes.md`.


| master volume | 0 | 20 | 40 | 60 | 80 | 100 | 120 |
|---|---|---|---|---|---|---|---|
| **boot state** | 4 | 80 | 100 | 100 | 148 | 228 | 228 |
| **`eu` applied** (`ov_1291_cew`) | 4 | 80 | 100 | 100 | 148 | 228 | 228 |
| **`stock` applied** (`ov_1291`) | 4 | 80 | 100 | 100 | 148 | 228 | 228 |
| **`wm1a` applied** (`ov_127x`) | 4 | **44** | **84** | **124** | **164** | **204** | 228 |

Two dead zones in the shipped curve, confirmed: PHV pins at 100 across volume 30–60, and at 228
across 100–120.

## The control experiment is what makes this trustworthy

A negative result from "I wrote a table and nothing changed" is worthless if the write never landed.
`wm1a` is a table already documented to change the curve, and applying it **does** — the bottom row
above is monotonic with no dead zones and matches the previously measured WM1A sweep. Same tool,
same session, same instrument. So the writes take effect and the instrument works, which means the
identical `eu`/`stock` rows are a real finding and not a broken test.

## Conclusion

**`ov_1291.tbl` and `ov_1291_cew.tbl` produce identical wired volume curves.** The 7576 differing
bytes live somewhere the PHV mapping does not read — the tables are 84950 bytes and the volume
mapping is only ~121 entries, so most of the file is something else (the name `ov` suggests
*overload* limiting, and there is a separate DSD table). Whatever the region difference does, it is
not the headphone volume curve.

So: **there is no EU volume cap to lift here.** The genuine improvement available is the WM1A curve,
which removes both dead zones and makes the whole range usable — and that was already implemented.

Not established: what the `_cew` bytes *do* control, and which of the two files this device boots
with. Both are now moot for output level, which is why neither was chased further.

## Amended 2026-09-13 — the sweep could not see the difference

Wampy's [`MAKING_OF_VOLUME_TABLES.md`](https://github.com/unknown321/wampy/blob/master/MAKING_OF_VOLUME_TABLES.md)
measured an NW-A50's line output at the jack across all 19 regions (REW, 28 runs, effects off) and
found exactly three settings that change the signal: regions **CEW2** and **KR3**, which boot the
`_cew` table, and gain mode 1. That contradicts the conclusion above, so the two files were decoded
field by field instead of compared as bytes.

**Layout.** Wampy's `src/dac/cxd3778gf_table.h`, taken from Sony's kernel source: sound effect
off/on × 27 output tables × volume 0..120 × 13 one-byte register values, then a sum/xor checksum.
That is `2 × 27 × 121 × 13 + 8` = **84950 bytes, exactly the file size.** The "repeating 13-byte
record" above is one volume step.

**What `_cew` changes.** 7569 data bytes, all inside tables 1–9 (the S-Master single-ended, BTL and
class-AB headphone tables, with their noise-cancel and ambient variants). Line out and every DSD
table are identical. In the two tables this model's headphone path uses — `SMASTER_SE_LG` and
`SMASTER_SE_HG` — **only `play` and `sdin2` differ. `hpout` is identical in both files.**

| table 1, sound effect off | vol 0 | 40 | 60 | 80 | 120 |
|---|---|---|---|---|---|
| `hpout` (the PHV attenuator) | same | same | same | same | same |
| `play`, plain → `_cew` | 179 → 162 | 216 → 199 | 236 → 219 | 0 → 239 | 0 → 239 |
| `sdin2`, plain → `_cew` | 239 → 0 | 239 → 0 | 219 → 236 | 199 → 216 | 167 → 184 |

**So the 2026-09-04 sweep read the one field the two files agree on.** `cinder-probe --volcurve`
reads `PHV_L`/`PHV_R`, which is `hpout`. Identical rows were guaranteed whatever the tables do to the
signal. The control experiment proved the writes land; it could not prove the instrument sees this
kind of difference, because the WM1A table differs in `hpout` and the region pair does not. **"There
is no EU volume cap to lift here" is withdrawn.**

What is and is not established now:

- **Established:** `_cew` moves `play` and `sdin2` by 17 codes at every step, in opposite
  directions, and leaves the analogue attenuator alone.
- **Not established:** what a code is in dB, and how the two digital stages combine. That needs
  Sony's driver (`cxd3778gf_volume.c` in the kernel source), which has not been read. Wampy's jack
  measurement says the net effect on a CEW2/KR3 unit is quieter.
- **Inferred, from one register read:** this unit boots the **plain** table.
  `RE_headphone_amp_modes.md` read `CODEC_SDIN2VOL` (`0x29`) = `0xC7` (199) at master volume 60;
  that is `ov_1291`'s sound-effect-on value at that step, and `_cew`'s is 216. A European unit
  (`shp` 0x6) is therefore probably not a restricted one, and the curve work in `RE_volume_pop.md`
  stands.
- **The consequence that matters:** `cinder-voltable`'s `stock`, `w1` and `wm1a` are all **plain**
  tables. On a CEW2 or KR3 unit, loading any of them removes the region restriction, so the same
  volume number is louder than its owner is used to.

**To settle it:** the jack rig from `RE_headphone_amp_modes.md` — line input, the fixed tone file,
volume pinned — with `eu` against `stock`, nothing on anyone's head. Not `--volcurve`. Tracked as
`docs/DEVICE_CHECKLIST.md` 13.6.

## What was wired

`cinder-voltable` gained:

- **`eu`** — the `_cew` pair, so the comparison above can be repeated by anyone.
- **`tone-stock` / `tone-w1` / `tone-wm1a`** — the tone-control tables (`tc_*.tbl`, 2888 bytes) into
  `/proc/icx_audio_cxd3778gf_data/tct`. Sony loads one at every boot and nothing had wired them.
  These have **no `_cew` variant**, so tone is not region-restricted. Kept as separate keys so that
  applying a volume curve does not silently also change tone.

`cinder-probe --volcurve [step] [force]` — the instrument. Restores the original volume, and refuses
to sweep with something in the jack unless forced.

## Related

- `analysis/RE_volume_pop.md` — the same attenuator, and where the curve measurement came from
- `analysis/RE_walkmanone_extract.md` — where the table swap idea came from
- `cinder-home/src/cinder-voltable.c` — the helper
