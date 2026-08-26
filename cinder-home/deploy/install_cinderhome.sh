#!/bin/sh
# install_cinderhome.sh — exec_file payload; runs ONCE in the NWZ UPDATER as root
# (exec_file.sh already cleared the fup flag, so this is brick-safe). It installs the
# cinder-home easel Home app and makes appmgr launch it INSTEAD of the stock Qt
# HgrmMediaPlayerApp — the true Option-B replacement (no SIGSTOP overlay, no blit war).
#
# MECHANISM (least-invasive): the 9 MB Qt binary is left 100% untouched. We only swap the
# 78-byte HgrmMediaPlayerApp.appcfg so appmgr's `command:` points at a tiny launcher, which
# execs cinder-home. cinder-home registers with appmgr under the name "HgrmMediaPlayerApp"
# and completes the Foreground handshake via easel::ApplicationBase::run() — so appmgr is
# satisfied and does NOT reboot (see analysis/F_appmgr_home/RE_findings.md).
#
# UPDATER TOOLING (hardened 2026-07-01 after a false-abort on the first flash): the NWZ
#   updater's AMBIENT shell utilities are unreliable — a bare `wc -c` returned 0 (→ the
#   size-sanity check false-aborted a GOOD copy) and `rm -f` choked on its own flag. Wampy
#   avoids this by invoking `/xbin/busybox <cmd>` for every op; we now do the same, so the
#   brick-critical .appcfg write no longer rides on those flaky tools. A runtime fallback
#   (/xbin/busybox → /system/xbin/busybox → bare) covers layout variance. mount/umount/sync
#   stay ambient — exec_file.sh already proved the updater's own mount works (it remounts
#   /contents rw and our log lands there).
#
# SAFETY — BAD-BOOT COUNTER + AUTO-REVERT (hardened 2026-06-26 after a hung launch needed wbrt):
#   The launcher increments /data/cinder/bootcount each boot; the counter is reset to 0
#   ONLY by cinder-home once it proves HEALTHY (painted + survived 8 s). So a crash OR
#   HANG never resets it -> it accumulates and after MAXBAD=4 boots the launcher execs the stock
#   Qt app -> stock UI returns on its own, NO PC/wbrt. (The old blind 60s-timer reset is removed:
#   a hung process "survived" it -> the counter never accumulated -> soft-brick.) Manual escapes:
#   a USB cable connected at boot -> stock (needs no filesystem at all), or /contents/cinderhome_off
#   over USB-MSC. State lives on /data (ext4) because /contents is vfat AND is unmounted for
#   USB-MSC -> when it went missing the counter stopped advancing and the device could not revert.
#   All writes here are ATOMIC (temp+verify+mv) + a FINAL SANITY GATE reverts to stock if any
#   piece is wrong, so a partial install can't soft-brick. Full revert: cinder_home_uninstall.upg.
# The original Qt binary is never modified; only the .appcfg (backed up to .appcfg.real).
LOG=/contents/cinder_home_install.log
exec >>"$LOG" 2>&1
echo "================================================================"
echo "== cinder-home installer  $(date 2>/dev/null)"

# ── busybox anchor ─────────────────────────────────────────────────────────────────────────
# Route every file op through the updater's known-good busybox (see UPDATER TOOLING above).
BB=/xbin/busybox
[ -x "$BB" ] || BB=/system/xbin/busybox
[ -x "$BB" ] || BB=busybox      # last resort: whatever is on PATH (may be the flaky one)
echo "busybox: $BB ($("$BB" 2>&1 | head -1 2>/dev/null))"

VENDOR=/system/vendor/unknown321
BIN=$VENDOR/bin
SRC=/contents/cinder-home
SRC_UMOUNT=/contents/cinder-umount
SRC_GPUNODE=/contents/cinder-gpunode
SRC_POWER=/contents/cinder-power
SRC_MSC=/contents/cinder-msc
SRC_CLOCK=/contents/cinder-clock
SRC_FM=/contents/cinder-fm
SRC_VOLTABLE=/contents/cinder-voltable
SRC_BATTERY=/contents/cinder-battery
SRC_SIGNATURE=/contents/cinder-signature.sh
SONYBIN=/system/vendor/sony/bin
APPCFG=$SONYBIN/HgrmMediaPlayerApp.appcfg
LAUNCH=$BIN/cinderhome-launch.sh

# ── optional components ────────────────────────────────────────────────────────────────────
# Which optional parts to install, chosen on the host by tools/configure.sh and staged next to
# the binaries. See deploy/components.conf for the catalogue and what each one costs.
#
# This file is READ, never SOURCED. /contents is vfat and is writable by anyone who plugs the
# player in over USB-MSC, so `. $COMPONENTS` would execute whatever a user (or anything that had
# access to the device) left in it, as root, in the updater. We grep out one KEY=VALUE at a time
# and whitelist-validate every value before it is used.
#
# A MISSING file is not an error — it means "defaults", which reproduces the behaviour this
# installer had before components existed, so older staging layouts keep working unchanged.
COMPONENTS=/contents/cinder_components.conf

comp_raw() {  # comp_raw <VARNAME> — echo the last assignment, or empty
    [ -f "$COMPONENTS" ] || return 0
    "$BB" grep -E "^$1=[A-Za-z0-9_]*[[:space:]]*$" "$COMPONENTS" 2>/dev/null \
        | "$BB" tail -1 | "$BB" cut -d= -f2 | "$BB" tr -cd 'A-Za-z0-9_'
}
comp_bool() {  # comp_bool <VARNAME> <default> — only ever echoes 0 or 1
    v="$(comp_raw "$1")"
    case "$v" in 0|1) echo "$v" ;; *) echo "$2" ;; esac
}
comp_voltable() {  # comp_voltable — only ever echoes a known table keyword
    v="$(comp_raw CINDER_VOLTABLE)"
    case "$v" in stock|wm1a|w1) echo "$v" ;; *) echo stock ;; esac
}
comp_sig() {   # comp_sig — only ever echoes a known variant name
    v="$(comp_raw CINDER_SIGNATURE)"
    case "$v" in stock|pv1|pv2|clock|hw1|hw2) echo "$v" ;; *) echo stock ;; esac
}

WANT_POWER="$(comp_bool CINDER_POWER 1)"
WANT_MSC="$(comp_bool CINDER_MSC 1)"
WANT_CLOCK="$(comp_bool CINDER_CLOCK 1)"
WANT_UMOUNT="$(comp_bool CINDER_UMOUNT 1)"
WANT_GPUNODE="$(comp_bool CINDER_GPUNODE 0)"
WANT_FM="$(comp_bool CINDER_FM 1)"
WANT_BATTERY="$(comp_bool CINDER_BATTERY 1)"
WANT_VOLTABLE="$(comp_voltable)"
WANT_SIGNATURE="$(comp_sig)"

