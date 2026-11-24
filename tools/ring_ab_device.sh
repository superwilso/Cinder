#!/system/xbin/busybox sh
# ring_ab_device.sh — the long_ring battery A/B (DEVICE_CHECKLIST 51.54), ON THE DEVICE, detached,
# so one unplugged session needs no adb. Arm it on the cable; from the moment the cable is out and
# music plays on the jack it runs MIN minutes on the long ring, then Sony's 1 s ring for MIN more
# (long ring first, owner 11-24); the flag is off at the end. tools/battery_track.sh records both
# halves (its `ring` column tells them apart) and `battery_track.sh report` compares them.
#
#   adb push tools/ring_ab_device.sh /data/cinder/ring_ab.sh
#   adb shell 'busybox setsid /system/xbin/busybox sh /data/cinder/ring_ab.sh 40 >/dev/null 2>&1 </dev/null & sleep 2'
#   (with tools/battery_track.sh start running; /data is noexec and setsid is a busybox applet)
#
# Each switch needs the jack PCM to reopen (the mono shim reads the flag at open): pause, wait for the
# codec's 30 s standby to close it, play. A dev build reads /tmp/cinder_cmd once a second (devctl cmd).
MIN=${1:-40}
LOG=/data/cinder/ring_ab.log
cmd() { echo "$1" > /tmp/cinder_cmd; chmod 644 /tmp/cinder_cmd; }
say() { echo "$(cut -d' ' -f1 /proc/uptime) $*" >> "$LOG"; }
ring() { grep -m1 '^buffer_size' /proc/asound/card0/pcm4p/sub0/hw_params 2>/dev/null; }
# Pause, let the codec standby close the PCM, play: the next open reads the flag.
reopen() {
    cmd pause
    n=0
    while [ $n -lt 60 ] && [ "$(head -1 /proc/asound/card0/pcm4p/sub0/status 2>/dev/null)" != closed ]; do
        sleep 2; n=$((n + 1))
    done
    cmd play
    sleep 5
}

touch /tmp/cinder_long_ring && chmod 644 /tmp/cinder_long_ring
# Armed on the cable: the clock starts once the cable is out and the jack is playing.
say "armed: waiting for the cable out and music on the jack"
until [ "$(cat /sys/class/power_supply/usb/online 2>/dev/null)" = 0 ] \
      && grep -q RUNNING /proc/asound/card0/pcm4p/sub0/status 2>/dev/null; do sleep 10; done
# Music may have started before the flag was read (a PCM opened on the cable): make sure.
ring | grep -q 44104 && reopen
say "A: long ring for $MIN min ($(ring))"
sleep $((MIN * 60))
rm /tmp/cinder_long_ring
reopen
say "B: Sony's ring for $MIN min ($(ring))"
sleep $((MIN * 60))
say "done"
