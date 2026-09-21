# Walkman One parity — the plan

**Written 2026-09-21 against a live NW-A55 running Walkman One.** A plan, not a change.

## 0. The goal, as given

> Full Walkman One compatibility with all features retained, and keep the main UI design of
> stock / Walkman One but with some tweaks to make it easier to access some settings.

And the question behind it: **how much of this cake can be had and eaten?** — how much of Clear
Bass and SensMe can be ported or recreated rather than given up.

This document answers that as a ledger. The supporting measurements are in
[`../analysis/RE_walkmanone_extract.md`](../analysis/RE_walkmanone_extract.md) §"second session",
and the raw material is in the Sony-files repo under `analysis/walkman_one/` and
`device/nw-a55/fw-3.02-walkmanone/`.

---

## 1. The one fact the whole plan rests on

**Walkman One is not in the Home app. It is underneath it.**

Its settings processor is `/sbin/boot_complete.sh` — a 1,452-line shell script in the ramdisk,
"Walkman One / Settings Processor and Boot Script v3 / MrWalkman / 2021-09-22". init runs it at
boot. By the time any Home app starts, it has already:

* flashed the chosen NVP image over `mmcblk0p22` (if the signature changed) and rebooted;
* swapped `libaudiohal-adleralsa.so` for the chosen Plus-mode variant;
* swapped `/system/vendor/sony/etc/audioanalyzer_params/*` to match;
* swapped `/usr/share/audio_dac/ov_127x*.tbl` for the chosen gain mode;
* re-run the whole `dacdat` programming sequence — volume/tone tables, the three NC CRAM sets,
  IRAM, the ambient gains, and `limiter_31/500/750` with the region code;
* written the icon-colour NVP flag (`nvpflag clv`);
* remounted `/system` read-only again.

Cinder installs by adding binaries under `/system/vendor/unknown321/` and repointing `.appcfg`. It
changes nothing on that list.

> **So the default answer to "does Cinder on Walkman One keep this?" is *yes, untouched* — for
> everything that is audio, and *no* for everything that is a screen.** The rest of this document
> is that sentence made precise.

---

## 2. Where Walkman One stands today

| | |
|---|---|
| Player | NW-A55, Walkman One installed 2026-04-21, `ro.sony.version` 3.02, `BBDMP2_linux` |
| Settings | `SIG=3` (WM1Z), `REG=MX3`, `REM=0`, `PMV=2`, `PMD=0`, `GMD=0`, `DIM=0`, `COL=0` |
| External tuning | **WM1Z installed** (`/opt2/sig` = `wm1z`) |
| NVP identity | `kas` = nw-wm1a, `fpi` = `NW-WM1Z`, `mid` = `128G` |
| Cinder | **not installed.** The correct package is staged and verified; nothing has run |

**The install is one step away.** `/contents/NW_WM_FW.UPG` is byte-identical to
`cinder-home/dist/dev/cinder_home_install.nw-wm1a.upg`, the staged `cinder-home` matches
`dist/dev/cinder-home`, and the launcher ships inside the payload. Checklist item **16.2 is
prepared and unrun** — it needs a deliberate flash, and it is the gate on everything below.

Two facts make that flash meaningfully safer than it looked this morning:

1. **All three of Walkman One's NVP images carry the same KAS.** One `nw-wm1a`-sealed package
   covers every Walkman One A50 installation, not just this player's — so this is a reusable
   result, not a per-device fix.
2. **Walkman One keeps its own restore material on the player**: `/opt2/stock/conf_bk` is the
   player's original NVP (KAS `dd49de9d…` = nw-a50, `mid` `64G`) and `/opt2/stock/nv_bk` its
   original NVRAM, plus `StockRevert_Walkman_One_A50.exe` in `/system/etc/.mod/stockrevert/`.
   That is not a wbrt backup and does not replace one — it restores identity, not partitions.

---

## 3. Ledger A — Walkman One's own eight settings

`settings.txt` documents eight keys. There is a ninth the file never mentions.

