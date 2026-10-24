#!/usr/bin/env bash
# test_migrate.sh — two blocks of install_cinderhome.sh, and the uninstaller, on the host.
#
#   stock-appcfg  the .appcfg.real backup. The owner's player carried one that named Cinder's own
#                 launcher (2026-09-22 to 10-24), so every restore "restored" Cinder.
#   migrate       step 4b: an install from before 2026-10-24 lives in Wampy's folder,
#                 /system/vendor/unknown321. Cinder's files leave it, Wampy's stay.
#
# Same shape as test_install_mounts.sh: the lines between the installer's `# >>> NAME` and
# `# <<< NAME` fences run as they are, with /system pointed into a scratch root and $BB the host.
set -u
HERE="$(cd "$(dirname "$0")/.." && pwd)"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
pass=0; fail=0
check() { if [ "$2" = "$3" ]; then echo "  ok   $1"; pass=$((pass+1));
  else echo "  FAIL $1 (got '$2', want '$3')"; fail=$((fail+1)); fi; }

block() {  # the fenced lines of the installer, paths moved into $R
  awk -v n="$1" '$0 ~ "^# <<< " n {f=0} f; $0 ~ "^# >>> " n {f=1}' "$HERE/deploy/install_cinderhome.sh" \
    | sed "s#/system/#$R/system/#g"
}
R=
fresh() {
  R="$T/r$RANDOM$RANDOM"
  mkdir -p "$R/system/vendor/sony/bin" "$R/system/vendor/cinder/bin"
  printf '#!/bin/sh\nexit 0\n' > "$R/cleanup"
}
APP() { echo "$R/system/vendor/sony/bin/HgrmMediaPlayerApp.appcfg"; }
NEW_CFG='name: HgrmMediaPlayerApp
command: /system/vendor/cinder/bin/cinderhome-launch.sh
type: Home
hidden: false'
OLD_CFG="${NEW_CFG/vendor\/cinder/vendor/unknown321}"
STOCK_CFG='name: HgrmMediaPlayerApp
command: HgrmMediaPlayerApp
type: Home
hidden: false'
run() {  # run <block> — with the installer's variables, as they are at that point
  { echo 'BB=; cleanup_mounts() { :; }'
    echo "VENDOR=$R/system/vendor/cinder; BIN=\$VENDOR/bin; LAUNCH=\$BIN/cinderhome-launch.sh"
    echo "APPCFG=$R/system/vendor/sony/bin/HgrmMediaPlayerApp.appcfg; VT_DIR=$R/system/vendor/cinder/audio_dac"
    block "$1"; } > "$T/run.sh"
  # $BB is empty, so "$BB" cat runs as the command "" — give the host's tools that name instead.
  sed -i 's#"\$BB" #command #g' "$T/run.sh"
  sh "$T/run.sh" > "$R/out" 2>&1
}

echo "the .appcfg backup"
fresh; echo "$OLD_CFG" > "$(APP)"; echo "$OLD_CFG" > "$(APP).real"; run stock-appcfg
check "a backup naming the launcher is replaced by Sony's own" "$(cat "$(APP).real")" "$STOCK_CFG"
fresh; echo "$STOCK_CFG" > "$(APP)"; run stock-appcfg
check "no backup, Sony's .appcfg in place: it is copied" "$(cat "$(APP).real")" "$STOCK_CFG"
fresh; echo "$OLD_CFG" > "$(APP)"; run stock-appcfg
check "no backup, the launcher in place: Sony's own is written" "$(cat "$(APP).real")" "$STOCK_CFG"
fresh; echo "$OLD_CFG" > "$(APP)"; printf 'name: X\ncommand: Other\ntype: Home\n' > "$(APP).real"; run stock-appcfg
check "a good backup is left alone" "$(sed -n 2p "$(APP).real")" "command: Other"