if [ -f "$COMPONENTS" ]; then
    echo "components: from $COMPONENTS"
else
    echo "components: no $COMPONENTS staged — using defaults"
fi
echo "components: power=$WANT_POWER msc=$WANT_MSC clock=$WANT_CLOCK umount=$WANT_UMOUNT gpunode=$WANT_GPUNODE fm=$WANT_FM voltable=$WANT_VOLTABLE battery=$WANT_BATTERY signature=$WANT_SIGNATURE"

mount -t ext4 -o rw /emmc@android /system 2>/dev/null
mount -o remount,rw /emmc@android /system 2>/dev/null

# the staged binary must be present (user copies 'cinder-home' to the storage root first)
if [ ! -f "$SRC" ]; then
    echo "ERROR: $SRC not found — copy the 'cinder-home' binary to the storage root"
    echo "       (tools/flash.sh --push cinder-home/cinder-home) before flashing. ABORT (no changes)."
    sync; umount /system 2>/dev/null; exit 0
fi
if [ ! -f "$APPCFG" ]; then
    echo "ERROR: $APPCFG not found — wrong device/layout. ABORT (no changes)."
    sync; umount /system 2>/dev/null; exit 0
fi

# ensure the install dir exists (Wampy provides it; create if missing)
[ -d "$BIN" ] || "$BB" mkdir -p "$BIN"

# 1) install the cinder-home binary ATOMICALLY (write temp -> verify -> mv). A truncated binary
#    must never become the live one. (busybox cp is flaky -> use cat>.)
"$BB" cat "$SRC" > "$BIN/cinder-home.tmp" 2>/dev/null
if [ ! -s "$BIN/cinder-home.tmp" ]; then
    echo "ERROR: failed to stage $BIN/cinder-home (copy failed/zero bytes). ABORT (no .appcfg change)."
    "$BB" rm -f "$BIN/cinder-home.tmp" 2>/dev/null; sync; umount /system 2>/dev/null; exit 0
fi
# size sanity: the binary is ~2.6 MB. Measure with busybox (the ambient wc returned 0 and
# false-aborted the first flash). Compare against the SOURCE size too. Only abort on a
# *measured* implausibly-small file; if size is genuinely unmeasurable, the -s check above
# already proved the file is non-empty, so proceed and let the bad-boot counter be the net.
sz=$("$BB" wc -c < "$BIN/cinder-home.tmp" 2>/dev/null | "$BB" tr -cd '0-9')
srcsz=$("$BB" wc -c < "$SRC" 2>/dev/null | "$BB" tr -cd '0-9')
case "$sz"    in ''|*[!0-9]*) sz=-1;;    esac
case "$srcsz" in ''|*[!0-9]*) srcsz=-1;; esac
echo "staged size: $sz bytes (source $srcsz bytes)"
if [ "$sz" -ge 0 ] && [ "$sz" -lt 1000000 ]; then
    echo "ERROR: staged binary only $sz bytes (expected ~2.6MB) — partial copy. ABORT (no .appcfg change)."
    "$BB" rm -f "$BIN/cinder-home.tmp" 2>/dev/null; sync; umount /system 2>/dev/null; exit 0
fi
if [ "$sz" -ge 0 ] && [ "$srcsz" -ge 0 ] && [ "$sz" != "$srcsz" ]; then
    echo "ERROR: staged $sz != source $srcsz bytes — truncated copy. ABORT (no .appcfg change)."
    "$BB" rm -f "$BIN/cinder-home.tmp" 2>/dev/null; sync; umount /system 2>/dev/null; exit 0
fi
[ "$sz" -lt 0 ] && echo "WARN: size unmeasurable even via busybox; file is non-empty (-s passed) — proceeding; bad-boot counter is the net."
"$BB" chmod 0755 "$BIN/cinder-home.tmp"
"$BB" mv -f "$BIN/cinder-home.tmp" "$BIN/cinder-home"
echo "installed binary: $BIN/cinder-home ($sz bytes)"

# 1b) install the setuid-root umount helper (SETUID is what lets capless uid-100 cinder-home
#     unmount /contents for USB-MSC). Atomic temp->verify->chown root->chmod 4755->mv. Non-fatal:
#     if the helper is missing/bad, cinder falls back to the (racy) stock-trigger MSC path.
if [ "$WANT_UMOUNT" != 1 ]; then
    echo "components: umount NOT selected — skipping cinder-umount."
    "$BB" rm -f "$BIN/cinder-umount" 2>/dev/null
elif [ -s "$SRC_UMOUNT" ]; then
    "$BB" cat "$SRC_UMOUNT" > "$BIN/cinder-umount.tmp" 2>/dev/null
    if [ -s "$BIN/cinder-umount.tmp" ]; then
        "$BB" chown 0:0 "$BIN/cinder-umount.tmp" 2>/dev/null
        "$BB" chmod 4755 "$BIN/cinder-umount.tmp"
        "$BB" mv -f "$BIN/cinder-umount.tmp" "$BIN/cinder-umount"
        echo "installed setuid helper: $BIN/cinder-umount ($("$BB" wc -c < "$BIN/cinder-umount" 2>/dev/null | "$BB" tr -cd '0-9') bytes, mode $("$BB" stat -c %a "$BIN/cinder-umount" 2>/dev/null))"
    else
        echo "WARN: cinder-umount stage empty — MSC will use the fallback path."
        "$BB" rm -f "$BIN/cinder-umount.tmp" 2>/dev/null
    fi
else
    echo "WARN: $SRC_UMOUNT not staged (tools/flash.sh --push dist/<ch>/cinder-umount) — MSC fallback path."
fi

# 1c) install the setuid-root GPU-node helper (chmod 0666 on /dev/ion, /dev/mtkfb_vsync,
#     /dev/mtk_disp, /dev/sw_sync — the four root-only nodes uid-100 EGL needs). Same atomic
#     temp->chown root->chmod 4755->mv treatment. Non-fatal: without it the GPU present path
#     refuses to start and cinder renders via the software framebuffer exactly as before.
if [ "$WANT_GPUNODE" != 1 ]; then
    echo "components: gpunode NOT selected — skipping cinder-gpunode (software render)."
    "$BB" rm -f "$BIN/cinder-gpunode" 2>/dev/null
elif [ -s "$SRC_GPUNODE" ]; then
    "$BB" cat "$SRC_GPUNODE" > "$BIN/cinder-gpunode.tmp" 2>/dev/null
    if [ -s "$BIN/cinder-gpunode.tmp" ]; then
        "$BB" chown 0:0 "$BIN/cinder-gpunode.tmp" 2>/dev/null
        "$BB" chmod 4755 "$BIN/cinder-gpunode.tmp"
        "$BB" mv -f "$BIN/cinder-gpunode.tmp" "$BIN/cinder-gpunode"
        echo "installed setuid helper: $BIN/cinder-gpunode ($("$BB" wc -c < "$BIN/cinder-gpunode" 2>/dev/null | "$BB" tr -cd '0-9') bytes, mode $("$BB" stat -c %a "$BIN/cinder-gpunode" 2>/dev/null))"
    else
        echo "WARN: cinder-gpunode stage empty — GPU path stays off (software render)."
        "$BB" rm -f "$BIN/cinder-gpunode.tmp" 2>/dev/null
    fi
