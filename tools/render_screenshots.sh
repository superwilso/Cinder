#!/usr/bin/env bash
# render_screenshots.sh — regenerate the README's screenshots from the UI as it is now.
#
#   tools/render_screenshots.sh            render every preview, copy the README's into docs/screenshots
#   tools/render_screenshots.sh --check    render and compare only; exit 1 if any README image is stale
#
# WHY. The README's images are previews cinder-host renders — the same frames golden.txt hashes —
# copied into docs/screenshots by hand, and by 2026-09-14 several showed a UI two releases old.
# Nothing ever re-copied them, because nothing knew which preview each image was. The map below is
# that knowledge (recovered by matching every image to a preview byte for byte), and
# tools/release.sh runs this before every tag so the README describes the release it ships with.
#
# hero.png is not a preview — it is a composed 1980x1636 image — and nothing links to it, so it is
# left alone. Exit codes: 0 up to date or updated, 1 stale (--check), 2 could not render, or a
# preview in the map no longer exists (renamed in cinder-host: fix the map).
set -uo pipefail
cd "$(dirname "$0")/.." || { echo "cannot reach the repo root" >&2; exit 2; }

CHECK=""
[ "${1:-}" = "--check" ] && CHECK=1

# docs/screenshots/<name>.png  <-  player/out/<preview>.png
MAP=(
    now-playing:now_playing_day            now-playing-night:now_playing_night
    up-next:up_next_day                    up-next-queue:up_next_queue_day
    up-next-reorder:up_next_reorder_day    volume:overlay_volume
    library-albums:library_albums_day      library-albums-night:library_albums_night
    library-songs:library_songs_day        library-artists:library_artists_day
    library-playlists:library_playlists_day album:album_drill
    artist:artist_day                      folders:folders_dir_day
    track-info:track_info_day              equalizer:eq_day
    sound:sound_day                        bluetooth:bluetooth_day
    usb-dac:usbdac_day                     settings:settings_day
    shelf:shelf_day                        lock:lock_day
    fm-radio:fm_day                        gestures:onboard_2_gestures_day
    visualiser-bars:np_spectrum_0_bars     visualiser-ribbon:np_spectrum_1_ribbon
)

# A clean render, with the real-cover overrides unset: those read files outside the tree, and the
# README must show what anyone gets from this commit.
rm -rf player/out
if ! ( cd player && env -u CINDER_PREVIEW_T48 -u CINDER_PREVIEW_T96 \
        cargo run --release -q -p cinder-host >/dev/null ); then
    echo "cinder-host failed to render the previews" >&2
    exit 2
fi

stale=0; updated=0; missing=0
for pair in "${MAP[@]}"; do
    name="${pair%%:*}"; preview="${pair#*:}"
    src="player/out/$preview.png"; dst="docs/screenshots/$name.png"
    if [ ! -f "$src" ]; then
        printf '  MISSING  %-24s preview %s no longer exists — update the map\n' "$name" "$preview"
        missing=1; continue
    fi
    if [ -f "$dst" ] && cmp -s "$src" "$dst"; then
        printf '  ok       %s\n' "$name"
        continue
    fi
    if [ -n "$CHECK" ]; then
        printf '  STALE    %-24s differs from %s\n' "$name" "$preview"; stale=$((stale+1))
    else
        cp "$src" "$dst"
        printf '  updated  %-24s from %s\n' "$name" "$preview"; updated=$((updated+1))
    fi
done

[ "$missing" = 0 ] || exit 2
if [ -n "$CHECK" ]; then
    [ "$stale" = 0 ] && { echo "README screenshots match the current UI."; exit 0; }
    echo "$stale README screenshot(s) are stale — run tools/render_screenshots.sh"
    exit 1
fi
echo "README screenshots: $updated updated, $(( ${#MAP[@]} - updated )) already current."
