#!/usr/bin/env bash
# Sandbox test for the installer's Walkman One tuning step (the `w1tuning` component). The block
# is the installer's own lines, cut out at its `# >>> w1-tuning` fences. Sony's unpacker and the
# real images are stood in for; what is checked is the rule that matters on a player: nothing
# unverified is written to the partition, the player's own data is saved first and kept, and a
# write that does not read back is undone.
set -u
SP="$(mktemp -d "${TMPDIR:-/tmp}/cinder_w1t_test.XXXXXX")"; trap 'rm -rf "$SP"' EXIT
SRC="$(cd "$(dirname "$0")/.." && pwd)/deploy/install_cinderhome.sh"
PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then printf '  ok    %-58s -> %s\n' "$1" "$2"; PASS=$((PASS+1))
  else printf '  FAIL  %-58s -> %s (want %s)\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi; }
md5() { md5sum < "$1" | cut -d' ' -f1; }

# $1 = /opt2/sig ("" = not Walkman One), $2 = what fwpup produces: good | bad | none
fresh() {
  R="$(mktemp -d "$SP/r.XXXXXX")"
  mkdir -p "$R/opt2" "$R/contents" "$R/dev" "$R/bin" "$R/tun/WM1Z_external_tuning/Data/Device" \
           "$R/tun/Neutral_&_Warm_external_tuning/Data/Device"
  [ -n "$1" ] && echo "$1" > "$R/opt2/sig"
  echo package > "$R/tun/WM1Z_external_tuning/Data/Device/NW_WM_FW.UPG"
  echo package > "$R/tun/Neutral_&_Warm_external_tuning/Data/Device/NW_WM_FW.UPG"
  echo "this player's own nvram" > "$R/dev/p3"; echo "the tuning's nvram" > "$R/good.img"
  # fwpup -z -f <upg> -2 <out>: needs md5.txt beside it, like the real one
  printf '#!/bin/sh\n[ -s md5.txt ] || exit 1\ncase %s in good) cat "%s" > "$5";; bad) echo junk > "$5";; esac\n' \
      "$2" "$R/good.img" > "$R/bin/fwpup"
  # busybox: dd can be made to write garbage once, the way a failing eMMC write would look
  cat > "$R/bin/busybox" <<EOF
#!/bin/sh
cmd="\$1"; shift
if [ "\$cmd" = dd ] && [ -e '$R/badwrite' ]; then rm '$R/badwrite'; echo garbage > '$R/dev/p3'; exit 0; fi
exec "\$cmd" "\$@"
EOF
  chmod +x "$R/bin/fwpup" "$R/bin/busybox"
  awk '/^# <<< w1-tuning/{f=0} f; /^# >>> w1-tuning/{f=1}' "$SRC" \
  | sed -e "s#/opt2/sig#$R/opt2/sig#" -e "s#/system/etc/.mod/tunings#$R/tun#" \
        -e "s#/dev/block/mmcblk0p3#$R/dev/p3#" -e "s#/contents/#$R/contents/#g" \
        -e "s/ccb29dd20d0116042d7f02f85208693c/$(md5 "$R/good.img")/" \
        -e "s/d7d0878020c38e869ec88577a0a001f7/$(md5 "$R/good.img")/g" > "$R/block.sh"
}
# shellcheck disable=SC2034  # read by the sourced block
run() { ( PATH="$R/bin:$PATH"; BB="$R/bin/busybox"; WANT_W1TUNING="${1:-1}"; . "$R/block.sh" ); }
P3() { cat "$R/dev/p3"; }

echo "── the option is off ──"
fresh wm1z good; o="$(run 0)"
check "nothing is written"                         "$(P3)" "this player's own nvram"
check "and nothing is said"                        "$o" ""

echo "── not a Walkman One player ──"
fresh "" good; o="$(run)"
check "nothing is written"                         "$(P3)" "this player's own nvram"
check "it says why"                                "$(echo "$o" | grep -c 'not running Walkman One')" "1"

echo "── Walkman One, signature wm1z, the package unpacks ──"
fresh wm1z good; o="$(run)"
check "the partition holds the tuning"             "$(P3)" "the tuning's nvram"
check "the player's own data is kept on the drive" "$(cat "$R/contents/cinder_nvram_backup.img")" "this player's own nvram"
check "the working folder is gone"                 "$(ls "$R/contents")" "cinder_nvram_backup.img"
o="$(run)"
check "a second run finds it applied"              "$(echo "$o" | grep -c 'already applied')" "1"
check "…and leaves the first backup alone"         "$(cat "$R/contents/cinder_nvram_backup.img")" "this player's own nvram"

echo "── the folder with '&' in its name (signature warm) ──"
fresh warm good; run >/dev/null
check "the partition holds the tuning"             "$(P3)" "the tuning's nvram"

echo "── the package does not unpack (a live install: 'Invalid signature') ──"
for how in bad none; do
  fresh wm1z $how; o="$(run)"
  check "fwpup=$how: nothing is written"           "$(P3)" "this player's own nvram"
  check "fwpup=$how: it says NOTHING was written"  "$(echo "$o" | grep -c 'NOTHING was written')" "1"
done

echo "── the write does not read back ──"
fresh wm1z good; : > "$R/badwrite"; o="$(run)"
check "the player's own data is put back"          "$(P3)" "this player's own nvram"
check "and it says so"                             "$(echo "$o" | grep -c 'was put back')" "1"

echo "── the drive cannot hold the backup ──"
fresh wm1z good; mkdir "$R/contents/cinder_nvram_backup.img"; o="$(run 2>/dev/null)"
check "nothing is written"                         "$(P3)" "this player's own nvram"

echo; printf '%s passed, %s failed\n' "$PASS" "$FAIL"; [ "$FAIL" = 0 ]