else
    echo "WARN: $SRC_GPUNODE not staged (tools/flash.sh --push dist/<ch>/cinder-gpunode) — GPU path stays off."
fi

# 1d) install the setuid-root power helper (reboot(2) for Settings ▸ Power off / Restart, and for
#     the Power-button hold menu). Sony's PowerMgrServiceClient cannot serve those while Cinder is
#     the Home app — its shutdown barrier waits on an ACK we do not send. Same atomic
#     temp->chown root->chmod 4755->mv treatment. Non-fatal: without it Power off and Restart log
#     a clear failure and do nothing, which is what they effectively did before this existed.
if [ "$WANT_POWER" != 1 ]; then
    echo "components: power NOT selected — skipping cinder-power (no Power off / Restart)."
    "$BB" rm -f "$BIN/cinder-power" 2>/dev/null
elif [ -s "$SRC_POWER" ]; then
    "$BB" cat "$SRC_POWER" > "$BIN/cinder-power.tmp" 2>/dev/null
    if [ -s "$BIN/cinder-power.tmp" ]; then
        "$BB" chown 0:0 "$BIN/cinder-power.tmp" 2>/dev/null
        "$BB" chmod 4755 "$BIN/cinder-power.tmp"
        "$BB" mv -f "$BIN/cinder-power.tmp" "$BIN/cinder-power"
        echo "installed setuid helper: $BIN/cinder-power ($("$BB" wc -c < "$BIN/cinder-power" 2>/dev/null | "$BB" tr -cd '0-9') bytes, mode $("$BB" stat -c %a "$BIN/cinder-power" 2>/dev/null))"
    else
        echo "WARN: cinder-power stage empty — Power off / Restart will not work."
        "$BB" rm -f "$BIN/cinder-power.tmp" 2>/dev/null
    fi
else
    echo "WARN: $SRC_POWER not staged (tools/flash.sh --push dist/<ch>/cinder-power) — Power off / Restart will not work."
fi

# 1e) install the setuid-root USB-MSC helper. Both privileged steps of the handoff (binding the
#     LUN's backing block device, and switching sys.sony.config) are root-only, so without this
#     USB mass storage cannot work at all from capless cinder-home.
if [ "$WANT_MSC" != 1 ]; then
    echo "components: msc NOT selected — skipping cinder-msc (no USB mass storage)."
    "$BB" rm -f "$BIN/cinder-msc" 2>/dev/null
elif [ -s "$SRC_MSC" ]; then
    "$BB" cat "$SRC_MSC" > "$BIN/cinder-msc.tmp" 2>/dev/null
    if [ -s "$BIN/cinder-msc.tmp" ]; then
        "$BB" chown 0:0 "$BIN/cinder-msc.tmp" 2>/dev/null
        "$BB" chmod 4755 "$BIN/cinder-msc.tmp"
        "$BB" mv -f "$BIN/cinder-msc.tmp" "$BIN/cinder-msc"
        echo "installed setuid helper: $BIN/cinder-msc ($("$BB" wc -c < "$BIN/cinder-msc" 2>/dev/null | "$BB" tr -cd '0-9') bytes, mode $("$BB" stat -c %a "$BIN/cinder-msc" 2>/dev/null))"
    else
        echo "WARN: cinder-msc stage empty — USB mass storage will not work."
        "$BB" rm -f "$BIN/cinder-msc.tmp" 2>/dev/null
    fi
else
    echo "WARN: $SRC_MSC not staged (tools/flash.sh --push dist/<ch>/cinder-msc) — USB mass storage will not work."
fi

# 1f) install the setuid-root clock helper. settimeofday(2) and the RTC_SET_TIME ioctl both need
#     CAP_SYS_TIME, and cinder-home runs as uid 100 with an empty capability set. NOTHING in
#     vendor/sony/lib exposes a clock setter (a sweep of every library's demangled `virtual`
#     prototypes finds none), so the kernel is the only route. Same atomic
#     temp->chown root->chmod 4755->mv treatment. Non-fatal: without it the clock cannot be set.
if [ "$WANT_CLOCK" != 1 ]; then
    echo "components: clock NOT selected — skipping cinder-clock (clock cannot be set)."
    "$BB" rm -f "$BIN/cinder-clock" 2>/dev/null
elif [ -s "$SRC_CLOCK" ]; then
    "$BB" cat "$SRC_CLOCK" > "$BIN/cinder-clock.tmp" 2>/dev/null
    if [ -s "$BIN/cinder-clock.tmp" ]; then
        "$BB" chown 0:0 "$BIN/cinder-clock.tmp" 2>/dev/null
        "$BB" chmod 4755 "$BIN/cinder-clock.tmp"
        "$BB" mv -f "$BIN/cinder-clock.tmp" "$BIN/cinder-clock"
        echo "installed setuid helper: $BIN/cinder-clock ($("$BB" wc -c < "$BIN/cinder-clock" 2>/dev/null | "$BB" tr -cd '0-9') bytes, mode $("$BB" stat -c %a "$BIN/cinder-clock" 2>/dev/null))"
    else
        echo "WARN: cinder-clock stage empty — the clock cannot be set."
        "$BB" rm -f "$BIN/cinder-clock.tmp" 2>/dev/null
    fi
else
    echo "WARN: $SRC_CLOCK not staged (tools/flash.sh --push dist/<ch>/cinder-clock) — clock cannot be set."
fi

# 1f2) FM register helper. chmod 0666 on /proc/regmon/Si4708icx/{target,value} — the two kernel
#      files Sony's own driver publishes the FM tuner's registers through. Reaching them is what
#      gives the radio a real signal meter, a one-second band scan and the chip's hardware seek;
#      Sony's service has none of the three. Same atomic temp->chown root->chmod 4755->mv
#      treatment. Non-fatal: without it the radio still plays, scan falls back to measuring the
#      audio (about a minute) and the screen draws no meter rather than a fake one.
if [ "$WANT_FM" != 1 ]; then
    echo "components: fm NOT selected — skipping cinder-fm (no meter, slow scan)."
    "$BB" rm -f "$BIN/cinder-fm" 2>/dev/null
