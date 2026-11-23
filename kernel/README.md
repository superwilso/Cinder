# kernel/

The NW-A50 series kernel, rebuilt from Sony's own GPL source, with fixes for bugs in Sony's and
MediaTek's drivers. GPL-2.0 (see `LICENSE`), like the source it patches.

The installer's `kernel` option (off by default, experimental) installs it: see "Installing on the
player" below. A rebuilt kernel is tried first by booting it from RAM over fastboot, which writes
nothing to the player (see "Testing" below).

## Build

```sh
./build.sh                       # out/zImage, out/czram.ko, out/modules/, out/System.map
./build.sh STOCK_BOOT.img        # also out/boot.img: that image's ramdisk with the new kernel
SONY_MODULES=DIR ./build.sh      # also check that Sony's binary modules (DIR/*.ko) still load
```

It needs Sony's `linux-kernel-3.10.26.tar.gz` (NW-A57) and GCC 4.9 for ARM, both fetched and pinned
by hash by `cinder-llusbdac/build.sh`; point `KWORK` at that checkout's `work/` folder.

Two settings come from Sony's internal project configuration, not the kernel configuration, so
`build.sh` passes them to `make`: `MTK_TB_WIFI_3G_MODE=WIFI_ONLY`, and `MTK_CM4_SUPPORT=yes`. Without
the second, the low-power playback path does not open, Sony's sound service stalls, and Sony's hang
checker restarts the player about 10 s into playback.

One deliberate difference from Sony's configuration: the kernel is compressed with LZO instead of
XZ, which needs `lzop` on the build machine (`apt install lzop`). The image is 6.2 MB instead of
3.8 MB, and the player starts it 1.4 s sooner: 1.3 s from the bootloader to the kernel's first
timer, against 2.7 s (measured with patch 0006).

How close the result is to the player's own kernel: the configuration is identical to the player's
`/proc/config.gz`; the build has 47,715 symbols to the shipped kernel's 47,719, the difference being
small functions GCC 4.9 inlines where Sony's GCC 4.8 did not; every symbol Sony's 12 binary
modules import resolves; and every message string in the shipped kernel is in this one, apart from
the ones the patches remove and two the compilers handle differently.

## Patches

| Patch | What it fixes |
|---|---|
| 0001 | Builds Sony's tree with current tools and without Sony's internal build system. Restores the partition headers the source release left out, read back from the shipped kernel. |
| 0002 | Reading or writing part of a codec table under `/proc/icx_audio_cxd3778gf_data/` blocked volume, playback start and standby until a restart. |
| 0003 | Writing the backlight's `duty` or `div` crashed and restarted the player. Removes `pwm_register`, which wrote any value to any hardware address. |
| 0004 | Three `/proc` and ioctl handlers copied caller-chosen lengths into small fixed buffers. |
| 0005 | zram swap hung the player the first time a swapped page came back (backport of the upstream fix). |
| 0006 | Logs when the kernel started, in ms since power-on (`cinder: kernel started N ms`), so boot time can be measured: subtract `bootprof.lk_t` from the command line. |
| 0007 | The touch panel's 0.6 s power-up runs beside the other drivers instead of in front of them: userspace starts 0.3 s sooner (2.57 s, was 2.88 s). The panel keeps its input number, which Sony's own app is given. |
| 0008 | Sony's USB unplug check (`khubd_poll`) looks every 100 ms only while a high-speed device is plugged into the player as a host, and once a second otherwise. That is 9 fewer wakeups a second almost all the time: measured on a player with the screen off, 87 to 79 a second during playback and 46 to 39 when paused. |
| 0009 | Two builds of the same source give the same image. The Mali GPU driver wrote the build machine's git revision, commit date and clock into its version string; with `__DATE__` and `__TIME__` pinned in `build.sh`, the release check can compare the kernel byte for byte. |
| 0010 | The voltage of the 1040, 747.5 and 598 MHz steps can be set at build time (`UNDERVOLT_MV`, below). Unset, build.sh passes Sony's 1150 mV and the image is byte for byte the same. |
| 0011 | The low-power playback ring's largest size can be set at build time (`LP_RING_MB`, below). Unset, build.sh passes Sony's 2 MB and the image is byte for byte the same. |

## czram.ko: working swap on the stock kernel

The shipped kernel's built-in zram has the bug patch 0005 fixes. `czram.ko` is the fixed driver
under another name (`/dev/block/czram0`), so it loads beside the built-in one with nothing flashed.
Tested on an NW-A55 on 2026-11-15:

```sh
insmod czram.ko num_devices=1
echo 134217728 > /sys/block/czram0/disksize
busybox mkswap /dev/block/czram0 && busybox swapon /dev/block/czram0
```

300 MB of test data pushed 72 MB into swap; 17,811 freed slots and 2,515 pages read back with no
kernel error, and every word of the test pattern checked correct. Compression was 2.5:1.

## Boot images

`bootimg.py` unpacks and repacks the `bootimg` (and `recovery`) partition. Repacking the player's
own images gives back the same bytes. There is no signature on the boot image: a changed ramdisk
already boots.

