# Roadmap

What is planned for Cinder and Flint, and where help is welcome. Feature state lives in
[`../cinder-home/STATUS.md`](../cinder-home/STATUS.md), and what shipped in
[`../CHANGELOG.md`](../CHANGELOG.md). To help with anything here, open an issue saying which item
you are taking, or report a result there.

## Built, waiting for a test on a player

Each has a row in [`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md) §41:

- **Low-latency USB DAC**, an installer option that fetches
  [cinder-llusbdac](https://github.com/superwilso/cinder-llusbdac) (zhangboyang's llusbdac, built
  for the NW-A50 kernel), and a Low latency switch on the USB-DAC screen.
- **Bluetooth that sleeps and wakes:** the radio switches off after ten idle minutes and back on
  when the player is used (screen on or any button).
- **From Wampy:** a list of Winamp skins and cassette tapes to pick from, swapping ⏮ and ⏭,
  dragging a long Winamp title, and your own font from the drive.

## Being built next

1. **USB after a resume.** Suspend to RAM is now the default; USB has to come back reliably after a
   resume (DEVICE_CHECKLIST 33.2). The port reset Sony's own firmware does is in
   `cinder-msc usb-resume` and needs a run on a player.

Later: the rest of the redesign ([`PLAN_redesign_2026-09.md`](PLAN_redesign_2026-09.md) Part F:
the Library view bar, Search scopes, the sideways page swipe, saved views, the installer window),
CUE tracks listed in Songs, the RMT-NWS20 remote, and Flint 0.3 (setup, convert-on-transfer,
copying, the palette preview).

## Help wanted

**Testers with a Mac.** The installer has a macOS build (Apple silicon and Intel, one binary). It
is experimental until someone installs and uninstalls with it on a real Mac, on stock firmware and
on Walkman One. The first thing to find out is whether current macOS lets root take the player's
mass-storage interface. Steps are in [`../install.md`](../install.md).

**Device tests.** If you have an NW-A50-series player, the rows in
[`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md) are what turns host-tested features into verified
ones. Read its safety rules first. Grouped by what each needs:

| Session | Rows | Setup |
|---|---|---|
| New since 0.3.17 | §37 audiobooks; Sound ▸ Presets; the DAC EQ at the jack; 38.1 the queue on Bluetooth; 38.7 NC; AAC and SBC XQ from the installer; the receiver's codec choice with an Android phone (LDAC) | NC earphones for 38.7 |
| Standalone features | §39: ATRAC, FM recording, album sound, the volume curve, CUE sheets, the Cassette, Clock and Winamp styles, the guided installer | a `.wsz` skin, a CUE album |
| Daily use | 19.x–30.x, 11.7, 11.12, 13.4, 13.7, 13.8, 15.1–15.5 | none |
| Bluetooth | 2A.1–2A.5, 2C.1–2C.3, 5.2, 5.3, 21.10, 26.x, 28.2, 28.6, 31.5, 31.6, 32.3, 36.1, 36.2, 38.4 | headphones |
| Sony's own firmware | the EQ chain with `preload` on (a +10 dB band must be audible over Bluetooth) | a player on stock firmware |
| Power | 5.5 a discharge number; §33, §34, 36.3–36.5 | `tools/battery_track.sh` |
| Walkman One | 18.2–18.6, and installing a `.UPG` on a Walkman One player | a Walkman One player |

**Research.** Each of these is open and needs no code to start:

- **Buttons on Apple (CTIA) headphones.** Wampy found no difference in `regmon`. A capture of what
  the jack sees on a press would settle whether it can work.
- **What each sound effect costs on its own:** DSEE HX, the normalizer, Vinyl, Clear Bass
  (`tools/ssfw_cost.sh`).
- **Two 10 Hz pollers in Sony's services** (KeyService's `select()`, DisplayService's LED thread):
  about 20 of the 36 wakeups a second at idle. Measure off the cable before patching either.
- **Noise cancelling:** the ambient filter Walkman One removed, and whether the DAC EQ and the
  linear amp interact with NC.
- **DSEE AI**, and the SensMe channel id → name table.

**Other models** (ZX300, WM1A/Z, A40, A30): start from antiheroriot's ZX300 branch as a pull
request.

**Translations:** [`LOCALIZATION.md`](LOCALIZATION.md) has the workflow. Every translation has to
draw from the bundled fonts; a test checks it.