elif [ -s "$SRC_FM" ]; then
    "$BB" cat "$SRC_FM" > "$BIN/cinder-fm.tmp" 2>/dev/null
    if [ -s "$BIN/cinder-fm.tmp" ]; then
        "$BB" chown 0:0 "$BIN/cinder-fm.tmp" 2>/dev/null
        "$BB" chmod 4755 "$BIN/cinder-fm.tmp"
        "$BB" mv -f "$BIN/cinder-fm.tmp" "$BIN/cinder-fm"
        echo "installed setuid helper: $BIN/cinder-fm ($("$BB" wc -c < "$BIN/cinder-fm" 2>/dev/null | "$BB" tr -cd '0-9') bytes, mode $("$BB" stat -c %a "$BIN/cinder-fm" 2>/dev/null))"
    else
        echo "WARN: cinder-fm stage empty — FM meter/fast scan unavailable."
        "$BB" rm -f "$BIN/cinder-fm.tmp" 2>/dev/null
    fi
else
    echo "WARN: $SRC_FM not staged (tools/flash.sh --push dist/<ch>/cinder-fm) — FM meter/fast scan unavailable."
fi

# 1f3) wired volume curve. Installs cinder-voltable (setuid-root) and records the chosen table in
#      /contents/cinder_voltable.conf, which the LAUNCHER applies on every boot — it has to be every
#      boot, because load_sony_driver re-installs the stock table each time. Putting the choice on
#      /contents means it can be changed over USB-MSC without a reinstall.
#      Non-fatal: without it the stock curve stays, which is what the device does today.
if [ -s "$SRC_VOLTABLE" ]; then
    "$BB" cat "$SRC_VOLTABLE" > "$BIN/cinder-voltable.tmp" 2>/dev/null
    if [ -s "$BIN/cinder-voltable.tmp" ]; then
        "$BB" chown 0:0 "$BIN/cinder-voltable.tmp" 2>/dev/null
        "$BB" chmod 4755 "$BIN/cinder-voltable.tmp"
        "$BB" mv -f "$BIN/cinder-voltable.tmp" "$BIN/cinder-voltable"
        echo "installed setuid helper: $BIN/cinder-voltable (mode $("$BB" stat -c %a "$BIN/cinder-voltable" 2>/dev/null))"
    else
        echo "WARN: cinder-voltable stage empty — the stock volume curve stays."
        "$BB" rm -f "$BIN/cinder-voltable.tmp" 2>/dev/null
    fi
else
    echo "WARN: $SRC_VOLTABLE not staged — the stock volume curve stays."
fi
# Only seed the conf if the user has not already got one: their on-device choice outranks the
# installer's default on a re-flash.
if [ ! -f /contents/cinder_voltable.conf ]; then
    echo "$WANT_VOLTABLE" > /contents/cinder_voltable.conf 2>/dev/null
    echo "volume curve: $WANT_VOLTABLE (wrote /contents/cinder_voltable.conf)"
else
    echo "volume curve: keeping existing /contents/cinder_voltable.conf ($("$BB" cat /contents/cinder_voltable.conf 2>/dev/null))"
fi

# 1f4) battery/charger reader. The bq24262 charger's registers live under /proc/regmon/bq24262/,
#      root-only, and they are the ONLY source on this device for charge state, the fault code, the
#      input/charge current settings and the battery regulation voltage. sysfs has capacity, status,
#      health and voltage_now and nothing more — there is no fuel gauge here and no current sense.
#      Unlike cinder-fm this helper does NOT chmod the nodes: writing this chip reprograms a lithium
#      battery charger, so it reads them itself and prints. Non-fatal: without it the battery screen
#      shows the four sysfs facts and omits the charger detail.
if [ "$WANT_BATTERY" != 1 ]; then
    echo "components: battery NOT selected — skipping cinder-battery (no charger detail)."
    "$BB" rm -f "$BIN/cinder-battery" 2>/dev/null
elif [ -s "$SRC_BATTERY" ]; then
    "$BB" cat "$SRC_BATTERY" > "$BIN/cinder-battery.tmp" 2>/dev/null
    if [ -s "$BIN/cinder-battery.tmp" ]; then
        "$BB" chown 0:0 "$BIN/cinder-battery.tmp" 2>/dev/null
        "$BB" chmod 4755 "$BIN/cinder-battery.tmp"
        "$BB" mv -f "$BIN/cinder-battery.tmp" "$BIN/cinder-battery"
        echo "installed setuid helper: $BIN/cinder-battery (mode $("$BB" stat -c %a "$BIN/cinder-battery" 2>/dev/null))"
    else
        echo "WARN: cinder-battery stage empty — no charger detail on the battery screen."
        "$BB" rm -f "$BIN/cinder-battery.tmp" 2>/dev/null
    fi
else
    echo "WARN: $SRC_BATTERY not staged — no charger detail on the battery screen."
fi

# 1g) audio "sound signature". Installs the switcher and applies the chosen variant by patching
#     three bytes of the stock audio HAL — which ALSA PCM device the output stream opens, and the
#     CPU clock floor held during playback. That is the entirety of what Walkman One's paid sound
#     signature does; see analysis/RE_walkmanone_extract.md. Not setuid: it is a plain script run
#     as root from here, and afterwards by hand.
#     The switcher itself is installed even when the variant is `stock`, so the choice can be
#     changed later on-device without a reinstall. It verifies the library against a known md5
#     before touching it, keeps a pristine .stock backup, and rebuilds every variant from that
#     backup — so variants never stack and a revert is exact. Wholly non-fatal.
if [ -s "$SRC_SIGNATURE" ]; then
    "$BB" cat "$SRC_SIGNATURE" > "$BIN/cinder-signature.sh.tmp" 2>/dev/null
    if [ -s "$BIN/cinder-signature.sh.tmp" ]; then
        "$BB" chown 0:0 "$BIN/cinder-signature.sh.tmp" 2>/dev/null
        "$BB" chmod 755 "$BIN/cinder-signature.sh.tmp"
        "$BB" mv -f "$BIN/cinder-signature.sh.tmp" "$BIN/cinder-signature.sh"
        echo "installed: $BIN/cinder-signature.sh"
        # $WANT_SIGNATURE is whitelist-validated above — it can only ever be one of the six
        # known variant names, so it is safe to pass through to the shell here.
        sh "$BIN/cinder-signature.sh" set "$WANT_SIGNATURE" || \
            echo "WARN: signature '$WANT_SIGNATURE' not applied — audio HAL left as found."
    else
        echo "WARN: cinder-signature.sh stage empty — signature left as found."
        "$BB" rm -f "$BIN/cinder-signature.sh.tmp" 2>/dev/null
    fi
elif [ "$WANT_SIGNATURE" != stock ]; then
    echo "WARN: $SRC_SIGNATURE not staged but signature=$WANT_SIGNATURE requested — NOT applied."
fi

