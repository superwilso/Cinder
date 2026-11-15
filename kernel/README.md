# kernel/

The NW-A50 series kernel, rebuilt from Sony's own GPL source, with fixes for bugs in Sony's and
MediaTek's drivers. GPL-2.0 (see `LICENSE`), like the source it patches.

Nothing here is installed by Cinder yet. A rebuilt kernel is tested by booting it from RAM over
fastboot, which writes nothing to the player (see "Testing" below).

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
