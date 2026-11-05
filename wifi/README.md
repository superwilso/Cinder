# wifi

Wi-Fi for the Sony NW-A50 series. The player's MT8590 has a Wi-Fi radio and ships its firmware
(`/system/etc/firmware/WIFI_RAM_CODE_8590`), but Sony's kernel was built without the driver and
without `cfg80211`, so nothing on the player can use it. This folder builds what is missing.

Part of [Cinder](../README.md): `cinder-home/build.sh` stages it, and the installer offers it as the
optional `wifi` component (**Device ▸ Wi-Fi**, off by default). Everything here is **GPL-2.0**
([LICENSE](LICENSE)), the licence of the kernel source it comes from; it is kept apart from
Cinder's own files for that reason.

## What it builds

| File | From | What it does |
|---|---|---|
| `compat.ko`, `cfg80211.ko` | backports-3.17.1 in Sony's GPL kernel tree | the wireless core the kernel lacks |
| `mtk_wmt_wifi_soc.ko` | MediaTek's CONSYS WMT glue, same tree | powers the Wi-Fi function on the combo chip |
| `wlan_mt.ko` | MediaTek's `mt_soc` wlan driver, same tree, two patches | the driver; makes `wlan0` |
| `wpa_supplicant`, `wpa_cli` | wpa_supplicant 2.10 + OpenSSL 3.0, static | joins WPA2, WPA3 (SAE) and OWE networks |
| `iw` | iw 5.19, static | reads the link (signal) |
| `wifiscan` | [`tools/wifiscan.c`](tools/wifiscan.c) | a scan with no libraries, for bring-up |
| `curl`, `cacert.pem` | curl 8.22.0 + OpenSSL 3.0, static; Mozilla's CA list as curl.se publishes it | HTTPS: ListenBrainz, the update check, internet radio |
| `shairport-sync` | Shairport Sync 5.5.2 (MIT), popt, libconfig, static; [`airplay/config.h`](airplay/config.h) in place of autotools | the AirPlay 1 receiver, audio to stdout |

`./build.sh` makes the modules. It needs Sony's GPL kernel tree for the NW-A50 (`KWORK`, the one
[cinder-llusbdac](https://github.com/superwilso/cinder-llusbdac) builds against) and checks every
module's vermagic against the player's kernel, `3.10.26 SMP preempt mod_unload ARMv7`.
`./build-userland.sh` makes the programs with an ARM musl cross compiler; every download is pinned
by SHA-256.

The patches ([`patches/`](patches)):

1. **The driver on Sony's kernel.** The kernel has no wireless extensions, so the driver's few calls
   into them go to a no-op stub; its WMT assert call gets the chip-type argument the kernel's WMT
   API takes.
2. **A fixed address.** Without its calibration file the driver makes up a new MAC on every boot. A
   `mac=` module parameter fixes it; `wifi-up.sh` passes `02:` plus five bytes of a hash of the
   eMMC's CID, so each player keeps one locally administered address.

## On the player

Everything lives in `/system/vendor/cinder/wifi`. Nothing loads at boot. Cinder's setuid helper
`cinder-wifi` ([source](../cinder-home/src/cinder-wifi.c)) is the only caller:

- `on` runs [`device/wifi-up.sh`](device/wifi-up.sh): load the four modules (once per boot), power the radio through
  `/dev/wmtWifi`, start `wpa_supplicant` with the saved network, and DHCP with busybox `udhcpc`
  ([`device/dhcp.sh`](device/dhcp.sh) applies the lease and writes `/tmp/resolv.conf`, which Sony's
  `/etc/resolv.conf` already points at).
- `off` runs [`device/wifi-down.sh`](device/wifi-down.sh): the radio off; the modules stay loaded.
- `status`, `scan`, `join` (network name and password on stdin, never in argv), `forget`, and
  `timesync`: busybox `ntpd`, then the RTC. The player's clock holds local time (Cinder has no time
  zone), so `timesync` keeps the clock's distance from UTC, rounded to 15 minutes, and corrects only
  the drift (`/data/cinder/wifi/utc_offset`; `status` reports it as `utc_offset=`).

Everything that only needs the network runs as cinder-home itself, not through the helper. The
kernel's paranoid networking lets group 3003 (`inet`) open sockets and appmgr starts cinder-home in
it, so cinder-home runs `curl` directly: ListenBrainz (`cinder_listenbrainz.txt` holds the token),
the GitHub update check, lyrics from LRCLIB (written beside the song as a `.lrc`), internet radio (`cinder_radio.txt`; MP3 decoded by
[minimp3](../cinder-home/third_party/minimp3), CC0), the phone remote (a server in cinder-ffi on
port 8080, locked by a code shown on the player), and AirPlay (`shairport-sync -o stdout`, read by
cinder-home and played like the radio).

Third-party licences: [NOTICE](NOTICE).

The saved network is `/data/cinder/wifi/wpa.conf`, mode 0600. Unticking the option and updating
removes it with everything else.

## Measured

On one NW-A55, 2026-11-04: 8 networks in a scan; a WPA2 network joined at −72 to −75 dBm with a
58.5 Mbit/s link; about 8.4 Mbit/s down; cold start to online in 9.2 s. Not yet measured: WPA3
against a WPA3 network, the battery cost, and Bluetooth playback while Wi-Fi is on.

The driver also lists a network named `NVRAM WARNING: Err = 0x10`. It is not a real network but
the driver complaining that it has no calibration file; `cinder-wifi scan` leaves it out.