# 2) back up the ORIGINAL .appcfg BEFORE writing anything. If this fails we must NOT touch
#    the .appcfg (otherwise the stock launch config is lost with no .real to restore).
if [ ! -f "$APPCFG.real" ]; then
    "$BB" cat "$APPCFG" > "$APPCFG.real" && "$BB" chmod 0644 "$APPCFG.real"
    if [ ! -s "$APPCFG.real" ]; then
        echo "ERROR: failed to back up $APPCFG -> .appcfg.real. ABORT (no .appcfg change)."
        sync; umount /system 2>/dev/null; exit 0
    fi
    echo "backed up $APPCFG -> .appcfg.real"
fi

# 3) write the launcher ATOMICALLY (temp -> verify -> mv). A truncated launcher would fail to
#    exec cinder-home AND never run the counter -> no auto-revert. Quoted heredoc = verbatim.
#    (The launcher runs at NORMAL boot, where /system/bin/sh + standard tools are available.)
"$BB" cat > "$LAUNCH.tmp" <<'LAUNCH_EOF'
#!/system/bin/sh
# cinder-home launcher — appmgr execs this (via the repointed .appcfg command:). It runs
# cinder-home behind a BAD-BOOT COUNTER so a failed/HUNG launch reverts to the stock Qt app
# WITHOUT a wbrt restore. The stock Qt binary is never modified.
#
# SAFETY MODEL (rewritten 2026-06-26 after a hung launch required wbrt):
#  * The counter is incremented HERE every boot and persisted (sync). It is reset to 0 ONLY by
#    cinder-home itself, after it has proven healthy (painted + survived its risky init). So a
#    HANG — which never resets the counter — ACCUMULATES across (force-)reboots and auto-reverts
#    after MAXBAD. (The old launcher reset the counter on a blind 60 s timer, which a hung
#    process "survives" → it never accumulated → soft-brick. That bug is removed.)
#  * ESCAPES, weakest dependency first (each works when the one below it cannot):
#      1. USB cable connected at boot          -> stock. No fs, no shell, no counter. Always works.
#      2. /contents/cinderhome_off  (USB-MSC)  -> stock. Needs a mountable /contents.
#      3. /contents/cinderhome_clear (USB-MSC) -> clears the latch and retries.
#      4. bad-boot counter hits MAXBAD         -> stock, automatically. Needs a writable /data.
#      5. Settings ▸ Boot to stock (in Cinder)  -> stock ONCE, then back. Needs Cinder to be running,
#         so it is the weakest of the five — but it is the only one reachable with NO CABLE, and
#         unlike the others it is self-undoing, so it cannot strand the user on stock.
#
# WHERE THE STATE LIVES (moved off /contents 2026-07-26, after a hard brick):
#   The counter used to live on /contents. That is the WORST possible home for a safety net here:
#     - /contents is VFAT (no journal) on /emmc@contents, so an unclean unmount corrupts it;
#     - it is the ONLY partition handed to the PC for USB-MSC (init.rc: `service unmount_msc1
#       /system/bin/umount /contents`), so it is repeatedly yanked away at runtime.
#   When it failed to mount, the counter write went nowhere AND the old `>/contents/cinderhome.log`
#   redirect on the exec line failed — so sh exited without exec'ing, appmgr rebooted, and the
#   counter never advanced. Result: a logo boot-loop that could NEVER auto-revert. Required wbrt.
#   State now lives on /data (ext4, journaled, `usrdata /emmc@usrdata /data ext4 discard` per
#   /bin/mount_partition), which USB-MSC never touches. /contents keeps only the human-facing
#   escapes, so recovery over USB-MSC still works without a shell.
STATE=/data/cinder
BOOTCOUNT=$STATE/bootcount
DISABLED=$STATE/DISABLED_badboot
OFF=$STATE/off
# MSC-reachable escapes (read-only here — /contents may be absent, so every use is guarded)
MSC_OFF=/contents/cinderhome_off        # drop this file over USB-MSC -> boot stock
MSC_CLEAR=/contents/cinderhome_clear    # drop this file over USB-MSC -> clear the latch, try again
# ONE-SHOT stock boot, armed from Cinder's own Settings ▸ Boot to stock row. Deliberately NOT the
# same thing as $OFF: it is CONSUMED on the boot it fires, so the boot after that returns to Cinder.
# That matters because it is the only escape a user can reach with NO CABLE, and a persistent latch
# would then be one-way — you could leave Cinder without a cable but not come back without one.
# Written to both filesystems: /data is journaled and reliable, /contents is visible over USB-MSC.
ONCE=$STATE/once_stock
MSC_ONCE=/contents/cinderhome_once
# 4, not 2. cinder-home clears the counter ~8 s after its first painted frame, so a genuinely
# healthy boot is "proven" almost immediately — but a developer reboot inside that window still
# costs one count, and MAXBAD=2 meant a single impatient reboot could latch the device to stock
# permanently. 4 gives real margin while still auto-reverting a build that truly never paints.
MAXBAD=4
REAL=/system/vendor/sony/bin/HgrmMediaPlayerApp           # untouched stock Qt app
HOME_BIN=/system/vendor/unknown321/bin/cinder-home
export LD_LIBRARY_PATH="/system/vendor/sony/lib:/system/vendor/unknown321/lib:/system/lib:/usr/lib:/lib:$LD_LIBRARY_PATH"

run_stock() { exec "$REAL" "$@"; }

# ── USB CABLE ESCAPE (re-added 2026-07-26) ────────────────────────────────────────────────────
# Plug the cable in and boot -> stock. This is the ONLY escape that needs no filesystem, no shell
# and no working counter, so it is checked FIRST, before anything that could itself fail. It is
# what would have recovered the 2026-07-26 brick without wbrt: /contents was gone, so every
# file-based escape was unreachable, but the cable was always available.
#
# It was removed 2026-07-25 because it means "charge overnight -> wake up on stock". That
# tradeoff is now resolved the other way round — recovery beats convenience — with an explicit
# opt-out for cable-heavy dev sessions (dev has adb, so it loses nothing by opting out).
#   /data/cinder/cable_escape_off       persistent, invisible to USB-MSC
#   /contents/cinderhome_cable_off      settable over USB-MSC
# A missing file leaves the escape ON, so a broken fs fails toward recovery, never away from it.
usb_connected() {
    for p in /sys/class/android_usb/android0/state \
             /sys/class/power_supply/usb/online \
             /sys/class/power_supply/usb/present; do
        [ -r "$p" ] || continue
        case "$(cat "$p" 2>/dev/null)" in CONFIGURED|CONNECTED|1) return 0;; esac
    done
    return 1
}
# Cost is zero on a cable-free boot: nothing sleeps unless a cable is actually present. The 3 s
# re-check rejects the transient CONNECTED blip the gadget emits while enumerating.
if [ ! -f /data/cinder/cable_escape_off ] && [ ! -f /contents/cinderhome_cable_off ] \
   && usb_connected; then
    sleep 3
    usb_connected && run_stock "$@"
fi

