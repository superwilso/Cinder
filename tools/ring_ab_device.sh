#!/system/xbin/busybox sh
# ring_ab_device.sh — the long_ring battery A/B (DEVICE_CHECKLIST 51.54), ON THE DEVICE, detached,
# so one unplugged session needs no adb. Arm it on the cable; from the moment the cable is out and
# music plays on the jack it runs MIN minutes on Sony's 1 s ring, then the long ring for MIN
# more, then the flag comes off again. tools/battery_track.sh records both halves (its `ring` column
# tells them apart) and `battery_track.sh report` compares them.
#
#   adb push tools/ring_ab_device.sh /data/cinder/ring_ab.sh
#   adb shell 'busybox setsid /system/xbin/busybox sh /data/cinder/ring_ab.sh 40 >/dev/null 2>&1 </dev/null & sleep 2'
#   (with tools/battery_track.sh start running; /data is noexec and setsid is a busybox applet)
#
# The switch needs the jack PCM to reopen (the mono shim reads the flag at open): pause, wait for the
# codec's 30 s standby to close it, play. A dev build reads /tmp/cinder_cmd once a second (devctl cmd).
MIN=${1:-40}
LOG=/data/cinder/ring_ab.log
cmd() { echo "$1" > /tmp/cinder_cmd; chmod 644 /tmp/cinder_cmd; }
say() { echo "$(cut -d' ' -f1 /proc/uptime) $*" >> "$LOG"; }

rm /tmp/cinder_long_ring 2>/dev/null
# Armed on the cable: the clock starts once the cable is out and the jack is playing.
say "armed: waiting for the cable out and music on the jack"
until [ "$(cat /sys/class/power_supply/usb/online 2>/dev/null)" = 0 ] \
      && grep -q RUNNING /proc/asound/card0/pcm4p/sub0/status 2>/dev/null; do sleep 10; done
say "A: Sony's ring for $MIN min"
sleep $((MIN * 60))
touch /tmp/cinder_long_ring && chmod 644 /tmp/cinder_long_ring
cmd pause
n=0
while [ $n -lt 60 ] && [ "$(head -1 /proc/asound/card0/pcm4p/sub0/status 2>/dev/null)" != closed ]; do
    sleep 2; n=$((n + 1))
done
cmd play
sleep 5
say "B: long ring for $MIN min ($(grep -m1 '^buffer_size' /proc/asound/card0/pcm4p/sub0/hw_params 2>/dev/null))"
sleep $((MIN * 60))
rm /tmp/cinder_long_ring
say "done: flag off (applies from the next stream open)"
