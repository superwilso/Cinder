#!/usr/bin/env bash
# flash.sh — one-command Walkman firmware/.UPG flasher for the NW-A50 series.
#
# Wraps the proven pipeline (CLAUDE.md Part E7 / analysis/E_usbdac_ldac/README.md):
#   1. find the Walkman block device (MSC mode, attached to WSL via usbipd),
#   2. mount its data partition,
#   3. copy the chosen .UPG to the device root as NW_WM_FW.UPG (+ verify),
#   4. confirm, then trigger the Sony updater via scsitool do_fw_upgrade.
# The device then reboots into the UPDATER, runs the payload, and reboots to normal.
#
# This is BRICK-SAFE for our exec_file payloads: exec_file.sh clears the fw-upgrade
# flag before running, so a failed payload does not boot-loop. Recover via the wbrt
# eMMC backup if anything wedges.
#
# Usage:
#   tools/flash.sh <file.upg>              # detect, copy, confirm, flash
#   tools/flash.sh install                 # shortcut: cinder-home/dist/dev/cinder_home_install.upg
#   tools/flash.sh uninstall               # shortcut: cinder-home/dist/dev/cinder_home_uninstall.upg
#   tools/flash.sh --list                  # just show detected Walkman device(s)
#   tools/flash.sh --trigger-only          # device already holds NW_WM_FW.UPG; just fire
#   tools/flash.sh --ls                    # list the device root (read-only mount)
#   tools/flash.sh --cat <file>            # print a file from the device (e.g. a log)
#   tools/flash.sh --log                   # shortcut: --cat cinder_install.log
#   tools/flash.sh --push <file>           # copy a local file to the device root
#   tools/flash.sh --pull <file> [dest]    # copy a file OFF the device (binary-safe; e.g. MTPDB_copy.dat)
#   tools/flash.sh --clear-latch           # clear a bad-boot revert; give the build another chance
#   tools/flash.sh --cable-off             # cable-heavy dev session: stop cable-at-boot -> stock
#   tools/flash.sh --cable-on              # UNDO --cable-off. Do this when the session ends.
#
# Flags:
#   -d /dev/sdX   force the block device (skip autodetect)
#   -y            skip the confirmation prompt (non-interactive)
#   -s nw-a50     scsitool series (default nw-a50)
#
# Needs root for mount/umount and raw SCSI; re-runs itself under sudo if needed.

set -euo pipefail

# ---------------------------------------------------------------- paths/colors
SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SELF/.." && pwd)"
SCSITOOL="$REPO/artifacts/repos/rockbox/utils/nwztools/scsitools/scsitool"

if [ -t 1 ]; then
  C_R=$'\e[31m'; C_G=$'\e[32m'; C_Y=$'\e[33m'; C_B=$'\e[36m'; C_D=$'\e[2m'; C_0=$'\e[0m'
else
  C_R=; C_G=; C_Y=; C_B=; C_D=; C_0=
fi
say()  { printf '%s\n' "$*"; }
info() { printf '%s==>%s %s\n' "$C_B" "$C_0" "$*"; }
ok()   { printf '%s ok %s %s\n' "$C_G" "$C_0" "$*"; }
warn() { printf '%swarn%s %s\n' "$C_Y" "$C_0" "$*" >&2; }
die()  { printf '%s err %s %s\n' "$C_R" "$C_0" "$*" >&2; exit 1; }