# /contents present? Cinder's DB, settings, art cache and log all live there, and a missing
# /contents is the signature of a corrupt vfat — exactly the state that bricked the device on
# 2026-07-26. Fail to stock: Sony's app degrades gracefully with no content, and the user gets a
# usable UI to work from instead of a logo loop. In a healthy boot /contents is mounted long
# before appmgr runs us (init.rc `on fs` -> mount_partition, then prepare_contentroot.sh), so
# this cannot false-trip on a mount race.
contents_up() { grep -q " /contents " /proc/mounts 2>/dev/null; }
contents_up || run_stock "$@"

# MSC ESCAPE — clear the latch without a shell: drop /contents/cinderhome_clear over USB-MSC.
# Consumed here (deleted) so it fires exactly once.
if [ -f "$MSC_CLEAR" ]; then
    rm -f "$MSC_CLEAR" 2>/dev/null
    rm -f "$DISABLED" "$OFF" "$BOOTCOUNT" 2>/dev/null
    sync
fi

# SELF-HEALING LATCH: a bad-boot revert used to be permanent — the off flag is checked below
# before anything counts, and NOTHING in the running app ever clears it (the app can't: it never
# gets to run while the flag is set). So a single false trip meant "boots to stock forever" until
# the flags were removed by hand over USB-MSC. Here: if a NEWER cinder-home binary has been
# installed since the latch was written, that is a deliberate "try again" signal from the
# developer, so clear the latch and give the new build its chance. If the shell lacks `-nt` the
# test just fails and we leave the latch alone (safe fallback).
if [ -f "$DISABLED" ] && [ "$HOME_BIN" -nt "$DISABLED" ] 2>/dev/null; then
    rm -f "$DISABLED" "$OFF" "$BOOTCOUNT" 2>/dev/null; sync
fi

# ONE-SHOT stock boot: consume the flag and hand over. Checked BEFORE the counter is incremented,
# because this is a deliberate user choice, not a failed boot — it must not spend a bad-boot life.
if [ -f "$ONCE" ] || [ -f "$MSC_ONCE" ]; then
    rm -f "$ONCE" 2>/dev/null
    rm -f "$MSC_ONCE" 2>/dev/null
    sync
    run_stock "$@"
fi

# explicit disable / missing binary -> stock, no counting
[ -f "$OFF" ] && run_stock "$@"
[ -f "$MSC_OFF" ] && run_stock "$@"
[ ! -x "$HOME_BIN" ] && run_stock "$@"

# THE COUNTER MUST BE PERSISTABLE OR WE DO NOT RUN. This is the rule whose absence caused the
# brick: if the counter can't be written, a hung build accumulates nothing and loops forever with
# no way out. No working safety net -> don't take the risk. Proven by write-then-read-back, not by
# a `[ -w ]` test, because a full or read-only fs passes -w and still fails the write.
mkdir -p "$STATE" 2>/dev/null
if ! echo probe > "$STATE/.wtest" 2>/dev/null || [ "$(cat "$STATE/.wtest" 2>/dev/null)" != "probe" ]; then
    run_stock "$@"
fi
rm -f "$STATE/.wtest" 2>/dev/null

# bad-boot counter: increment + persist FIRST. cinder-home resets it once healthy.
n=0; [ -f "$BOOTCOUNT" ] && n=$(cat "$BOOTCOUNT" 2>/dev/null)
# a partial write could leave non-numeric garbage -> treat as 0 (don't let `$(())`/`[ -ge ]` error)
case "$n" in ''|*[!0-9]*) n=0;; esac
n=$((n + 1)); echo "$n" > "$BOOTCOUNT"; sync
if [ "$n" -ge "$MAXBAD" ]; then
    touch "$OFF" "$DISABLED"; sync
    # mirror the latch onto /contents so it is VISIBLE over USB-MSC — the user can see why they
    # are on stock, and `cinderhome_clear` next to it is the documented way back.
    touch /contents/cinderhome_DISABLED_badboot 2>/dev/null; sync
    run_stock "$@"
fi

# optional USB-DAC->LDAC bridge supervisor (no-op if the bridge isn't installed). Started
# HERE because appmgr execs only this launcher at boot — nothing else starts it.
[ -x /system/vendor/unknown321/bin/ldac-run.sh ] && \
    /system/vendor/unknown321/bin/ldac-run.sh >/dev/null 2>&1 &

# Hand over to cinder-home (replaces this process; keeps the appmgr-expected name/args).
# THE REDIRECT MUST NOT BE ABLE TO STOP THE EXEC. `exec cmd >file` that cannot open `file` makes
# sh exit WITHOUT exec'ing — appmgr then sees no foreground app and reboots. With the log on
# /contents that turned "vfat won't mount" into an unrecoverable logo loop (2026-07-26 brick).
# So: prove the log path opens first, fall back to /data (ext4), and if neither opens, run with
# the inherited stdio rather than not running at all.
# The probe MUST run in a SUBSHELL. `:` is a POSIX *special builtin*, and a redirection failure
# on a special builtin makes the shell EXIT (dash: rc=2) instead of returning non-zero — so the
# plain `: > "$LOGF" || LOGF=fallback` form dies before it can fall back, reproducing the exact
# no-exec-then-appmgr-reboot bug it was written to prevent. A subshell contains the exit.
can_write() { ( : > "$1" ) 2>/dev/null; }
# Keep ONE previous boot's log. Both `exec > "$LOGF"` and can_write's probe truncate, so a boot
# that crashed erased the evidence of its own crash as soon as the next boot started — twice on
# 2026-07-26, including the frozen-panel boot whose log was the whole diagnosis. One rename makes
# the failing boot readable from the boot after it (`tools/flash.sh --cat cinderhome.log.1`).
# NO `-f` HERE. This device's /bin/mv is toolbox, and toolbox mv REJECTS the flag outright —
# `mv -f a b` prints "failed on '-f'", returns 255 and MOVES NOTHING (measured on device
# 2026-08-19). So this rotation, added precisely so a crashed boot's log survives into the next
# boot, has never once run: every boot truncated the evidence it was written to keep. Plain `mv`
# overwrites an existing destination on toolbox, which is all this needs.
for l in /contents/cinderhome.log /data/cinder/cinderhome.log; do
    [ -f "$l" ] && mv "$l" "$l.1" 2>/dev/null
done
LOGF=/contents/cinderhome.log
can_write "$LOGF" || LOGF=/data/cinder/cinderhome.log
can_write "$LOGF" || LOGF=""

