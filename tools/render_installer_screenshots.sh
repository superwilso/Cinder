#!/usr/bin/env bash
# render_installer_screenshots.sh — the README's pictures of the installer, from the real page.
#
#   tools/render_installer_screenshots.sh     build, render, copy into docs/screenshots
#
# The installer's window is a page it serves to the browser (installer/src/web.rs), so the pictures
# are that page in a headless Chrome or Edge, pointed at made-up players: CINDER_PLAYER_ROOTS makes
# the installer look only in the folders given, and --dry-run means nothing is written even there.
# On WSL it uses the Windows Chrome or Edge through interop; elsewhere a chromium on the PATH.
#
# NO --check, unlike render_screenshots.sh: the pixels depend on the browser and its fonts, so two
# machines never agree byte for byte.
#
# Exit codes: 0 rendered, 2 could not build or render, 3 no headless browser found.
set -uo pipefail
cd "$(dirname "$0")/.." || { echo "cannot reach the repo root" >&2; exit 2; }

BROWSER_EXE=""
for b in "/mnt/c/Program Files/Google/Chrome/Application/chrome.exe" \
         "/mnt/c/Program Files (x86)/Microsoft/Edge/Application/msedge.exe" \
         "/mnt/c/Program Files/Microsoft/Edge/Application/msedge.exe" \
         "$(command -v chromium 2>/dev/null)" "$(command -v chromium-browser 2>/dev/null)" \
         "$(command -v google-chrome 2>/dev/null)"; do
    [ -n "$b" ] && [ -x "$b" ] && { BROWSER_EXE="$b"; break; }
done
[ -n "$BROWSER_EXE" ] || { echo "needs a headless Chrome, Edge or Chromium" >&2; exit 3; }
win() { if [[ "$BROWSER_EXE" == /mnt/* ]]; then wslpath -w "$1"; else echo "$1"; fi; }

( cd installer && cargo build --quiet --release ) || { echo "could not build the installer" >&2; exit 2; }
EXE="installer/target/release/cinder-installer"

WORK="$(mktemp -d)"
SRV=""
trap '[ -n "$SRV" ] && kill "$SRV" 2>/dev/null; rm -rf "$WORK"' EXIT

# One player on 0.4.1-rc1 with a few options, as most people updating will have.
P="$WORK/player"
mkdir -p "$P/MUSIC" "$P/PC_Application"
printf 'version=0.4.1-rc1\nchannel=stable\n' > "$P/cinder_running.txt"
printf '== cinder-home installer  Mon Nov  3 17:54:38 UTC 2026\n== done. reboot to normal\n' > "$P/cinder_home_install.log"
printf '# channel: stable\nCINDER_SEARCH=1\nCINDER_LANG_EN=1\n' > "$P/cinder_components.conf"

CINDER_NO_BROWSER=1 CINDER_PLAYER_ROOTS="$P" "$EXE" --gui --dry-run > "$WORK/out" 2>&1 &
SRV=$!
for _ in $(seq 1 50); do grep -q 'http://127' "$WORK/out" && break; sleep 0.1; done
URL="$(grep -o 'http://127[^ ]*' "$WORK/out" | head -1)"
[ -n "$URL" ] || { echo "the installer did not start its page" >&2; cat "$WORK/out" >&2; exit 2; }
# WSL hands a new listening port to Windows' 127.0.0.1 a moment after it opens.
sleep 2

mkdir -p docs/screenshots
shot() {  # shot NAME #SCREEN HEIGHT
    local png="$WORK/$1.png"
    "$BROWSER_EXE" --headless=new --disable-gpu --hide-scrollbars --user-data-dir="$(win "$WORK/profile")" \
        --window-size=1200,"$3" --virtual-time-budget=6000 --screenshot="$(win "$png")" "$URL$2" >"$WORK/browser.log" 2>&1
    [ -s "$png" ] || { echo "could not capture $1" >&2; tail -5 "$WORK/browser.log" >&2; exit 2; }
    cp "$png" "docs/screenshots/$1.png"
    echo "  docs/screenshots/$1.png"
}
shot installer-home "#home" 820
shot installer-options "#options" 1400
shot installer-confirm "#review" 1100
shot installer-fix "#fix" 1000
# The window's pages before the page replaced it; nothing shows them now.
rm -f docs/screenshots/installer-start.png docs/screenshots/installer-options-library.png
