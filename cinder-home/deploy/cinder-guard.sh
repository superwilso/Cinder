#!/bin/sh
#
# cinder-guard.sh — a boot-time backstop that sits BELOW appmgr.
#
# INSTALLED ONLY WITH THE `preload` COMPONENT (deploy/cinder-preload.sh), which is off by default:
# the hagodaemon wrapper stands in front of every Sony service and is not allowed on a player
# without this underneath it. init BLOCKS on the script that calls this, which is why it is not
# switched on for everyone. It can still be copied on by hand; see "Installing it" at the bottom.
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

# Two lines a boot and nothing ever removed them: 21 KB after a fortnight on the first player to
# carry this. Keep the newest 16 KB once it passes 64 KB. One pass, no loop.
if [ -x /xbin/busybox ] && [ -n "`/xbin/busybox find $LOG -size +64k 2>/dev/null`" ]; then
    /xbin/busybox tail -c 16384 $LOG > $LOG.tmp 2>/dev/null && mv $LOG.tmp $LOG 2>/dev/null
fi

# ── THE hagodaemon WRAPPER IS ON TRIAL UNTIL IT HAS PLAYED AUDIO (2026-10-05) ─────────────────────
# src/cinder-hagowrap.c stands in front of every Sony service, so a wrapper that does not work is
# a player with no services — and possibly no adb to fix it with. Nobody has to arm this: a
# wrapper is on trial whenever one is installed (Sony's binary kept beside it as hagodaemon.real)
# and THIS build of it has not been confirmed. The installer runs in Sony's updater, where /db is
# not mounted, so the state cannot depend on a file the installer left.
#
# cinder-home writes the proof, in /data/cinder, and only on a player that has the wrapper:
#   hago_up     Cinder came up healthy this boot, so every Sony service started through it
#   hago_play   playback was asked for, and is removed again once audio has run
#   hago_ok     audio ran for five seconds through the sound service the wrapper preloads
# Read here on the NEXT boot, before a single service starts:
#   hago_ok                 confirmed. This wrapper's checksum is recorded and it is left alone
#                           from then on; a different build starts its own trial.
#   hago_play, no hago_ok   playback began and never proved itself: Sony's binary goes back.
#   hago_up alone           it boots, nothing has been played yet: still on trial, nothing changed.
#   none of them            the boot never reached Cinder: Sony's binary goes back.
# So the worst case is one bad boot and a held power button, and a wrapper that boots but breaks
# playback costs one more. Runs before the Cinder checks below: it does not depend on Cinder's
# own state at all, only on the three files above being absent when things went wrong.
HAGO=/system/vendor/sony/bin/hagodaemon
TRIAL=$STATE/hago_trial
CONFIRMED=$STATE/hago_ok
PROOF=/data/cinder
BBX=/xbin/busybox
# One file per call: a stale proof left behind because an earlier name was missing would confirm a
# wrapper that never played anything.
_gone() { [ -e "$1" ] && rm "$1" 2>/dev/null; true; }
_no_proof() { _gone "$PROOF/hago_up"; _gone "$PROOF/hago_play"; _gone "$PROOF/hago_ok"; }

_hago_revert() {
    if [ ! -s "$HAGO.real" ]; then
        _log "hagodaemon wrapper: no $HAGO.real to put back - nothing changed"
        return
    fi
    mount -o remount,rw /system 2>/dev/null
    cat "$HAGO.real" > "$HAGO.guardtmp" 2>/dev/null
    chmod 0755 "$HAGO.guardtmp" 2>/dev/null
    OKCOPY=0; SAME=0
    if [ -s "$HAGO.guardtmp" ]; then
        OKCOPY=1
        # Byte for byte when there is a cmp to ask; non-empty is the floor otherwise.
        if [ -x $BBX ]; then
            if $BBX cmp -s "$HAGO.real" "$HAGO.guardtmp"; then SAME=1; else OKCOPY=0; fi
        fi
    fi
    if [ "$OKCOPY" = "1" ]; then
        mv "$HAGO.guardtmp" "$HAGO" 2>/dev/null
        # Only once the copy was compared: with Sony's binary proven back in place, the kept copy
        # is what would mark this player as still having a wrapper.
        if [ "$SAME" = "1" ]; then _gone "$HAGO.real"; fi
        _log "hagodaemon wrapper: reverted - Sony's hagodaemon is back in place"
    else
        rm "$HAGO.guardtmp" 2>/dev/null
        _log "hagodaemon wrapper: REVERT ABORTED - could not copy $HAGO.real"
    fi
    sync
    mount -o remount,ro /system 2>/dev/null
}

