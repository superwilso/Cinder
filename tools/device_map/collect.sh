#!/system/bin/sh
# collect.sh — the ON-DEVICE half of tools/device_map/device_map.py. Do not run it by hand.
#
# READ-ONLY. It writes nothing but its own output under /tmp (tmpfs, gone at the next boot) and
# changes no setting, service or mixer value. What it deliberately does NOT touch, and why:
#   * /proc/regmon, /dev/kmem, /dev/mem  — a write panics or reprograms hardware; a map needs neither
#   * /sys/kernel/debug contents          — debugfs nodes can act on read; listed by name only
#   * /sys/power/wakeup_count            — blocks while wakeup events are in flight
#   * lcd-backlight/duty                 — the node whose write panicked the kernel; not even read
#   * /data and /contents contents       — the owner's music, pairings (BT_Addr, nvram) and settings
#   * logcat                             — track titles and device names
# Identifiers it cannot avoid reading (serial number, MAC addresses) are scrubbed on the host.
BB=/xbin/busybox
[ -x "$BB" ] || BB=/system/xbin/busybox
O=/tmp/cinder_map
$BB rm -rf "$O"
for d in system storage fs init processes ipc sysfs alsa input kernel; do $BB mkdir -p "$O/$d"; done

dump() {  # dump <outfile> <path>... — each readable regular file under a "== path" header
    out="$1"; shift
    for f in "$@"; do
        [ -f "$f" ] && [ -r "$f" ] || continue
        echo "== $f"
        $BB head -c 16384 "$f" 2>&1
        echo
    done >> "$out"
}

# ── system ───────────────────────────────────────────────────────────────────────────────────
getprop > $O/system/getprop.txt 2>&1
$BB uname -a > $O/system/uname.txt 2>&1
for p in version cpuinfo meminfo cmdline filesystems devices misc interrupts iomem softirqs \
         vmstat zoneinfo buddyinfo pagetypeinfo slabinfo cgroups crypto consoles execdomains swaps \
         partitions mounts modules emmc dumchar_info; do
    [ -r /proc/$p ] && $BB cat /proc/$p > $O/system/proc_$p.txt 2>&1
done
$BB sysctl -a > $O/kernel/sysctl.txt 2>/dev/null
[ -r /proc/config.gz ] && $BB cat /proc/config.gz > $O/kernel/config.gz
$BB ls -1 /proc | $BB grep -v '^[0-9]*$' > $O/system/proc_entries.txt
[ -r /system/build.prop ] && $BB cat /system/build.prop > $O/system/build.prop
[ -r /default.prop ] && $BB cat /default.prop > $O/system/default.prop

# ── storage ──────────────────────────────────────────────────────────────────────────────────
$BB df -k > $O/storage/df.txt 2>&1
$BB ls -lAR /dev/block > $O/storage/dev_block.txt 2>&1
for b in /sys/block/*; do
    dump $O/storage/sys_block.txt $b/size $b/ro $b/removable $b/device/type $b/device/name \
        $b/device/manfid $b/device/oemid $b/device/date $b/device/fwrev $b/device/hwrev \
        $b/device/erase_size $b/device/preferred_erase_size
done

# ── filesystem (the firmware's own trees; no user data) ──────────────────────────────────────
$BB ls -lAen / > $O/fs/root.txt 2>&1
for d in /system /sbin /res /etc /lib /bin /xbin /usr /vendor /opt1 /opt2 /opt3; do
    [ -d "$d" ] && [ ! -L "$d" ] && $BB ls -lAenR "$d" > "$O/fs/ls$(echo $d | $BB tr / _).txt" 2>&1
done
$BB ls -lAR /dev > $O/fs/dev.txt 2>&1
for d in /data /var /db /cache; do   # the owner's partitions: names at the top level only
    $BB ls -lAen "$d" > "$O/fs/toplevel$(echo $d | $BB tr / _).txt" 2>&1
done
$BB ls -lAen /tmp > $O/fs/tmp.txt 2>&1

# ── init (the ramdisk as booted — on a Wampy player this includes Wampy's edits) ─────────────
for f in /init*.rc /ueventd*.rc /fstab*; do
    [ -f "$f" ] && $BB cat "$f" > "$O/init/$($BB basename $f)"
done

# ── processes: what runs, as whom, with which libraries and descriptors ───────────────────────
ps > $O/processes/ps.txt 2>&1
$BB ps -o pid,ppid,user,vsz,rss,stat,args > $O/processes/ps_busybox.txt 2>&1
for p in /proc/[0-9]*; do
    pid=${p#/proc/}
    [ -r $p/cmdline ] || continue
    cmd=$($BB tr '\000' ' ' < $p/cmdline 2>/dev/null)
    [ -n "$cmd" ] || continue          # kernel threads: listed in ps.txt already
    {
        echo "=== pid $pid: $cmd"
        $BB grep -E '^(Name|State|PPid|Uid|Gid|Groups|Threads|CapInh|CapPrm|CapEff|CapBnd|VmRSS):' $p/status 2>/dev/null
        echo "-- preload"
        $BB tr '\000' '\n' < $p/environ 2>/dev/null | $BB grep '^LD_'
        echo "-- mapped files"
        $BB awk '$6 ~ /^\// {print $6}' $p/maps 2>/dev/null | $BB sort -u
        echo "-- descriptors"
        $BB ls -l $p/fd 2>/dev/null | $BB awk '{print $(NF-2), $(NF-1), $NF}'
        echo
    } >> $O/processes/processes.txt
done

# ── ipc: every unix socket, and who holds it ─────────────────────────────────────────────────
$BB cat /proc/net/unix > $O/ipc/proc_net_unix.txt 2>&1
$BB netstat -axp > $O/ipc/netstat_unix.txt 2>&1
$BB netstat -tulnp > $O/ipc/netstat_inet.txt 2>&1

# ── sysfs: the class/bus maps, and attribute values from classes that are safe to read ────────
$BB ls -lA /sys/class/* > $O/sysfs/classes.txt 2>&1
$BB ls -lA /sys/bus/platform/drivers/* > $O/sysfs/platform_drivers.txt 2>&1
$BB ls -lA /sys/bus/platform/devices > $O/sysfs/platform_devices.txt 2>&1
dump $O/sysfs/i2c_devices.txt /sys/bus/i2c/devices/*/name
$BB ls -lA /sys/bus/i2c/devices > $O/sysfs/i2c_devices_links.txt 2>&1
dump $O/sysfs/spi_devices.txt /sys/bus/spi/devices/*/modalias
dump $O/sysfs/power_supply.txt /sys/class/power_supply/*/*
dump $O/sysfs/switch.txt /sys/class/switch/*/name /sys/class/switch/*/state
dump $O/sysfs/thermal.txt /sys/class/thermal/thermal_zone*/type /sys/class/thermal/thermal_zone*/temp \
    /sys/class/thermal/thermal_zone*/mode /sys/class/thermal/thermal_zone*/policy \
    /sys/class/thermal/thermal_zone*/trip_point_* /sys/class/thermal/cooling_device*/type \
    /sys/class/thermal/cooling_device*/max_state /sys/class/thermal/cooling_device*/cur_state