| setting | what it really does | under Cinder |
|---|---|---|
| **`SIG`** sound signature | Picks the NVP image (`conf_a/b/c`), which sets `fpi` so the matching external tuning will install; then picks the HAL variant | **Kept, untouched.** All of it is boot-time and below the app |
| **`REG`** region | Rewrites the destination in NVP; feeds `dacdat limiter_*` | **Kept, untouched** |
| **`REM`** show the RMT-NWS20 remote option | A stock-UI option unlocked regardless of region | **Lost** — it is a screen. Re-creatable as a Cinder Bluetooth setting; the remote itself is a BT peer like any other |
| **`PMV`/`PMD`** Plus mode + hold-switch default | Swaps `libaudiohal-adleralsa.so` (ALSA device + CPU floor) and the analyser delay tables. **Also swaps the Qt `.qm` translations**, which is how the mode is named in the stock UI | **Audio kept, untouched. The naming is lost** — those `.qm` files belong to `HgrmMediaPlayerApp`. Cinder should read the mode itself and say which one it is |
| **`GMD`** gain mode | Copies `gain_n` or `gain_l` over `/usr/share/audio_dac/ov_127x*.tbl` before `dacdat` runs. `gain_n` is the A50's own curve renamed; `gain_l` is the NW-WM1A's | **Kept, untouched.** Cinder's own `cinder-voltable` is a *second*, runtime route to the same tables — they must not both drive it |
| **`DIM`** DAC initialisation mode | `0` runs the full stock sequence; **`1` runs only `dacdat auto` and skips the NC CRAM sets, IRAM, ambient gains and all three limiters** | **Kept, untouched.** Worth knowing what it means: `DIM=1` leaves the output limiter unprogrammed |
| **`COL`** Home icon colour | `nvpflag clv` ← 0 / 3 / 5 / 7 / 9 (Default / Peach / Red / Blue / Green). That is NVP Area 6 Zone 0, **"color variation"** | **Free to keep.** The flag survives; Cinder reads `nvpflag -x clv` and tints its own icons the same five ways |
| **`ADB`** *(undocumented)* | `ADB=1` / `ADB=2` sets `persist.sys.sony.icx.adb`. Not mentioned anywhere in `settings.txt` | **Useful now.** A Walkman One owner can enable adb by editing a text file over MSC — no flash, no adb enabler |

**Score: six of eight kept for free, one free to re-create (`COL`), one genuinely lost (`REM`) and
one half-lost (the Plus-mode *label*, not the Plus mode).**

### One fragility worth documenting

`boot_complete.sh` guards every apply block with `TMD5 = CMD5`, where `CMD5` is the md5 of
`/dev/block/mmcblk0p3` (NVRAM) and `TMD5` is a constant per signature (`ccb29dd2…` for WM1Z,
`d7d08780…` for the others). **If NVRAM ever stops matching, Walkman One silently falls back to
"Normal (no tuning)" mode and says so only in `boot_log.txt`.** Cinder must never write
`mmcblk0p3`, and "my sound signature stopped working" should send anyone straight to that log.

---

## 4. Ledger B — Clear Bass

**Short answer: the *feature* is reachable; Sony's *implementation* of it currently is not.**

What is already settled (`analysis/RE_clear_bass.md`):

* Clear Bass is **band 0 of the A50's six-band EQ**, −10…+10 in whole dB.
* Its filter is fully decoded. At 44.1 kHz and +10 it is a **boost resonator at ≈47 Hz** in
  parallel with a **cut around ≈100 Hz**, each through a limiter, added to the dry signal. Both
  coefficient tables are decoded to CSV in the Sony-files repo.
* Driving it through `EffectCtrlDmp` **measured no effect at the jack** (2026-09-17): ±0.6 dB at
  40–63 Hz, against a control on Cinder's ten-band that measured **+7.9 dB**. The writes arrive —
  the service logs the parameter and `UpdateProcCond … isproc is 1` — and are then followed by
  `no desired value, skip`.

So there are two routes, and they are independent:

**Route 1 — make Sony's six-band engage.** Read `Eq6band::UpdateProcCond` and the path from
`ExecEffectParam("eq6band,band=…")` to `CB_6bandEQ_*` in `libSoundServiceFw.so`, and find what
"desired value" it is waiting for. **This player makes that much cheaper than it was**, because
Walkman One's stock UI drives the same six-band and can be used as a working reference: set Clear
Bass from the stock Sound Settings screen, capture the service's log and the effect state, and diff
against what Cinder writes. That comparison was impossible before — there was no known-good caller.

