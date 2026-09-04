# RE — the region volume tables, and why there is no cap to lift

**Date:** 2026-09-04 · **Status:** measured on hardware, with a control experiment.
**Result: negative.** The region difference is real in the files and does **not** reach the wired
volume curve. There is no EU output cap reachable this way.

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
