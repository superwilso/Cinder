# Mono

Sound ▸ MONO sums left and right so both ears get the whole mix: the accessibility setting every
phone has, for anyone listening with hearing in one ear or with one earbud in. Sony's firmware has
no such setting, and nothing on the player can be switched to do it, so Cinder adds it. The
research behind it is [`analysis/RE_mono_audio.md`](../analysis/RE_mono_audio.md).

## What it does

- **Where:** music on the headphone jack and over Bluetooth (both shown summing on the device,
  2026-09-16). The USB-DAC to Bluetooth bridge sums in Cinder's own code and needs nothing
  installed.
- **How:** left and right are added and halved, after all of Sony's sound effects, so it never
  clips. DSD, and anything that is not plain stereo, passes through untouched.
- **Cost:** with MONO off, one cached file check every 250 ms on the audio path.

The same library also lets soundscapes play over music (Menu ▸ Soundscapes).

## How it installs

Both jack and Bluetooth audio come out of one Sony service, `SoundServiceFw`, so the summing has to
run inside it: a small library, `libcinder_mono.so`, loaded into that service when it starts.
Cinder never edits the player's boot image, so it brings its own way in: the install option
**Mono and soundscapes over music** (`preload`). A small program takes the place of Sony's
`hagodaemon`, starts every Sony service exactly as before, and adds the library for the sound
service alone. It is installed with a boot guard and stays on trial until the player has played
five seconds of music; see `deploy/components.conf`.

With Wampy installed, Wampy's own library still loads first and Cinder's after it. Until 0.3.22 a
second option, `mono`, used Wampy's slot instead; an Update now puts Wampy's file back and ticks
`preload` on a player where `mono` was working.

## Safety

A sound service that crashes powers the player off, so the library guards itself:

- Every load counts itself in `/data/cinder/mono_shim_boots`. The first audio through it, or
  Cinder starting normally, clears the count. After two loads in a row that cleared nothing, it
  loads in safe mode: no hooks, nothing chained.
- `/data/cinder/mono_shim_off` turns its processing off without reinstalling.
- A running service has the file open, so it is only ever replaced by rename, never rewritten.

## How it got here, and why Cinder stands on its own

Mono was asked for on 2026-09-15, including "system wide, over Bluetooth as well". There was no
control, DSP call or firmware setting to switch on, so it had to be a library inside Sony's sound
service. The quickest way in was the one already on the developer's player: Wampy's installer
preloads a library into that service, so Cinder's could ride along. That shipped in 0.3.8
(2026-09-17), and mono only worked on players with Wampy.

That was the first of several places where Cinder leaned on Wampy. Its files also lived in Wampy's
install folder (`/system/vendor/unknown321`). Wampy and Cinder work in different ways: Wampy runs
alongside Sony's music app and draws over it, while Cinder replaces that app. So Wampy's interface
can't run inside Cinder, and anything Cinder needed from it had to be borrowed at a lower level.

I wanted Cinder to stand on its own: one install that does everything I use, with mono, soundscapes,
skins and the rest working the same whether or not anything else is on the player. Mono came first.
The `preload` option (0.3.15, 2026-10-05) gave it its own way into the sound service. On
2026-10-24 Cinder moved into its own folder, `/system/vendor/cinder`. The other features people
know from Wampy, like Winamp skins or cassette tapes, could only be had inside Cinder by building
them into Cinder, so they are installer options you can tick or leave off. None of Wampy's code is in
Cinder. Wampy's write-ups on how the NW-A50 works were a big help in understanding the hardware, and
they are credited where they are used. Where it is easy, Cinder reads the same files (a Winamp skin,
a tape folder), so what people have already made works in both.

I reached out to Wampy's author for advice a while back and didn't hear back, which is fair enough,
so I didn't ask again before building these. If Wampy suits you better, use it: it supports far more
players than Cinder does.