**Route 2 — recreate it on the ten-band, which already works.** The coefficients are decoded, the
shape is known, and Cinder's ten-band demonstrably moves the jack by +7.9 dB. A "Clear Bass"
control that maps −10…+10 onto the 47 Hz-up / 100 Hz-down shape would not be bit-identical to
Sony's parallel-bandpass-plus-limiter, but it would be the same intent, measurable, and shippable
without waiting on Route 1.

**Recommendation: do Route 1's measurement first** — it is now a couple of hours with the player in
hand rather than open-ended RE — and keep Route 2 as the fallback that does not depend on it.
Either way the answer to "can I have Clear Bass on Cinder" is **yes**; what is open is whether it is
Sony's filter or a faithful reconstruction.

One caution carried over: `cinder-probe --clearbass` opens a second PlayerService client, which has
previously caused silence and a leaked session ending in a power-off. Do not run it while the stock
app owns the player.

---

## 5. Ledger C — SensMe

**Short answer: essentially solved, and it works on Walkman One with or without Cinder.**

Established 2026-09-17 (`analysis/RE_sensme_musiccenter.md`, `docs/PLAN_sensme_sync.md`):

* Sony's classifier (`MMLib11.dll`, from Music Center) produces **~6 KB per track**, not the
  megabyte Walkman One's author warned about.
* The **player's own scanner** parses exactly those chunks and computes the five axes, the channel
  bitmask and the chorus position itself.
* Tagging the Walkman's *copy* of a file — FLAC `SMFM` application block, MP3 `GEOB` — and letting
  the device scan it **produced real channels on the device.** Verified: three Flint-tagged FLACs
  scanned, TEMPO/MOOD/TYPE/STYLE/TIME, channel bitmask (akey 50) and SABI (akey 60) all written.

Because the computation happens in `MediaStoreService`, which Cinder does not replace, **this is
real SensMe on stock firmware, on Walkman One, and on Cinder alike.** The PC library is never
touched; the cost is about 21 MB across a 3,500-track library.

What is left is not RE:

1. **Flint** (the Rust sync tool, `/home/sony/flint`) finishing the tagging path for MP3 as well as
   FLAC. Requires Windows + Music Center for the engine, which is Sony's and not shipped.
2. **Cinder's channel-browse screens** — `CINDER_SENSME` is `0` in the staged
   `cinder_components.conf` today. Channel membership comes from MTPDB, not a sidecar.