`logo.py` lists, extracts and replaces the images in the `logo` partition (image 0 is the boot
logo; images 1–3 are the battery screens). Untouched images keep their original bytes.

## Testing

The bootloader has fastboot, and its `boot` command runs a kernel from RAM without checking a
signature, so a test writes nothing to the player and the next restart is back on the kernel in
flash:

```sh
python3 bootimg.py pack out/zImage STOCK_RAMDISK out/boot-fastboot.img fastboot
adb shell reboot bootloader
fastboot boot out/boot-fastboot.img
```

The `fastboot` form matters. On this path the bootloader copies the kernel, ramdisk and boot
parameters to the addresses written in the image header, while a normal boot rewrites Sony's
header addresses (0x10008000, which is the register space, not memory). It also passes only the
header's command line plus its own common tail. The `fastboot` form writes memory addresses
(kernel 0x80008000, ramdisk 0x84000000, parameters 0x80000100, all below the download buffer at
0x85000000) and the base command line a normal boot builds. Never flash an image made this way.

Tested 2026-11-15 on an NW-A55: Sony's own kernel and the patched one both boot to Cinder this
way in about 30 s, with all of Sony's modules loaded. On the patched kernel, partial reads of the
codec tables no longer block it, writing the backlight's `duty` no longer crashes, and swap on
the built-in zram0 read back 34,036 pages with no kernel error. With `MTK_CM4_SUPPORT=yes` set, music played
for 15 minutes through the low-power playback path with no stall and no kernel error, and the
kernel printed no error a normal boot does not also print.

### An undervolt test kernel

```sh
UNDERVOLT_MV=1090 ./build.sh     # out/test-uv1090/zImage; nothing in out/ itself changes
```

The CPU runs 1040, 747.5 and 598 MHz at 1150 mV. MediaTek's source has a validated low-voltage
corner (`HQA_LV_1_09V`) that runs the same three steps at 1090 mV, and a 1050 mV step it enables
only with other power chips. `UNDERVOLT_MV` accepts 1050–1150. Such a kernel is only ever booted
from RAM as above, never flashed or shipped: chips differ, so a voltage that holds on one player
proves nothing about another.

### A larger low-power playback ring

```sh
LP_RING_MB=8 ./build.sh          # out/test-ring8/zImage (2..16; combine with UNDERVOLT_MV)
```

Music on the headphone jack plays through `hw:0,4`, whose DRAM ring the CM4 coprocessor drains
while the main cores sleep; they wake when it is nearly empty. Sony's kernel caps the ring at 2 MB
(about 12 s at 16-bit 44.1 kHz). A larger cap lets a player that asks for more (Cinder's `long_ring`
switch) sleep longer between refills; nothing that asks for less is affected, but the larger block
is reserved at boot. See `analysis/RE_lowest_power_wired.md`.

## Installing on the player

**With the installer:** tick `kernel` (Device). In Sony's updater, `deploy/cinder-kernel.sh`:

1. saves the partition's image to the drive as `cinder_boot_original.img`, reads it back, and
   writes nothing without it (an existing saved copy is never replaced);
2. puts the new image together on the player with `cinder-bootimg` (`src/cinder-bootimg.c`): the
   partition's own header and ramdisk, so stock and Walkman One keep theirs, this kernel, and
   `cinder` in the header's name field so a later run knows whose kernel is in place;
3. writes it to `bootimg` and reads it back; if it differs, the saved original goes straight back.

Unticking `kernel` and updating, or uninstalling, writes the saved original back the same way.
Host test: `cinder-home/tools/test_kernel.sh`. A player that will not start after this: restore
`bootimg` with cinder-rescue, or boot Sony's kernel from RAM as in "Testing" and write the saved
original back.

**By hand** (how the first player got it, 2026-11-16). The bootloader's `fastboot flash boot` is refused ("Security deny"), so the image is written from
the player itself. The `boot` partition is `mmcblk0p8` (`/proc/dumchar_info` lists it as
`bootimg`); the bootloader is p7 and is never written. Keep a copy of p8 first.

```sh
adb shell 'busybox dd if=/dev/block/mmcblk0p8 of=/data/local/tmp/p8.bin'   # then adb pull it and keep it
adb push out/boot.img /data/local/tmp/cinder_boot.img                       # out/boot.img, not the fastboot form
adb shell 'busybox dd if=/data/local/tmp/cinder_boot.img of=/dev/block/mmcblk0p8 bs=4096 conv=fsync; sync'
adb shell reboot
```

Check the write before restarting: read back exactly the image's length (`bs=512`, its size / 512
blocks) and compare checksums. A short read does not match, which is not a failed write.

To go back to Sony's kernel, write the saved p8 the same way. If the new kernel does not start,
run Sony's kernel from RAM (Testing, above, with Sony's own zImage) and write the saved p8 from
there. Uninstalling Cinder does not change the kernel.

Done 2026-11-16 on an NW-A55 (patches 0001–0008, LZO): it started from flash with all of Sony's
services, Cinder, touch and music. The kernel starts 3.4 s after power-on.