if [ -s "$HAGO.real" ]; then
    # Which wrapper this is. No checksum to be had means it can never be confirmed, only kept on
    # trial: every boot then has to prove itself, which is the safe way to be without a tool.
    ID=
    if [ -x $BBX ]; then
        ID=`$BBX cksum "$HAGO" 2>/dev/null`
        ID=`echo $ID | $BBX cut -d' ' -f1,2 2>/dev/null`
    fi
    WAS=`cat $CONFIRMED 2>/dev/null`
    if [ -x $BBX ] && $BBX cmp -s "$HAGO" "$HAGO.real"; then
        :                                           # Sony's binary under both names: no wrapper
    elif [ -n "$ID" ] && [ "$ID" = "$WAS" ]; then
        :                                           # this build has played audio before
    elif [ "`cat $TRIAL 2>/dev/null`" != "trial $ID" ]; then
        # First boot with this wrapper. Clear old proof so only this boot's counts.
        _no_proof
        echo "trial $ID" > $TRIAL 2>/dev/null
        sync
        _log "hagodaemon wrapper: first boot of this build - on trial until it has played audio"
    elif [ -f "$PROOF/hago_ok" ]; then
        _no_proof; _gone "$TRIAL"
        if [ -n "$ID" ]; then
            echo "$ID" > $CONFIRMED 2>/dev/null
            _log "hagodaemon wrapper: confirmed - the last boot played audio through it"
        else
            _log "hagodaemon wrapper: played audio, but with no checksum it stays on trial"
        fi
        sync
    elif [ -f "$PROOF/hago_play" ]; then
        _log "hagodaemon wrapper: playback began last boot and never proved itself - putting Sony's binary back"
        _no_proof; _gone "$TRIAL"
        _hago_revert
    elif [ -f "$PROOF/hago_up" ]; then
        _gone "$PROOF/hago_up"
        _log "hagodaemon wrapper: the last boot came up, nothing played yet - still on trial"
    else
        _log "hagodaemon wrapper: the last boot never reached Cinder - putting Sony's binary back"
        _gone "$TRIAL"
        _hago_revert
    fi
fi

# ── THE HOME APP BACKSTOP IS OPT-IN (2026-10-05) ───────────────────────────────────────────────
# Everything below reverts the .appcfg to Sony's Home app after $MAX boots that did not prove
# healthy. It was written for one investigation (Cinder looping below the launcher on Walkman One)
# and it has a cost nobody should carry by default: the launcher takes the cable escape BEFORE it
# touches its counter, so a counter left at 1 by a boot that was switched off early stays at 1
# through every boot with the cable in — and three of those in a row would take Cinder off the
# player for good, with no file on /contents that brings it back. The launcher's own latch can be
# cleared with /contents/cinderhome_clear; an .appcfg this script reverted cannot.
# So: on only where someone asked for it, by creating $STATE/backstop_on.
if [ ! -f "$STATE/backstop_on" ]; then
    exit 0
fi

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
# init runs this under umask 077 (see /db/cinder-guard: 0700/0600), so the temp file is born 0600
# root:root — and appmgrservice runs as uid 100. Moved into place as-is, the revert would leave an
# .appcfg appmgr cannot READ: no Home app at all, the outcome this block exists to prevent. Stock
# is 0755; give it that back before the move.
chmod 0755 "$APPCFG.guardtmp" 2>/dev/null
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
# deploy/cinder-preload.sh installs this file and the six lines in /system/bin/bootswitcher.sh that
# call it (Sony's script kept as bootswitcher.sh.precinder), and removes both again. By hand:
#
#   adb push cinder-guard.sh /tmp/cinder-guard.sh
#   adb shell 'mount -o remount,rw /system
#     cat /tmp/cinder-guard.sh > /system/bin/cinder-guard.sh
#     chmod 755 /system/bin/cinder-guard.sh'
#
# The Home app backstop is a separate switch: `touch /db/cinder-guard/backstop_on` as root.
# The guard alone is inert — with no hook calling it, nothing runs it.
#
# ── Verifying it without a boot ─────────────────────────────────────────────────────────────────
# tools/test_guard.sh runs this file against a scratch root: the wrapper's trial in every order of
# proof, the backstop's three unproven boots, its healthy reset, and the log bound.