3. A non-Windows fallback analyser stays the long-tail option (PR #13's design), not the first
   release.

---

## 6. Ledger D — the stock player's own features

"All features retained" mostly means *Sony's* features, not Walkman One's — Walkman One adds eight
settings to a player that already does a great deal. The screen inventory is
`analysis/ui_assets/QML_INDEX.md` in the Sony-files repo (180 carved screens).

The honest position, by area:

| area | state |
|---|---|
| Library browsing, Now Playing, transport, playlists, search | Cinder has these |
| Queue, shelf | Cinder has these; **stock does not** — net gain |
| Ten-band EQ, tone control, DSEE HX, DC Phase, Dynamic Normalizer, Vinyl, VPT | Sony's services are reachable; Cinder wraps 13 of Sony's ~54 effect methods today. **This is the largest parity gap and it is ordinary work, not RE** — the enums are already recovered from the `.qm` catalogues |
| Clear Bass / six-band | §4 |
| SensMe channels | §5 |
| Bluetooth, LDAC, codec selection, wireless quality | Cinder has these |
| FM tuner | Cinder has it; the audio path is via `AudioInPlayerService` and is known-hazardous |
| USB-DAC | Gated on `FuncMode==1`; recovered, not shipped |
| Noise cancelling / Ambient Sound Mode | **Inert without Sony's own NC headphones.** Not a Cinder limitation |
| Language Study, Alexa, Help Guide | Not planned; say so rather than imply parity |
| Clock, USB-MSC, Reset/Format, initial setup | Cinder has clock and MSC; format/reset is stock-only today |

---

## 7. The UI plan

**Walkman One's UI *is* Sony's UI.** Measured by carving both player binaries — the same
9,055,736 bytes, an in-place patch:

* 796 embedded images each; **741 share an md5 and exactly one differs** — the Power Off screen's
  WALKMAN logo, white in stock and orange in Walkman One;
* 239 embedded QML blobs each; **12 differ and every difference is a size** (rows 88→78, 84→89,
  72→40, 56→40; status block 80→70; `menuBottomSpace` 106→90);
* plus the runtime icon tint from `COL`.

So there is no separate "Walkman One look" to chase. The design target is the stock A50 UI,
tightened.

The design spec — canvas, grid, type, colour, component heights, screen inventory, and how to
reproduce the captures — is
`analysis/ui_assets/UI_DESIGN_SPEC.md` in the Sony-files repo, sitting next to the 796 carved
assets, the 180 carved screens and live framebuffer captures. **That is the artefact a design
session should be pointed at.**

### The tweaks, stated as a brief

Keep exactly: 480×800 canvas, 20 px gutters, the 2 px `#9C9A9C` hairline under the screen title,
near-monochrome on black, SST at 16/26/28/32, generous row heights.

Spend the freedom on what the stock UI is bad at:

1. **Sound Settings is a deep tree** — the effects people toggle most each live on their own leaf.
2. **Settings changed daily and settings changed once share one flat list** — no favourites, no
   recents, no search.
3. **The Home icon grid spends a screen on navigation the bottom bar largely repeats.**
4. Walkman One's own eight settings are **a text file edited over USB**, with a reboot per change.
   Surfacing them as real rows — signature, region, gain, Plus mode, icon colour — is the single
   most valuable "easier to access" win available, and six of the eight are readable from NVP or
   the filesystem without any new RE.

Point 4 is the part that makes this a Walkman One build rather than a reskin.

---

## 8. Order of work

| # | Step | Why here | Blocked on |
|---|---|---|---|
| 1 | **Run checklist 16.2** — flash the staged `nw-wm1a` package | Nothing below can be tested until Cinder runs on this player | A deliberate flash decision |
| 2 | **16.3** — play a track, read the volume-curve line in `cinderhome.log`, toggle the signature | W1 ships different `audio_dac` tables; `voltable` and `signature` are the two expected to disagree | 1 |
| 3 | Confirm `boot_complete.sh` still runs and `boot_log.txt` still advances with Cinder as Home | The whole of §3 assumes it; assumed, not observed | 1 |
| 4 | **Clear Bass Route 1 measurement** against the stock UI as reference | Cheapest it will ever be, and it decides §4 | cable out |
| 5 | Read `clv` and honour it; surface the W1 settings as rows | Free features, and the point of §7.4 | 1 |
| 6 | The effects parity work (13 → the rest of Sony's ~54 methods) | Largest parity gap, ordinary work | 1 |
| 7 | SensMe channel screens; `CINDER_SENSME=1` | §5 is done except the UI | Flint's MP3 path |
| 8 | The NVP delta experiment for the external tuning (dump, change `SIG`, dump) | Settles whether the tuning is reachable at all | willingness to change signature |

---

## 9. What is still not known

* **Whether `boot_complete.sh` survives with Cinder as Home.** It is an init-time ramdisk script and
  should be entirely independent, but that is reasoning, not a measurement. Step 3.
* **Whether the external tuning is in NVP.** `conf_a` → live NVP is 3,480 bytes, and after
  discounting the NVP's own slot rotation roughly 1 KB is genuinely new — but that still mixes the
  tuning with this player's serial, Bluetooth and calibration data. The packages themselves remain
  undecryptable, with the player's own key and every other (`Signature Mismatch`). Step 8.
* **Why Sony's six-band does not engage.** §4 Route 1.
* **Whether `cinder-voltable` and Walkman One's `GMD` can coexist** — both write the same tables by
  different routes, at different times. At minimum Cinder should detect Walkman One and default
  `CINDER_VOLTABLE=stock`, which is what the staged config already says.