for l in /sys/class/leds/*; do
    case "$l" in *lcd-backlight*) dump $O/sysfs/leds.txt $l/max_brightness ;;
                 *) dump $O/sysfs/leds.txt $l/brightness $l/max_brightness $l/trigger ;; esac
done
dump $O/sysfs/backlight.txt /sys/class/backlight/*/brightness /sys/class/backlight/*/max_brightness \
    /sys/class/backlight/*/actual_brightness /sys/class/backlight/*/type
dump $O/sysfs/graphics.txt /sys/class/graphics/fb*/name /sys/class/graphics/fb*/modes \
    /sys/class/graphics/fb*/bits_per_pixel /sys/class/graphics/fb*/virtual_size /sys/class/graphics/fb*/stride
dump $O/sysfs/rtc.txt /sys/class/rtc/rtc*/name /sys/class/rtc/rtc*/hctosys
for a in /sys/class/android_usb/android0/*; do
    case "$a" in *iSerial*) ;; *) dump $O/sysfs/android_usb.txt "$a" ;; esac
done
dump $O/sysfs/input.txt /sys/class/input/input*/name /sys/class/input/input*/phys \
    /sys/class/input/input*/id/* /sys/class/input/input*/capabilities/* /sys/class/input/input*/properties
dump $O/sysfs/sound.txt /sys/class/sound/*/id /sys/class/sound/*/number /sys/class/sound/*/pcm_class /sys/class/sound/*/dev
dump $O/sysfs/misc_devices.txt /sys/class/misc/*/dev
dump $O/sysfs/cpu.txt /sys/devices/system/cpu/online /sys/devices/system/cpu/possible /sys/devices/system/cpu/present \
    /sys/devices/system/cpu/cpu*/cpufreq/cpuinfo_min_freq /sys/devices/system/cpu/cpu*/cpufreq/cpuinfo_max_freq \
    /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor /sys/devices/system/cpu/cpu*/cpufreq/scaling_available_governors \
    /sys/devices/system/cpu/cpu*/cpufreq/scaling_available_frequencies /sys/devices/system/cpu/cpu*/cpufreq/scaling_min_freq \
    /sys/devices/system/cpu/cpu*/cpufreq/scaling_max_freq /sys/devices/system/cpu/cpu*/cpuidle/state*/name \
    /sys/devices/system/cpu/cpu*/cpuidle/state*/desc /sys/devices/system/cpu/cpu*/cpuidle/state*/latency
dump $O/sysfs/power.txt /sys/power/state /sys/power/pm_async /sys/power/wake_lock /sys/power/wake_unlock
for m in /sys/module/*/parameters; do dump $O/sysfs/module_parameters.txt $m/*; done
$BB ls -1 /sys/kernel/debug > $O/sysfs/debugfs_entries.txt 2>&1
$BB ls -1 /proc/regmon > $O/sysfs/regmon_targets.txt 2>&1

# ── alsa ─────────────────────────────────────────────────────────────────────────────────────
$BB find /proc/asound -type f 2>/dev/null | while read -r f; do dump $O/alsa/proc_asound.txt "$f"; done
amixer -c 0 contents > $O/alsa/amixer_c0_contents.txt 2>&1
amixer -c 0 info > $O/alsa/amixer_c0_info.txt 2>&1
aplay -l > $O/alsa/aplay_l.txt 2>&1
aplay -L > $O/alsa/aplay_L.txt 2>&1

# ── input ────────────────────────────────────────────────────────────────────────────────────
getevent -p > $O/input/getevent_p.txt 2>&1 &
gp=$!; $BB sleep 3; kill $gp 2>/dev/null
dump $O/input/proc_bus_input.txt /proc/bus/input/devices /proc/bus/input/handlers

# ── kernel ───────────────────────────────────────────────────────────────────────────────────
$BB dmesg > $O/kernel/dmesg.txt 2>&1
$BB lsmod > $O/kernel/lsmod.txt 2>&1
$BB ls -1 /sys/module > $O/kernel/sys_module.txt 2>&1

cd /tmp && $BB tar -cf /tmp/cinder_map.tar cinder_map && $BB rm -rf "$O"
echo "collected: $($BB wc -c < /tmp/cinder_map.tar) bytes"
