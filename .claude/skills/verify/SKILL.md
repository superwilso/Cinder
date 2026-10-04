---
name: verify
description: Build and drive the Cinder UI to observe a change actually running. Use when verifying a diff that touches player/cinder-ui, player/cinder-ffi, or cinder-home/src.
---

# Verifying a Cinder change

The shipped target is an ARM device (NW-A55). You almost certainly cannot build or run that here
— the cross-build needs a glibc-2.23 xenial sysroot and libc++ **3.9.0** headers that are not in a
default environment (`cinder-home/build.sh` checks for both and exits). Don't start there.

## The surface that IS reachable

`cinder-sim --bin device` drives the **real on-device navigator** (`cinder_ui::nav::App`) in a
480×800 window — same state machine, same screens, same renderer the panel runs. Mouse is wired to
the touchscreen through the same classifier `cinder-home/src/main.cpp` uses on real evdev frames
(same thresholds), so a gesture here takes the path it takes on hardware. Actions the shell would
hand to PlayerService are printed to stdout as `action: <Action>`.

`cinder-host` is the other surface: it renders every screen to `player/out/*.png` in one shot.
Good for checking layout at a glance; it does NOT exercise input.

## Headless recipe (no display attached)

```bash
apt-get install -y -qq xdotool x11-apps        # xdotool + xwd; usually not preinstalled
Xvfb :99 -screen 0 1100x1700x24 & sleep 2
cd player && cargo build --release -p cinder-sim --bin device
DISPLAY=:99 ./target/release/device --unlocked > /tmp/sim.log 2>&1 &
sleep 3
DISPLAY=:99 xdotool search --name Cinder | tail -1     # window id
DISPLAY=:99 xdotool getwindowgeometry <id>             # gives the +X+Y origin
```

Gotchas that cost time — all of them:

- **`--unlocked` is required for scripted runs.** The sim boots locked and only the Hold switch
  (key `L`) unlocks; keystrokes don't reach minifb under a WM-less Xvfb (see below), so without the
  flag you are stuck on the lock screen forever.
- **Keyboard input does not work headless.** Neither `xdotool key --window` (XSendEvent) nor XTEST
  with PointerRoot focus reaches minifb. Mouse works fine — drive everything by touch.
- **The window is `Scale::X2`.** UI coords → screen: `sx = originX + ui_x*2`. `get_mouse_pos`
  returns buffer (UI) coords, so the app side needs no conversion.
- **A tap must HOLD ~150 ms.** The sim samples the button once per 60 fps frame; `xdotool click`
  is instantaneous and falls between polls, so nothing happens and it looks like a dead target.
- **`xwd` header**: `bits_per_pixel` is word **11** (byte offset 44), not 28. Reading the wrong
  word gives `bpp=0`, a stride of 0, and a uniformly black PNG that looks like a rendering failure.
  Width/height are words 4–5 (offset 16), `bytes_per_line` word 12 (48), `ncolors` word 19 (76).
  Pixels start at `header_size + ncolors*12`, laid out BGRX.

## No X server at all (no Xvfb, no xdotool, no sudo)

`device --script FILE` runs the same navigator and the same touch classifier with **no window**:
gestures come from a file and frames go to disk. One command per line —

```
tap X Y                  press, hold a frame, release
drag X0 Y0 X1 Y1         press, move in 16 steps, release (scrolls, or drags a slider it lands on)
route bt 0|1             what the shell reports on a Bluetooth link edge
route usb 0|1            …and for USB-DAC mode
wait N                   N ticks (lets a toast run out before a shot)
shot NAME                writes NAME.ppm (binary P6, 480x800) in the current directory
```

```bash
cd player && cargo build --release -p cinder-sim --bin device
./target/release/device --unlocked --script flow.txt     # prints `action: …` and `route: …` lines
```

Coordinates are UI coordinates (no ×2). A `.ppm` is not viewable by most image tools: convert it
(any PNG encoder; twelve lines of Python with `zlib` and `struct` do it). `route` prints the output,
the live profile and whether the shell would be told to re-apply the sound chain.

## Flows worth driving

| Area | Gesture |
|---|---|
| Seek / rewind | drag the Now Playing rail (UI y≈612); knob swells mid-drag, release prints `Seek(permille)` |
| Shelf | status-bar bookmark at (392, 16); slots at y 640 / 686 / 732, `×` column x ≥ 412 |
| Settings scroll | Menu row 9, then vertical drags — the ABOUT rows are only reachable scrolled |
| UI scale | Settings slider row, track x 176..372 — drag it and the whole UI rescales live |
| Library tabs | tap strip y≈101; label positions come from `library::tab_layout()`, not fixed thirds |
| Sound profiles | Menu ▸ Sound (row y≈415). Header A/B at (378, 67) / (434, 67). The list scrolls: `drag 240 700 240 300`. Profile row is the first row (y≈180 unscrolled) → Sound profiles: outputs at y≈197 / 261 / 325, Copy at y≈423. Then `route bt 1` and watch the letter change |
| Swipe-to-queue | rightward drag on a Songs row starting x > 38 (x ≤ 38 is the Back edge swipe) |

## Reading the result

Compare captured frames rather than trusting a single one — `cmp` on two PNGs is a cheap way to
prove a gesture did (or didn't) move something. To identify which element is active, sample the
accent colour (R>180, 60<G<170, B<90) inside the element's known x/y band.