# ---------------------------------------------------------------- arg parsing
DEV=""; ASSUME_YES=0; SERIES="nw-a50"; MODE="flash"; UPG=""; CATFILE=""; PUSHFILE=""; PULLFILE=""; PULLDEST=""
args=()
while [ $# -gt 0 ]; do
  case "$1" in
    -d) DEV="${2:-}"; shift 2;;
    -y) ASSUME_YES=1; shift;;
    -s) SERIES="${2:-}"; shift 2;;
    --list)         MODE="list"; shift;;
    --trigger-only) MODE="trigger"; shift;;
    --ls)           MODE="ls"; shift;;
    --cat)          MODE="cat"; CATFILE="${2:-}"; shift 2;;
    --log)          MODE="cat"; CATFILE="cinder_home_install.log"; shift;;
    --push)         MODE="push"; PUSHFILE="${2:-}"; shift 2;;
    --clear-latch)  MODE="clearlatch"; shift;;
    --cable-off)    MODE="cableoff"; shift;;
    --cable-on)     MODE="cableon"; shift;;
    --pull)         MODE="pull"; shift;
                    PULLFILE="${1:-}"; [ $# -gt 0 ] && shift;
                    if [ $# -gt 0 ] && [ "${1#-}" = "$1" ]; then PULLDEST="$1"; shift; fi;;
    -h|--help) grep -E '^#( |$)' "$0" | sed 's/^# \{0,1\}//'; exit 0;;
    install)   UPG="$REPO/cinder-home/dist/dev/cinder_home_install.upg";   shift;;
    uninstall) UPG="$REPO/cinder-home/dist/dev/cinder_home_uninstall.upg"; shift;;
    -*) die "unknown flag: $1";;
    *)  args+=("$1"); shift;;
  esac
