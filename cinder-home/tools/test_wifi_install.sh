#!/usr/bin/env bash
# test_wifi_install.sh — install_cinderhome.sh step 1g8 (the `wifi` component) on the host.
#
# Same shape as test_migrate.sh: the lines between `# >>> wifi` and `# <<< wifi` run as they are,
# with /system, /contents and /data pointed into a scratch root and $BB the host's tools.
set -u
HERE="$(cd "$(dirname "$0")/.." && pwd)"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
pass=0; fail=0
check() { if [ "$2" = "$3" ]; then echo "  ok   $1"; pass=$((pass+1));
  else echo "  FAIL $1 (got '$2', want '$3')"; fail=$((fail+1)); fi; }
INST="$HERE/deploy/install_cinderhome.sh"
FILES="compat.ko cfg80211.ko mtk_wmt_wifi_soc.ko wlan_mt.ko wpa_supplicant wpa_cli iw curl cacert.pem wifi-up.sh wifi-down.sh dhcp.sh"

R=
fresh() {
  R="$T/r$RANDOM$RANDOM"
  mkdir -p "$R/system/vendor/cinder/bin" "$R/contents" "$R/data/cinder/wifi"
  echo 'network={}' > "$R/data/cinder/wifi/wpa.conf"
}
stage() { for f in $FILES; do echo "$f body" > "$R/contents/cinder-wifi-$f"; done; echo helper > "$R/contents/cinder-wifi"; }
run() {  # run <WANT_WIFI>
  { echo 'BB='
    echo "VENDOR=$R/system/vendor/cinder; BIN=\$VENDOR/bin; WANT_WIFI=$1"
    awk '/^btaac_put\(\) \{/ {f=1} f; f && /^\}/ {exit}' "$INST"
    awk '$0 ~ "^# <<< wifi" {f=0} f; $0 ~ "^# >>> wifi" {f=1}' "$INST" \
      | sed "s#/system/#$R/system/#g; s#/contents/#$R/contents/#g; s#/data/#$R/data/#g"
  } > "$T/run.sh"
  sed -i 's#"\$BB" #command #g' "$T/run.sh"
  sh "$T/run.sh" > "$R/out" 2>&1
}
W() { echo "$R/system/vendor/cinder/wifi"; }
H() { echo "$R/system/vendor/cinder/bin/cinder-wifi"; }

echo "ticked, everything staged"
fresh; stage; mkdir -p "$(W)"; echo old > "$(W)/stale"; run 1
check "all twelve files are in the folder" "$(find "$(W)" -type f | wc -l | tr -d ' ')" 12
check "a module is 644" "$(stat -c %a "$(W)/wlan_mt.ko")" 644
check "a program is 755" "$(stat -c %a "$(W)/wpa_supplicant")" 755
check "the CA list is 644" "$(stat -c %a "$(W)/cacert.pem")" 644
check "the DHCP script is executable" "$(stat -c %a "$(W)/dhcp.sh")" 755
check "the old folder was replaced whole" "$([ -e "$(W)/stale" ] && echo kept || echo gone)" gone
check "the helper is setuid" "$(stat -c %a "$(H)")" 4755
check "no temporary folder is left" "$([ -e "$(W).new" ] && echo left || echo none)" none
check "the saved network stays" "$(cat "$R/data/cinder/wifi/wpa.conf")" 'network={}'
check "it says so" "$(grep -c 'wifi: installed' "$R/out")" 1

echo "ticked, one file missing"
fresh; stage; rm "$R/contents/cinder-wifi-iw"; mkdir -p "$(W)"; echo x > "$(H)"; run 1
check "nothing is installed" "$([ -e "$(W)" ] || [ -e "$(H)" ] && echo some || echo none)" none
check "the saved network stays (still ticked)" "$([ -f "$R/data/cinder/wifi/wpa.conf" ] && echo kept)" kept
check "it warns" "$(grep -c 'not all staged' "$R/out")" 1

echo "unticked after an install"
fresh; stage; run 1; run 0
check "the folder goes" "$([ -e "$(W)" ] && echo kept || echo gone)" gone
check "the helper goes" "$([ -e "$(H)" ] && echo kept || echo gone)" gone
check "the saved network (and its password) goes" "$([ -e "$R/data/cinder/wifi" ] && echo kept || echo gone)" gone

echo "$pass passed, $fail failed"
[ "$fail" = 0 ]
