#!/bin/sh
#
# cinder-guard.sh — a boot-time backstop that sits BELOW appmgr.
#
# NOT INSTALLED BY install_cinderhome.sh. Copy it on by hand when you are testing a build that
# might not come up; see "Installing it" at the bottom. It has only a handful of boots behind it,
# and init BLOCKS on the script that calls it, so enabling it for everyone is not warranted yet.
#
# Cinder boot guard — Walkman One (and stock; the logic is firmware-agnostic).
#
# WHY THIS EXISTS, AND WHY IT IS NOT A DUPLICATE OF THE LAUNCHER'S BAD-BOOT COUNTER.
# cinderhome-launch.sh already counts bad boots and reverts after MAXBAD. That counter can only
# advance if appmgr actually execs the launcher. On 2026-09-21 Cinder boot-looped on Walkman One
# below EVERY rung of that ladder — including the cable escape, which needs no filesystem at all.
# The only way all five rungs miss is if the launcher never runs. So the backstop has to sit
# lower than appmgr.
#
# This script is called from /system/bin/bootswitcher.sh, which init runs in `on boot` immediately
# before `setprop sys.sony.bootmode 1` — the property that triggers `class_start hagoromo` and
# therefore appmgr. Nothing about the Home app has happened yet. Dependencies: init, /db, /data.
# That is strictly less than what it rescues (Cinder + easel + appmgr), which is the rule.
#
# It must never hang: init blocks on bootswitcher.sh, so a hang here is a worse brick than the one
# it prevents. No loops, no sleeps, no waits, no network. Pure file operations.

STATE=/db/cinder-guard
COUNT=$STATE/count
LOG=$STATE/log
APPCFG=/system/vendor/sony/bin/HgrmMediaPlayerApp.appcfg
REAL=/system/vendor/unknown321/HgrmMediaPlayerApp.appcfg.real
LAUNCHER_COUNT=/data/cinder/bootcount
MAX=3

mkdir -p $STATE 2>/dev/null

_log() {
    echo "[cinder-guard] `date +'%Y-%m-%d %H:%M:%S'` $*" >> $LOG 2>/dev/null
    true
}

# Nothing to guard: Cinder is not installed (no backup means the .appcfg was never repointed).
if [ ! -s "$REAL" ]; then
    _log "cinder not installed (.appcfg.real absent) — nothing to guard"
    exit 0
fi

# Already on stock: the .appcfg does not point at Cinder's launcher. Leave it alone; this is how
# the device sits after a revert, and re-arming here would undo the user's own choice.
if ! grep -q 'cinderhome-launch.sh' "$APPCFG" 2>/dev/null; then
    _log "appcfg already on stock — standing down"
    exit 0
fi

CUR=0
if [ -f "$COUNT" ]; then
    CUR=`cat $COUNT 2>/dev/null`
fi
case "$CUR" in ''|*[!0-9]*) CUR=0 ;; esac

# Did the PREVIOUS boot end healthy? cinderhome-launch.sh increments its own counter every boot and
# cinder-home zeroes it a few seconds after its first painted frame. So reading 0 here means the
# last boot painted and proved itself. Absent means the launcher has no state yet — treated as not
# proven, which is the safe direction.
PREV_OK=0
if [ -f "$LAUNCHER_COUNT" ]; then
    LC=`cat $LAUNCHER_COUNT 2>/dev/null`
    if [ "$LC" = "0" ]; then
        PREV_OK=1
    fi
    _log "launcher bootcount=$LC (launcher DID run last boot)"
else
    # The distinguishing diagnostic: our count is advancing but the launcher left no state at all,
    # so appmgr never exec'd it. That is the 2026-09-21 failure shape, recorded in the log.
    _log "launcher bootcount ABSENT — appmgr did not exec the launcher last boot"
fi

if [ "$PREV_OK" = "1" ]; then
    echo 0 > $COUNT 2>/dev/null
    _log "previous boot proved healthy — guard count reset to 0"
    sync
    exit 0
fi

NEW=`expr $CUR + 1`
echo $NEW > $COUNT 2>/dev/null
sync
_log "unproven boot #$NEW of $MAX"

if [ "$NEW" -lt "$MAX" ]; then
    exit 0
fi

# Backstop fires. Restore the stock .appcfg so appmgr launches Sony's Qt app and the device comes
# up usable. Written to a temp file and moved into place only after it is verified non-empty: an
# empty .appcfg means appmgr can launch NO Home app at all, which is a far worse state than the
# loop this is escaping.
_log "LIMIT REACHED — reverting .appcfg to stock"
mount -o remount,rw /system 2>/dev/null
cat "$REAL" > "$APPCFG.guardtmp" 2>/dev/null
if [ -s "$APPCFG.guardtmp" ] && grep -q '^command:' "$APPCFG.guardtmp" 2>/dev/null; then
    mv "$APPCFG.guardtmp" "$APPCFG" 2>/dev/null
    echo 0 > $COUNT 2>/dev/null
    _log "reverted to stock Home app — Cinder disabled, device should boot normally"
else
    rm "$APPCFG.guardtmp" 2>/dev/null
    _log "REVERT ABORTED — backup unreadable, left Cinder appcfg in place"
fi
sync
mount -o remount,ro /system 2>/dev/null
exit 0

# ── Installing it ───────────────────────────────────────────────────────────────────────────────
#
#   adb push cinder-guard.sh /tmp/cinder-guard.sh
#   adb shell 'mount -o remount,rw /system
#     cat /system/bin/bootswitcher.sh > /system/bin/bootswitcher.sh.precinder   # keep the stock one
#     cat /tmp/cinder-guard.sh > /system/bin/cinder-guard.sh
#     chmod 755 /system/bin/cinder-guard.sh'
#
# then add this to /system/bin/bootswitcher.sh, immediately BEFORE its "# set properties" block —
# that is the last point still upstream of `setprop sys.sony.bootmode`, which is what starts class
# hagoromo and therefore appmgr:
#
#   if [ -x /system/bin/cinder-guard.sh ]; then
#       /bin/sh /system/bin/cinder-guard.sh
#   fi
#
# Removing it: restore /system/bin/bootswitcher.sh.precinder. The guard alone is inert — with no
# hook calling it, nothing runs it.
#
# ── Verifying it without waiting for a failure ──────────────────────────────────────────────────
# Simulate the install and three bad boots; it must revert on the third:
#
#   printf 'name: HgrmMediaPlayerApp\ncommand: /system/vendor/unknown321/bin/cinderhome-launch.sh\ntype: Home\nhidden: false\n' > $APPCFG
#   for i in 1 2 3; do sh /system/bin/cinder-guard.sh; grep '^command:' $APPCFG; done
#
# and the healthy path — `echo 0 > /data/cinder/bootcount` then run it; the count must reset to 0
# and the Cinder .appcfg must be left alone. Restore the real .appcfg afterwards, and mind that
# rewriting it as root under umask 077 leaves it 0600 when stock is 0755 root:shell.