echo "out of Wampy's folder"
O() { echo "$R/system/vendor/unknown321"; }
old_install() {  # Cinder's files where 0.3.20 put them, the .appcfg already on the new launcher
  mkdir -p "$(O)/bin" "$(O)/lib/preload" "$(O)/usr/share/cinder/audio_dac" "$(O)/rollback"
  for f in cinder-home cinderhome-launch.sh cinder-power cinder-msc cinder-voltable cinder-preload.sh launch-probe.sh; do
    echo old > "$(O)/bin/$f"; done
  echo old > "$(O)/lib/libcinder_mono.so"; echo old > "$(O)/lib/preload/BtCommonService.so"
  echo sony-table > "$(O)/usr/share/cinder/audio_dac/ov_127x.tbl"
  echo "$STOCK_CFG" > "$(O)/HgrmMediaPlayerApp.appcfg.real"
  echo "$NEW_CFG" | sed "s#/system/#$R/system/#" > "$(APP)"
}
fresh; old_install; rmdir "$(O)/rollback"; run migrate
check "only Cinder's files there: the folder is gone" "$([ -e "$(O)" ] && echo there || echo gone)" gone
check "…and the volume table came along" "$(cat "$R/system/vendor/cinder/audio_dac/ov_127x.tbl" 2>/dev/null)" sony-table

fresh; old_install; echo w > "$(O)/bin/wampy"; echo w > "$(O)/lib/libsound_service_fw.so"; run migrate
check "Wampy's files stay" "$(cat "$(O)/bin/wampy" "$(O)/lib/libsound_service_fw.so")" "w
w"
check "…Cinder's go" "$(ls "$(O)/bin")$(ls "$(O)/lib")" "wampylibsound_service_fw.so"
check "…and an empty preload folder goes" "$([ -e "$(O)/lib/preload" ] && echo there || echo gone)" gone
check "…and a folder of anything else stays, said in the log" "$(grep -c 'left there: bin lib rollback' "$R/out")" 1

fresh; old_install; echo "$OLD_CFG" | sed "s#/system/#$R/system/#" > "$(APP)"; run migrate
check "the .appcfg still on the old launcher: nothing removed" "$(ls "$(O)/bin" | wc -l)" 7

echo "the uninstaller"
uninstall() {  # the uninstaller with /system, /data and /contents in the scratch root
  mkdir -p "$R/data" "$R/contents" "$R/xbin"
  sed -e "s#/system/#$R/system/#g" -e "s#/data/#$R/data/#g" -e "s#/contents/#$R/contents/#g" \
      -e "s#^LOG=.*#LOG=$R/log#" -e "s#^BB=/xbin/busybox#BB=$R/xbin/busybox#" "$HERE/deploy/uninstall_cinderhome.sh" > "$T/un.sh"
  printf '#!/bin/sh\ncmd="$1"; shift\nexec "$cmd" "$@"\n' > "$R/xbin/busybox"; chmod +x "$R/xbin/busybox"
  mkdir -p "$T/stub"; printf '#!/bin/sh\nexit 0\n' > "$T/stub/mount"; cp "$T/stub/mount" "$T/stub/umount"
  chmod +x "$T/stub/mount" "$T/stub/umount"
  PATH="$T/stub:$PATH" sh "$T/un.sh" > /dev/null 2>&1
}
fresh; old_install; echo "$OLD_CFG" > "$(APP)"; echo "$OLD_CFG" > "$(APP).real"; echo new > "$R/system/vendor/cinder/bin/cinder-home"
uninstall
check "a backup naming the launcher: Sony's .appcfg is written instead" "$(cat "$(APP)")" "$STOCK_CFG"
check "…readable by appmgr" "$(stat -c %a "$(APP)")" 755
check "…Cinder's folder is gone" "$([ -e "$R/system/vendor/cinder" ] && echo there || echo gone)" gone
check "…and its files in Wampy's folder, leaving what is not Cinder's" "$(ls -A "$(O)")" rollback

echo; echo "migrate: $pass ok, $fail FAIL"
[ "$fail" -eq 0 ]