done
[ -z "$UPG" ] && [ ${#args[@]} -gt 0 ] && UPG="${args[0]}"

[ -x "$SCSITOOL" ] || die "scsitool not found/executable at $SCSITOOL (build it: make -C ${SCSITOOL%/*})"

# Re-run the rest of this script under sudo, preserving the resolved args. Called
# only AFTER a device is confirmed present, so we never prompt for a password when
# there's nothing to flash.
reexec_root() {
  [ "$(id -u)" -eq 0 ] && return 0
  info "need root for mount + raw SCSI — re-running under sudo"
  local a=(-d "$DEV" -s "$SERIES")
  [ "$ASSUME_YES" = 1 ] && a+=(-y)
  [ "$MODE" = trigger ] && a+=(--trigger-only)
  [ "$MODE" = ls ] && a+=(--ls)
  [ "$MODE" = cat ] && a+=(--cat "$CATFILE")
  [ "$MODE" = push ] && a+=(--push "$PUSHFILE")
  [ "$MODE" = clearlatch ] && a+=(--clear-latch)
  [ "$MODE" = cableoff ] && a+=(--cable-off)
  [ "$MODE" = cableon ] && a+=(--cable-on)
  [ "$MODE" = pull ] && { a+=(--pull "$PULLFILE"); [ -n "$PULLDEST" ] && a+=("$PULLDEST"); }
  [ -n "$UPG" ] && a+=("$UPG")
  exec sudo -E "$0" "${a[@]}"
}

# ---------------------------------------------------------------- device detect
# Print the Walkman block device path on stdout, or empty if none.
# The A50 exposes TWO LUNs — internal storage (~55.9G) AND the microSD slot (0B
# when empty). Pick the LARGEST matching LUN so we never land on the empty slot,
# regardless of enumeration order across reboots.
detect_dev() {
  local d base vendor model size best="" bestsz=-1
  for d in /sys/block/sd*; do
    [ -e "$d" ] || continue
    base="$(basename "$d")"
    vendor="$(cat "$d/device/vendor" 2>/dev/null | tr -d ' ' || true)"
    model="$(cat "$d/device/model" 2>/dev/null || true)"
    # WSL's own disks are "Msft Virtual Disk" — skip them.
    printf '%s' "$model" | grep -qi 'virtual disk' && continue
    if printf '%s %s' "$vendor" "$model" | grep -qiE 'sony|walkman|wm-port'; then
      size="$(cat "$d/size" 2>/dev/null || echo 0)"   # 512-byte sectors
      if [ "${size:-0}" -gt "$bestsz" ]; then bestsz="$size"; best="/dev/$base"; fi
    fi
  done
  printf '%s' "$best"
}

describe_dev() {
  local dev="$1" base; base="$(basename "$dev")"
  local vendor model size
  vendor="$(cat "/sys/block/$base/device/vendor" 2>/dev/null | xargs || true)"
  model="$(cat "/sys/block/$base/device/model" 2>/dev/null | xargs || true)"
  size="$(lsblk -dno SIZE "$dev" 2>/dev/null | xargs || true)"
  printf '%s  [%s %s, %s]' "$dev" "$vendor" "$model" "$size"
}

# Pick the data partition (largest fs-bearing partition; fall back to whole disk).
data_partition() {
  local dev="$1" best="" bestsz=0 line name fstype size
  while IFS= read -r line; do
    name="$(awk '{print $1}' <<<"$line")"
    fstype="$(awk '{print $2}' <<<"$line")"
    size="$(awk '{print $3}' <<<"$line")"
    [ -n "$fstype" ] || continue
    case "$fstype" in vfat|exfat|fat|fat32|msdos|ntfs)
      if [ "${size:-0}" -gt "$bestsz" ]; then bestsz="$size"; best="/dev/$name"; fi;;
    esac
  done < <(lsblk -rno NAME,FSTYPE,SIZE -b "$dev" 2>/dev/null)
  printf '%s' "$best"
}

# ---------------------------------------------------------------- --list
if [ "$MODE" = "list" ]; then
  info "scsitool list_devices:"
  "$SCSITOOL" -c list_devices 2>&1 | sed 's/^/    /' || true
  d="$(detect_dev)"
  if [ -n "$d" ]; then ok "block device: $(describe_dev "$d")"; p="$(data_partition "$d")"
    [ -n "$p" ] && say "    data partition: $p"
  else
    warn "no Sony/Walkman block device in WSL."
    say  "    If it's plugged in, attach it to WSL from an admin PowerShell:"
    say  "      ${C_D}usbipd list${C_0}                     # find the BUSID (PID 054c:0ca0 = MSC mode)"
    say  "      ${C_D}usbipd bind   --busid <BUSID>${C_0}"
    say  "      ${C_D}usbipd attach --wsl --busid <BUSID>${C_0}"
  fi
  exit 0
fi

# ---------------------------------------------------------------- resolve device
if [ -z "$DEV" ]; then DEV="$(detect_dev)"; fi
if [ -z "$DEV" ]; then
  warn "no Sony/Walkman block device found in WSL."
  say  "Put the Walkman in MSC/UMS (mass-storage) mode, then attach it to WSL"
  say  "from an admin PowerShell on Windows:"
  say  "    usbipd list                          # PID 054c:0ca0 = mass-storage mode"
  say  "    usbipd bind   --busid <BUSID>"
  say  "    usbipd attach --wsl --busid <BUSID>"
  say  "Then re-run:  tools/flash.sh ${UPG:-<file.upg>}"
  die  "device not present."
fi
[ -b "$DEV" ] || die "$DEV is not a block device"
ok "Walkman: $(describe_dev "$DEV")"

# Device confirmed present — now escalate (mount + raw SCSI need root).
reexec_root

# ---------------------------------------------------------------- read-only modes
# --ls / --cat: mount the data partition READ-ONLY and inspect it (e.g. to read
# /contents/cinder_install.log, which lands at the root of the user FAT partition).
if [ "$MODE" = "ls" ] || [ "$MODE" = "cat" ]; then
  PART="$(data_partition "$DEV")"
  [ -n "$PART" ] || die "no FAT/exFAT data partition on $DEV (is it in MSC mode?)"
  MNT="$(mktemp -d /tmp/walkman.XXXXXX)"
  trap 'mountpoint -q "$MNT" && umount "$MNT" 2>/dev/null; rmdir "$MNT" 2>/dev/null' EXIT
  mount -o ro "$PART" "$MNT" 2>/dev/null || mount -t exfat -o ro "$PART" "$MNT" 2>/dev/null \
    || die "could not mount $PART read-only"
  if [ "$MODE" = "ls" ]; then
    info "root of $PART:"
    ls -la "$MNT" | sed 's/^/    /'
  else
    [ -n "$CATFILE" ] || die "--cat needs a filename (relative to the device root)"
    REL="${CATFILE#/}"; REL="${REL#contents/}"   # the user FAT root == /contents on device
    F="$MNT/$REL"
    [ -f "$F" ] || die "not found on device: $CATFILE (try --ls to see what's there)"
    info "==== $CATFILE ===="
    cat "$F"
  fi
  exit 0
fi

# ---------------------------------------------------------------- --pull
# Copy a file OFF the device (binary-safe: cp, not cat) — e.g. the dev build's
# /contents/MTPDB_copy.dat for offline schema work.
if [ "$MODE" = "pull" ]; then
  [ -n "$PULLFILE" ] || die "--pull needs a device filename (relative to the device root)"
  PART="$(data_partition "$DEV")"
  [ -n "$PART" ] || die "no FAT/exFAT data partition on $DEV (is it in MSC mode?)"
  MNT="$(mktemp -d /tmp/walkman.XXXXXX)"
  trap 'mountpoint -q "$MNT" && umount "$MNT" 2>/dev/null; rmdir "$MNT" 2>/dev/null' EXIT
  mount -o ro "$PART" "$MNT" 2>/dev/null || mount -t exfat -o ro "$PART" "$MNT" 2>/dev/null \
    || die "could not mount $PART read-only"
  REL="${PULLFILE#/}"; REL="${REL#contents/}"
  F="$MNT/$REL"
  [ -f "$F" ] || die "not found on device: $PULLFILE (try --ls)"
  DEST="${PULLDEST:-$PWD/$(basename "$PULLFILE")}"
  cp -f "$F" "$DEST"; sync
  chmod a+r "$DEST" 2>/dev/null || true
  ok "pulled $PULLFILE -> $DEST ($(stat -c %s "$DEST") bytes)"
  exit 0
fi

# ---------------------------------------------------------------- --push
# Copy a local file to the device storage root (e.g. the cinder-device binary the
# Cinder installer expects at /contents/cinder-device).
if [ "$MODE" = "push" ]; then
  [ -n "$PUSHFILE" ] || die "--push needs a file path"
  [ -f "$PUSHFILE" ] || die "file not found: $PUSHFILE"
  PART="$(data_partition "$DEV")"
  [ -n "$PART" ] || die "no FAT/exFAT data partition on $DEV (is it in MSC mode?)"
  MNT="$(mktemp -d /tmp/walkman.XXXXXX)"
  trap 'mountpoint -q "$MNT" && umount "$MNT" 2>/dev/null; rmdir "$MNT" 2>/dev/null' EXIT
  mount "$PART" "$MNT" 2>/dev/null || mount -t exfat "$PART" "$MNT" 2>/dev/null \
    || die "could not mount $PART"
  base="$(basename "$PUSHFILE")"
  info "copying $base -> device root ($(stat -c %s "$PUSHFILE") bytes)"
  cp -f "$PUSHFILE" "$MNT/$base"
  sync
  umount "$MNT"; trap - EXIT
  ok "pushed $base to device root"
  exit 0
fi

# ---------------------------------------------------------------- --clear-latch
# Recover from a bad-boot revert. The latch itself now lives on /data (ext4) — see
# deploy/install_cinderhome.sh "WHERE THE STATE LIVES" — and USB-MSC only ever exposes the vfat
# /contents partition, so we cannot delete it directly from here. Instead we drop the trigger file
# `cinderhome_clear`, which the launcher consumes on the next boot: it removes the /data latch and
# gives the installed build another chance. Legacy /contents copies are cleaned up too.
if [ "$MODE" = "clearlatch" ]; then
  PART="$(data_partition "$DEV")"
  [ -n "$PART" ] || die "no FAT/exFAT data partition on $DEV (is it in MSC mode?)"
  MNT="$(mktemp -d /tmp/walkman.XXXXXX)"
  trap 'mountpoint -q "$MNT" && umount "$MNT" 2>/dev/null; rmdir "$MNT" 2>/dev/null' EXIT
  mount "$PART" "$MNT" 2>/dev/null || mount -t exfat "$PART" "$MNT" 2>/dev/null \
    || die "could not mount $PART"
  n=0
  # cinderhome_once is swept too, and deliberately is NOT handled by the launcher's clear path:
  # the one-shot "Boot to stock" outranks a clear on purpose (test_launcher.sh "one-shot fires even
  # when latched clear"). That is right for the launcher and wrong for us — running --clear-latch
  # means "get me back to cinder", and a pending one-shot would spend the very next boot on stock,
  # which reads exactly like the clear having silently failed.
  for f in cinderhome_off cinderhome_bootcount cinderhome_DISABLED_badboot cinderhome_once; do
    if [ -e "$MNT/$f" ]; then rm -f "$MNT/$f" && { info "removed $f"; n=$((n+1)); }
    else info "$f not present (ok)"; fi
  done
  : > "$MNT/cinderhome_clear" && info "armed cinderhome_clear (launcher clears the /data latch)"
  cable_off=0
  [ -e "$MNT/cinderhome_cable_off" ] && cable_off=1
  sync
  umount "$MNT"; trap - EXIT
  ok "cleared $n legacy file(s) + armed the clear trigger."
  if [ "$cable_off" = 1 ]; then
    ok "cinderhome_cable_off is SET — the cable escape is off, so you can boot with USB in."
    warn "rung 0 of the escape ladder is disabled. Put it back with --cable-on when you are done."
  else
    ok "Boot with USB UNPLUGGED — a cable at boot is itself an escape to stock."
    say "    Rebooting a lot this session? ${C_B}tools/flash.sh --cable-off${C_0} stops that,"
    say "    and ${C_B}--cable-on${C_0} puts the escape back when you are finished."
  fi
  warn "One-shot 'Boot to stock' can also be latched on /data (once_stock), which USB-MSC cannot"
  warn "reach. If this boot still lands on stock, clear it with: adb shell rm /data/cinder/once_stock"
  exit 0
fi

# ------------------------------------------------------- --cable-off / --cable-on
# Rung 0 of the escape ladder is "boot with the cable in -> stock". It needs no filesystem, no
# shell and no working counter, which is exactly why it is checked first and why it is the escape
# that recovers the failures nothing else can (see install_cinderhome.sh "cable escape").
#
# The cost is that a cable-heavy session — flash, reboot, flash, reboot — lands on stock every
# time, because the flag the installer uses (/data/cinder/cable_escape_off) is on ext4 and USB-MSC
# only ever exposes the vfat /contents. This writes its MSC-settable twin instead, which the
# launcher honours identically:
#     /data/cinder/cable_escape_off     persistent, needs a shell
#     /contents/cinderhome_cable_off    same effect, settable from here
#
# TAKE IT BACK WHEN THE SESSION ENDS (`--cable-on`). cinder-install.sh treats the ext4 flag as a
# LOAN for exactly this reason: leaving it set silently removes the one escape that depends on
# nothing, and you will not notice until the boot you needed it.
if [ "$MODE" = "cableoff" ] || [ "$MODE" = "cableon" ]; then
  PART="$(data_partition "$DEV")"
  [ -n "$PART" ] || die "no FAT/exFAT data partition on $DEV (is it in MSC mode?)"
  MNT="$(mktemp -d /tmp/walkman.XXXXXX)"
  trap 'mountpoint -q "$MNT" && umount "$MNT" 2>/dev/null; rmdir "$MNT" 2>/dev/null' EXIT
  mount "$PART" "$MNT" 2>/dev/null || mount -t exfat "$PART" "$MNT" 2>/dev/null \
    || die "could not mount $PART"
  if [ "$MODE" = "cableoff" ]; then
    : > "$MNT/cinderhome_cable_off"
    sync; umount "$MNT"; trap - EXIT
    ok "cable escape DISABLED — booting with the cable in now stays on cinder-home."
    warn "rung 0 of the escape ladder is OFF. Put it back with --cable-on when you are done."
    warn "Rung 1 (the bad-boot counter, MAXBAD=4) still covers a build that will not start."
  else
    if [ -e "$MNT/cinderhome_cable_off" ]; then
      rm -f "$MNT/cinderhome_cable_off"; ok "removed cinderhome_cable_off"
    else
      info "cinderhome_cable_off not present (ok)"
    fi
    sync; umount "$MNT"; trap - EXIT
    ok "rung 0 restored — a cable at boot escapes to stock again."
    warn "If /data/cinder/cable_escape_off is ALSO set, this did not fully restore rung 0."
    warn "That one needs a shell: adb shell rm /data/cinder/cable_escape_off"
  fi
  exit 0
fi

# ---------------------------------------------------------------- copy stage
if [ "$MODE" = "flash" ]; then
  [ -n "$UPG" ] || die "no .UPG given. Try: tools/flash.sh install | uninstall | <file.upg>"
  [ -f "$UPG" ] || die "file not found: $UPG"
  case "$UPG" in *.upg|*.UPG) :;; *) warn "'$UPG' doesn't end in .upg — continuing anyway";; esac

  PART="$(data_partition "$DEV")"
  [ -n "$PART" ] || die "no FAT/exFAT data partition found on $DEV (is it really in MSC mode?)"

  MNT="$(mktemp -d /tmp/walkman.XXXXXX)"
  cleanup() { mountpoint -q "$MNT" && umount "$MNT" 2>/dev/null || true; rmdir "$MNT" 2>/dev/null || true; }
  trap cleanup EXIT

  info "mounting $PART → $MNT"
  mount "$PART" "$MNT" 2>/dev/null || mount -t exfat "$PART" "$MNT" 2>/dev/null \
    || die "could not mount $PART (need exfat/vfat support in the WSL kernel)"

  DEST="$MNT/NW_WM_FW.UPG"
  src_sz="$(stat -c %s "$UPG")"
  if [ -f "$DEST" ]; then warn "overwriting existing $(basename "$DEST") ($(stat -c %s "$DEST") bytes)"; fi
  info "copying $(basename "$UPG") ($src_sz bytes) → device root as NW_WM_FW.UPG"
  cp -f "$UPG" "$DEST"
  sync
  dst_sz="$(stat -c %s "$DEST")"
  [ "$src_sz" = "$dst_sz" ] || die "size mismatch after copy ($src_sz vs $dst_sz) — aborting before flash"
  ok "copied + verified ($dst_sz bytes)"

  umount "$MNT"; trap - EXIT; cleanup
  ok "unmounted cleanly"