# ── CRASH SUPERVISOR ──────────────────────────────────────────────────────────────────────────
# WHY THIS EXISTS: until now the launcher `exec`'d cinder-home, so the shell was GONE. When the
# app died — a segfault, an allocation failure under fragmentation (this process has died that way
# before) — there was nothing left to restart it. appmgr installs a SIGCHLD handler
# (AppManagerService::OnInit: `sigaction(17, ...)`, SA_SIGINFO|SA_RESETHAND) and reboots the device
# on `Application process is killed! appmgrservice will exit...`. So one crash cost a full reboot,
# the user's place in the queue, and a bad-boot life.
#
# WHY STAYING ALIVE WORKS: appmgr's SIGCHLD only fires for its OWN direct child. It fork+execvp's
# THIS SCRIPT (ProcessController::Invoke), and waits on that pid (ProcessController::WaitFinished).
# If the shell stays alive and reaps cinder-home itself, appmgr sees a live, foreground child and
# stays satisfied — the same reason SIGSTOP on the stock Qt app was safe where SIGKILL was not
# (analysis/F_appmgr_home/RE_findings.md §3).
#
# WHAT IT DOES NOT TOUCH: every escape above this line is evaluated BEFORE the first launch and is
# unchanged. The ladder rule still holds — the supervisor is the shell we already are, so it adds
# no dependency the app did not already have, and rung 0 (cable) does not go through it at all.
#
# THE RESPAWN SET IS A WHITELIST, NOT A BLACKLIST. Only deaths that mean "crashed" are respawned;
# an rc we do not recognise falls through to `exit`, which is EXACTLY today's behaviour. So a bug
# in the accounting below degrades to the old reboot-and-count net rather than disabling it.
#   respawn:     132 ILL  133 TRAP  134 ABRT  135 BUS  136 FPE  139 SEGV  141 PIPE  158/159
#   hand back:   0   deliberate exit (Settings ▸ Boot to stock arms its flag then _exit(0), and
#                    relies on appmgr rebooting us — respawning would break it)
#                42  self-diagnosed fatal (guard/watchdog `_exit(42)`), whose whole contract is
#                    "die fast so the bad-boot counter reverts to stock"
#                143/137 SIGTERM/SIGKILL — somebody killed us ON PURPOSE (appmgr's
#                    DoKillAndWait, a shutdown, or `kill` from an adb shell). Fighting that would
#                    make the app unkillable.
NO_RESPAWN=$STATE/no_respawn              # kill switch, /data
MSC_NO_RESPAWN=/contents/cinderhome_norespawn   # same, settable over USB-MSC
RESPAWN_MAX_FAST=3        # consecutive crashes inside the healthy window before giving up
RESPAWN_HEALTHY_S=30      # a run at least this long "counts" — it resets the consecutive tally
RESPAWN_MAX_TOTAL=10      # absolute cap per boot, so a 31-s crash cycle cannot loop forever

# WIRED VOLUME CURVE. load_sony_driver installs the stock output volume table on every boot, so the
# choice has to be re-applied on every boot too — this is not an install-time patch.
#
# The stock A50 table wastes 40 of the 120 steps (vol 40-60 and 100-120 are both dead, measured in
# analysis/RE_volume_pop.md) and coarsens toward the top where the volume pop is worst. cinder-
# voltable is setuid-root because the tables go into /proc/icx_audio_cxd3778gf_data/, which the
# launcher (uid 100, like the app) cannot write.
#
# Best-effort and deliberately quiet on failure: a missing helper or an unreadable conf leaves the
# stock curve, which is exactly what the device does without any of this. Nothing here may stop a
# boot — the audio path is already up by now either way.
VOLTABLE_CONF=/contents/cinder_voltable.conf
VOLTABLE_BIN=/system/vendor/unknown321/bin/cinder-voltable   # same dir as HOME_BIN above
if [ -x "$VOLTABLE_BIN" ] && [ -f "$VOLTABLE_CONF" ]; then
    vt=$(cat "$VOLTABLE_CONF" 2>/dev/null | tr -d " \t\r\n")
    case "$vt" in
        stock|wm1a|w1)
            if "$VOLTABLE_BIN" "$vt" >/dev/null 2>&1; then
                log "volume curve: $vt applied"
            else
                log "volume curve: cinder-voltable $vt FAILED — stock curve stays"
            fi
            ;;
        "") : ;;
        *)  log "volume curve: unknown value '$vt' in $VOLTABLE_CONF — stock curve stays" ;;
    esac
fi

# Kill switch: restores the pre-supervisor `exec`. The escape for the escape — a file drop over
# USB-MSC needs strictly less than the supervisor it disables.
if [ -f "$NO_RESPAWN" ] || [ -f "$MSC_NO_RESPAWN" ]; then
    [ -n "$LOGF" ] && exec "$HOME_BIN" "$@" >"$LOGF" 2>&1
    exec "$HOME_BIN" "$@"
fi

# Clock with the fewest dependencies available: procfs, which the kernel guarantees. Contained in
# a command substitution so a read failure cannot take the shell down (the `:`-special-builtin
# lesson). Unreadable -> 0 -> every run looks "fast" -> we escalate to stock sooner, which is the
# safe direction to fail.
uptime_s() {
    s=$( (read u _ < /proc/uptime && echo "${u%%.*}") 2>/dev/null )
    case "$s" in ''|*[!0-9]*) s=0;; esac
    echo "$s"
}
can_append() { ( : >> "$1" ) 2>/dev/null; }
log_sv() {
    echo "cinderhome-launch: $*"
    [ -n "$LOGF" ] && ( echo "cinderhome-launch: $*" >> "$LOGF" ) 2>/dev/null
    true
}
# The redirect rides on a SIMPLE COMMAND, never on `exec`. A redirection failure on a simple
# command is just a non-zero rc; on `exec` it makes sh exit WITHOUT running anything, which is the
# precise shape of the 2026-07-26 brick. /contents also legitimately disappears mid-session during
# USB-MSC, so this path has to survive the log going away — hence the re-probe every launch.
run_home() {
    if [ -n "$LOGF" ] && can_append "$LOGF"; then
        "$HOME_BIN" "$@" >>"$LOGF" 2>&1
    else
        "$HOME_BIN" "$@"
    fi
}

