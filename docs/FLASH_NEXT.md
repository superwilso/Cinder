# Next flash — run sheet

Built **2026-07-28** from `HEAD`. Everything below is staged and packed:
`cinder-home/dist/dev/` and `cinder-home/dist/stable/`.

This is the first hardware run in **35 commits**. Several are on the boot path, so the order below
is a safety gradient, not a preference — each step can only fail in ways the previous step has
already ruled out.

---

## Before you plug anything in

**Confirm you have a wbrt backup of THIS unit.** It is the only thing that recovers a brick, it is
device-specific, and the 07-26 brick was recovered with one.

---

## Step 1 — LDAC, under stock (no boot risk at all)

Do this **first**, before flashing anything. It runs under Sony's own firmware, needs no Cinder
install, and cannot affect the boot path. It is also **goal #3 — the reason the project exists —
and it has never been run end to end.**

Follow `ldac-bridge/TEST.md`. Its two unknowns each have a documented three-outcome table:
does `SetCurrentSource(true)` open the server socket, and is the USB-DAC capture `-EBUSY`.

If you only get time for one thing this session, make it this one.

---

## Step 2 — Probe (still no boot risk)

`cinder-probe` has no easel lifecycle, so it cannot touch the bad-boot counter. Push it and run it
over adb. This de-risks the whole unverified batch before any of it can affect a boot.

```sh
adb push cinder-home/dist/dev/cinder-probe /data/local/tmp/
adb shell 'cd /data/local/tmp && chmod 755 cinder-probe && \
  LD_LIBRARY_PATH=/system/vendor/sony/lib:/system/vendor/unknown321/lib:/system/lib:/usr/lib:/lib \
  ./cinder-probe'
```

Three runs matter, in this order:

| Run | Why it matters |
|---|---|
| `--analyzer` | **Never run once.** Until 07-28 Cinder never called `SetPassband`, and the service reports nothing until told which bands to analyse — the likely reason the visualiser has never produced a frame. If this prints "frames flowed", the visualiser works. If it still prints nothing, there is a second cause and the visualiser pages are dead. |
| `--pump` **with music playing** | Re-confirms playback, and this is the run that can finally capture a **PlayStatus dump with audio actually running**. Every previous dump was all zeros because nothing was playing, which is why the byte offsets are still unmapped. |
| `--discover` | Refreshes the device dump. Cheap, and it is what every future offline decision is made against. |

Note the raw band values `--analyzer` prints. `spectrum::from_bands` auto-detects dBFS vs linear, so
most encodings work as-is, but a range far outside 40k–millions linear or 0..−60 dB is worth
recalibrating against.

---

## Step 3 — Flash `dist/dev`, cable OUT

```sh
# push the two binaries the installer stages from /contents
adb push cinder-home/dist/dev/cinder-home   /contents/
adb push cinder-home/dist/dev/cinder-umount /contents/
# then install cinder_home_install.upg the usual way, and REBOOT WITH THE CABLE UNPLUGGED
```

`cinder-gpunode` is **dev-only now** and you only need it if you intend to re-test the GPU path —
which measures 4.7× slower than software, so there is no reason to unless you are experimenting.

**A cable connected at boot is itself the escape to stock.** That is rung 0 of the ladder and it
depends on nothing, so booting with it out is what actually tests the app.

### What to check on the first boot, in order
1. **It paints.** If the panel stays on the boot animation, that is the 07-26 failure mode — pull
   power and boot with the cable in.
2. **Library loads** and the counter clears (~8 s after first paint).
3. **Tap a track.** Play-by-index has never been confirmed on hardware.
4. **Swipe the artwork** left/right — the pager is new. Then swipe *below* it: that should still
   skip tracks.
5. **Shuffle and repeat** — both new this session, and both worth pressing early (see below).
6. **Settings ▸ Accent** — tap a swatch, confirm the colour it draws is the one you touched.
7. **Vol±** audibly changes output.

---

## Step 4 — Soak it

Nothing has ever run for more than a few minutes. Leave it playing for a few hours and check:
memory growth, log growth within one boot, the art cache's first build across the library, and
**boot time and battery against stock** — goal #1's entire claim, still unmeasured.

---

## Step 5 — Flash `dist/stable` for daily use

Lean, no adb, and no setuid GPU helper.

---

## The specific things this build is asking you to settle

Each is a single observation, and each has a written fallback if it fails.

| Assumption | How it shows up | If wrong |
|---|---|---|
| `SetPassband` was the missing piece | `--analyzer` prints frames | visualiser pages stay empty; second cause to find |
| `OneTrackMode::On == 1` | repeat-one actually repeats the track | flip the enum value; one line |
| Setting repeat **live** on an in-use sequence is safe | toggling repeat mid-track does not glitch or stop playback | drop the live call, let the sticky flag apply from the next track; one line |
| `media_origin_t::Begin == 0` | drag-to-seek lands where you dropped it | try the other origin values |
| `duration_raw` is milliseconds | the progress bar's scale is right | the `1ccb7bc` diagnostic prints the answer on the first boot |
| The idle screen-off wakes | blank it, wake by touch **and** by Power | a failed wake looks exactly like a dead device — Power is the escape |
| The brightness node write is right | Settings ▸ Brightness moves the panel and survives a reboot | `cinder_backlight.conf` `day=` pins it (and, since 07-28, actually does) |

---

## If it goes wrong

The escape ladder, weakest dependency first — each rung needs strictly less than the one above:

0. **Cable in at boot** → stock. Depends on nothing.
1. **Settings ▸ Boot to stock** → one-shot, no cable needed at all. Two taps.
2. **`/contents/cinderhome_off`** over USB-MSC from a PC.
3. **Bad-boot counter** — four failed boots auto-revert.
4. **wbrt restore** — the backstop.

**A reboot loop is most likely a Rust panic.** As of 07-28 the log says which screen it happened on:
grep `cinderhome.log` (and `cinderhome.log.1`, which holds the *previous* boot — the one that
crashed) for `PANIC`.

```sh
adb shell 'cat /contents/cinderhome.log.1' | grep -A 4 PANIC
```
