#!/usr/bin/env bash
# render_release_notes.sh — build the GitHub release body, with the real checksums inlined.
#
#   tools/render_release_notes.sh <SHA256SUMS file> [output]     write the body (default stdout)
#   tools/render_release_notes.sh --preview                      render against the local dist/
#
# WHY THIS EXISTS. The release body used to be an inline `body:` block in release.yml, and the
# checksums were only ever written to an attached SHA256SUMS file. The workflow carried a comment
# claiming "the sums go in the release body so a download can be checked without trusting the
# download itself" — above a step that did no such thing. Inline YAML cannot hold anything
# computed, so the comment described an intention the format could not express, and nobody noticed
# because a release body is invisible until it is published.
#
# Attaching the sums beside the binaries is also weak on its own: whoever could replace one file
# could replace the other. Putting them in the release page makes them part of the record GitHub
# renders from the tag.
#
# CI runs THIS FILE (release.yml, "Compose the release notes"), so --preview shows exactly what
# will be published. That is the same rule tools/shell_check.sh is under: a check — or a document
# — that only ever runs during a release is one that fails for the first time during a release.
set -uo pipefail
cd "$(dirname "$0")/.." || { echo "cannot reach the repo root" >&2; exit 2; }

TEMPLATE=".github/release-notes.md"
MARKER="{{SHA256SUMS}}"

if [ "${1:-}" = "--preview" ]; then
    SUMS="$(mktemp)"
    trap 'rm -f "$SUMS"' EXIT
    if [ -d cinder-home/dist/stable ]; then
        ( cd cinder-home/dist/stable && sha256sum -- * ) > "$SUMS" 2>/dev/null
    fi
    if [ ! -s "$SUMS" ]; then
        printf '%s  (nothing built yet — this is a placeholder)\n' \
            "0000000000000000000000000000000000000000000000000000000000000000" > "$SUMS"
    fi
    OUT="-"
else
    SUMS="${1:-}"
    OUT="${2:--}"
fi

[ -n "$SUMS" ] || { echo "usage: tools/render_release_notes.sh <SHA256SUMS> [out] | --preview" >&2; exit 2; }
[ -f "$TEMPLATE" ] || { echo "missing template: $TEMPLATE" >&2; exit 2; }
[ -f "$SUMS" ]     || { echo "missing checksums: $SUMS" >&2; exit 2; }
[ -s "$SUMS" ]     || { echo "checksum file is empty: $SUMS" >&2; exit 1; }

# FAIL rather than publish a release whose verification section is blank. A body with an empty
# code block under "Verifying the download" is worse than no section at all — it looks like the
# check was done.
grep -qF "$MARKER" "$TEMPLATE" || {
    echo "template $TEMPLATE no longer contains the $MARKER line — nothing to substitute" >&2
    exit 1
}

render() {
    # Drop the HTML comment header: it is guidance for whoever edits the template, not for the
    # people reading the release. Everything from the first '## ' onward is the body.
    awk -v sums="$SUMS" -v marker="$MARKER" '
        !started && /^## / { started = 1 }
        !started { next }
        index($0, marker) {
            while ((getline line < sums) > 0) print line
            close(sums)
            next
        }
        { print }
    ' "$TEMPLATE"
}

if [ "$OUT" = "-" ]; then
    render
else
    render > "$OUT" || exit 1
    echo "wrote $OUT ($(wc -l < "$OUT") lines, $(grep -c '^[0-9a-f]\{64\}  ' "$OUT") checksums)"
fi