fi

# ---------------------------------------------------------------- confirm + fire
say ""
say "${C_Y}About to trigger a firmware upgrade on:${C_0}"
say "    device : $(describe_dev "$DEV")"
[ "$MODE" = "flash" ] && say "    payload: $UPG"
[ "$MODE" = "trigger" ] && say "    payload: NW_WM_FW.UPG already on the device"
say "    series : $SERIES"
say ""
say "The Walkman will reboot into the Sony UPDATER, run the payload, then reboot to normal."
say "(exec_file payloads clear the upgrade flag first — no boot-loop. wbrt backup = recovery.)"
say ""

if [ "$ASSUME_YES" -ne 1 ]; then
  printf '%sProceed? type "flash" to continue: %s' "$C_Y" "$C_0"
  read -r reply
  [ "$reply" = "flash" ] || die "aborted (you typed '$reply')."
fi

info "scsitool -s $SERIES $DEV do_fw_upgrade"
"$SCSITOOL" -s "$SERIES" "$DEV" do_fw_upgrade
ok "upgrade command sent."
say ""
say "Next:"
say "  • The screen should show the Sony updater, then reboot to normal."
say "  • The device drops off USB during the upgrade — that's expected."
if [ "$MODE" = "flash" ] && printf '%s' "$UPG" | grep -qi 'probe_install'; then
  say "  • Re-attach in MSC mode and check the install log:"
  say "      tools/flash.sh --list        # confirm it's back"
  say "      then read /contents/cinder_install.log off the device"
fi