fast=0
total=0
while : ; do
    # A hot-swap (`mv` a new binary over the live one) can land between two launches. Missing or
    # non-executable is not a crash to respawn into — hand the boot to stock instead of burning
    # the whole budget on rc=126/127.
    [ -x "$HOME_BIN" ] || { log_sv "$HOME_BIN vanished mid-session — falling back to stock"; run_stock "$@"; }

    t0=$(uptime_s)
    run_home "$@"
    rc=$?
    t1=$(uptime_s)
    ran=$((t1 - t0))
    [ "$ran" -lt 0 ] && ran=0

    case "$rc" in
        132|133|134|135|136|139|141|158|159) ;;    # a crash — fall through and respawn
        *) log_sv "cinder-home exited rc=$rc after ${ran}s (not a crash) — handing back to appmgr"
           exit "$rc" ;;
    esac

    total=$((total + 1))
    if [ "$ran" -ge "$RESPAWN_HEALTHY_S" ]; then fast=0; else fast=$((fast + 1)); fi
    log_sv "cinder-home CRASHED rc=$rc after ${ran}s (respawn $total, $fast consecutive fast)"

    # Give up for THIS BOOT ONLY — deliberately not a latch. A crash after the app has already run
    # healthily is a different animal from one that never paints: the bad-boot counter above is the
    # net for "never paints", and latching on a runtime crash is how a device ends up stuck on
    # stock forever (2026-07-26). The next boot tries Cinder again.
    #   Handing over mid-session is itself unproven — appmgr already has its Foreground ACK from
    # the instance that just died. If the Qt app cannot re-handshake, appmgr times out and reboots,
    # which lands on the bad-boot counter. That is the failure branch of a failure branch, and it
    # ends at stock either way.
    if [ "$fast" -ge "$RESPAWN_MAX_FAST" ]; then
        log_sv "$fast crashes each under ${RESPAWN_HEALTHY_S}s — handing this boot to the Sony player"
        run_stock "$@"
    fi
    if [ "$total" -ge "$RESPAWN_MAX_TOTAL" ]; then
        log_sv "$total crashes this boot — handing this boot to the Sony player"
        run_stock "$@"
    fi

    # Drop the GPU present path from the first respawn on. The Mali fbdev EGL stack is the least
    # proven code in the process and a plausible source of a SIGSEGV/SIGBUS; the software
    # framebuffer is the proven path and CINDER_GPU=0 wins over every opt-in. Costs frame rate,
    # buys a much better chance the retry survives.
    CINDER_GPU=0; export CINDER_GPU
    sleep 1
done
LAUNCH_EOF
# verify the launcher wrote fully (must contain its final exec line) before activating it
if ! "$BB" grep -q 'exec "\$HOME_BIN"' "$LAUNCH.tmp" 2>/dev/null; then
    echo "ERROR: launcher write was truncated. ABORT (no .appcfg change; stock intact)."
    "$BB" rm -f "$LAUNCH.tmp" 2>/dev/null; sync; umount /system 2>/dev/null; exit 0
fi
"$BB" chmod 0755 "$LAUNCH.tmp"
"$BB" mv -f "$LAUNCH.tmp" "$LAUNCH"
echo "wrote launcher: $LAUNCH"

# 4) repoint the .appcfg command: at the launcher, ATOMICALLY. This is THE most brick-sensitive
#    write: a truncated/empty .appcfg means appmgr can't launch ANY Home app, and the launcher
#    (hence the bad-boot counter) never runs -> unrecoverable soft-brick. So: write temp, VERIFY
#    it parses, then mv over the live one (rename is atomic on one fs). Keep name/type/hidden =
#    the stock Home contract; matches the stock 4-line format exactly.
"$BB" cat > "$APPCFG.tmp" <<'APPCFG_EOF'
name: HgrmMediaPlayerApp
command: /system/vendor/unknown321/bin/cinderhome-launch.sh
type: Home
hidden: false
APPCFG_EOF
if ! "$BB" grep -q '^command: /system/vendor/unknown321/bin/cinderhome-launch.sh$' "$APPCFG.tmp" \
   || ! "$BB" grep -q '^type: Home$' "$APPCFG.tmp"; then
    echo "ERROR: new .appcfg failed verification — NOT activating (stock .appcfg untouched)."
    "$BB" rm -f "$APPCFG.tmp" 2>/dev/null; sync; umount /system 2>/dev/null; exit 0
fi
"$BB" chmod 0644 "$APPCFG.tmp"
"$BB" mv -f "$APPCFG.tmp" "$APPCFG"
echo "repointed $APPCFG command: -> $LAUNCH"

# ── FINAL SANITY GATE ─────────────────────────────────────────────────────────────────────
# A half/broken install must boot to working STOCK, never soft-brick. Verify every piece the
# boot path needs; if ANY is wrong, restore the stock .appcfg (revert) before rebooting.
ok=1
[ -x "$BIN/cinder-home" ]    || { echo "sanity: cinder-home not executable"; ok=0; }
[ -x "$LAUNCH" ]             || { echo "sanity: launcher not executable"; ok=0; }
"$BB" grep -q 'cinderhome-launch.sh' "$APPCFG" 2>/dev/null || { echo "sanity: .appcfg not repointed"; ok=0; }
[ -x "$SONYBIN/HgrmMediaPlayerApp" ] || { echo "sanity: STOCK revert target missing!"; ok=0; }
[ -s "$APPCFG.real" ]       || { echo "sanity: .appcfg.real backup missing"; ok=0; }
if [ "$ok" != 1 ]; then
    echo "!! SANITY FAILED — reverting .appcfg to stock so the device boots normally."
    if [ -s "$APPCFG.real" ]; then
        "$BB" cat "$APPCFG.real" > "$APPCFG.tmp" && "$BB" mv -f "$APPCFG.tmp" "$APPCFG"
        echo "   restored stock .appcfg."
    fi
    sync; umount /system 2>/dev/null
    echo "== install ABORTED safely; device will boot the stock UI. =="
    exit 0
fi

# fresh install = enabled: clear any prior disable/bad-boot flags, on BOTH the current /data
# location and the legacy /contents one (an upgrade from a pre-2026-07-26 build leaves those).
"$BB" mkdir -p /data/cinder 2>/dev/null
"$BB" rm -f /data/cinder/off /data/cinder/bootcount /data/cinder/DISABLED_badboot /data/cinder/once_stock 2>/dev/null
"$BB" rm -f /contents/cinderhome_off /contents/cinderhome_bootcount /contents/cinderhome_DISABLED_badboot /contents/cinderhome_once 2>/dev/null
echo "cleared prior disable flags (fresh install = enabled)"
echo "left staged binary at $SRC (safe to delete once cinder-home is confirmed)"
sync
umount /system 2>/dev/null
echo "== done. reboot to normal; appmgr launches cinder-home as the Home app. =="
echo "   SAFETY: a failed/hung launch AUTO-REVERTS to stock after 4 boots (no wbrt)."
echo "   Escapes, in order of how little they depend on:"
echo "     1. boot with the USB CABLE CONNECTED -> stock. Needs no filesystem, always works."
echo "     2. create /contents/cinderhome_off over USB-MSC -> stock."
echo "     3. create /contents/cinderhome_clear over USB-MSC -> clears the latch, tries again"
echo "        (same as tools/flash.sh --clear-latch)."
echo "   NOTE (1) means charging at boot lands on stock. Turn it off for cable-heavy dev with"
echo "   /data/cinder/cable_escape_off or /contents/cinderhome_cable_off."
echo "   Or just install a newer cinder-home binary — the launcher self-heals when it is newer."
echo "   logs: /contents/cinderhome.log (falls back to /data/cinder/cinderhome.log)."
exit 0
