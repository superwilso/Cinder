#!/usr/bin/env bash
# Rewrite the repository's history to take out what should never have been published.
#
#   tools/rewrite_history.sh [WORKDIR]        (default: ../cinder-rewrite, next to this clone)
#
# From EVERY commit it removes:
#   * Sony's own files: the UI images, QML and English labels extracted from HgrmMediaPlayerApp
#     (analysis/ui_assets/), and the Ghidra decompilations of Sony libraries (analysis/F_*, G_*).
#     Deleting them in a new commit hides them from the tip and leaves every one of them a click
#     away in the history, which is exactly where a takedown notice would point.
#   * A device screenshot that shows a real album cover.
#   * Superseded ARM build output: old revisions of cinder-home/dist/, the dev channel, and the
#     .unstripped debug binaries. The payload that main and each release tag point at is KEPT, so
#     tools/verify_payload_manifest.sh still passes on every tag.
#
# It works on a fresh MIRROR of the remote in WORKDIR. It never touches this clone and it NEVER
# PUSHES: at the end it prints the commands that would, and docs/HISTORY_REWRITE.md says what
# pushing costs and the order to do things in.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${1:-$(dirname "$ROOT")/cinder-rewrite}"
REMOTE="${CINDER_REWRITE_REMOTE:-$(git -C "$ROOT" remote get-url origin)}"

# git-filter-repo is one Python file. If it is not installed, fetch this exact release and check it
# before running anything with it.
FILTER_REPO_VERSION=v2.47.0
FILTER_REPO_SHA256=67447413e273fc76809289111748870b6f6072f08b17efe94863a92d810b7d94

DIST=cinder-home/dist/

die() { printf '\033[1;31m✗ %s\033[0m\n' "$*" >&2; exit 1; }
say() { printf '\033[1m── %s\033[0m\n' "$*"; }

# Removed from every commit. A trailing / means the whole directory. Sony's files come from
# tools/sony_paths.txt — the same list their own repository is cut from, so the two always match.
SONY_PATHS="$ROOT/tools/sony_paths.txt"
[ -r "$SONY_PATHS" ] || die "$SONY_PATHS is missing"
mapfile -t REMOVE_PATHS < <(grep -v -e '^#' -e '^[[:space:]]*$' "$SONY_PATHS")
[ "${#REMOVE_PATHS[@]}" -gt 0 ] || die "$SONY_PATHS lists no paths"
REMOVE_PATHS+=(
    # Sony's own Windows firmware updater — SoftwareUpdateTool.exe, Sony's WmFwUpdater.dll and
    # Microsoft's Visual C++ 2010 runtime — embedded in every Windows installer until 0.3.1 for
    # one vendor SCSI command the installer now sends itself (installer/src/stage.rs). It shipped
    # inside release downloads as well as living here, which made it the larger exposure of the two.
    installer/sony-updater/
    cinder_screen_20260826_113143.png
    cinder-home/cinder-home.unstripped
    cinder-home/cinder-probe.unstripped
    cinder-home/dist/dev/
)

command -v python3 >/dev/null || die "python3 is needed: git-filter-repo is a Python script"
[ ! -e "$WORK" ] || die "$WORK already exists — remove it, or pass another directory"
mkdir -p "$WORK"
WORK="$(cd "$WORK" && pwd)"
M="$WORK/Cinder.git"

if git filter-repo --version >/dev/null 2>&1; then
    FILTER=(git filter-repo)
else
    say "fetching git-filter-repo $FILTER_REPO_VERSION"
    curl -fsSL -o "$WORK/git-filter-repo" \
        "https://raw.githubusercontent.com/newren/git-filter-repo/$FILTER_REPO_VERSION/git-filter-repo"
    echo "$FILTER_REPO_SHA256  $WORK/git-filter-repo" | sha256sum -c --quiet - \
        || die "git-filter-repo does not match its pinned checksum — not running it"
    FILTER=(python3 "$WORK/git-filter-repo")
fi

say "mirroring $REMOTE"
git clone --quiet --mirror "$REMOTE" "$M"
# A mirror brings GitHub's refs/pull/* with it, and GitHub refuses pushes to those. Drop them so
# nothing rewritten here looks pushable. GitHub itself still holds the old commits behind them —
# see "After pushing" in docs/HISTORY_REWRITE.md.
git -C "$M" for-each-ref --format='delete %(refname)' refs/pull | git -C "$M" update-ref --stdin
git -C "$M" gc --quiet --prune=now

size_mb() { git -C "$M" count-objects -v | awk '$1 == "size-pack:" { printf "%.1f", $2 / 1024 }'; }
before_mb="$(size_mb)"
before_commits="$(git -C "$M" rev-list --all --count)"
mapfile -t TAGS < <(git -C "$M" tag)
git -C "$M" ls-tree -r main > "$WORK/main-before.txt"
for t in "${TAGS[@]}"; do git -C "$M" ls-tree -r "$t" -- "${DIST}stable"; done > "$WORK/tags-dist-before.txt"

say "listing superseded build output"
# Every blob ever added under dist/, from the raw diff of every commit (the root commit included).
raw() { git -C "$M" log --all --root --format= --raw --no-abbrev --no-renames; }
raw | awk -F'\t' -v d="$DIST" '{ split($1, f, " ") } index($2, d) == 1 && f[4] !~ /^0+$/ { print f[4] }' \
    | sort -u > "$WORK/dist-blobs.txt"
# …minus what main and every release tag still point at, which stays…
for ref in main "${TAGS[@]}"; do git -C "$M" ls-tree -r "$ref" -- "$DIST"; done \
    | awk '{ print $3 }' | sort -u > "$WORK/dist-keep.txt"
# …minus anything that ALSO lived outside dist/ at some point. Stripping works by blob id, so a file
# copied into dist/ unchanged would otherwise disappear from its real home as well.
raw | awk -F'\t' -v d="$DIST" '{ split($1, f, " ") } index($2, d) != 1 && f[4] !~ /^0+$/ { print f[4] }' \
    | sort -u > "$WORK/elsewhere.txt"
comm -23 "$WORK/dist-blobs.txt" "$WORK/dist-keep.txt" | comm -23 - "$WORK/elsewhere.txt" > "$WORK/strip-blobs.txt"
echo "   $(wc -l < "$WORK/strip-blobs.txt") superseded dist/ blobs to strip, $(wc -l < "$WORK/dist-keep.txt") kept"

say "rewriting"
args=(--force --invert-paths)
for p in "${REMOVE_PATHS[@]}"; do args+=(--path "$p"); done
args+=(--strip-blobs-with-ids "$WORK/strip-blobs.txt")
if ! (cd "$M" && "${FILTER[@]}" "${args[@]}") > "$WORK/filter-repo.log" 2>&1; then
    tail -20 "$WORK/filter-repo.log" >&2
    die "git-filter-repo failed — full log: $WORK/filter-repo.log"
fi

say "checking the result"
git -C "$M" ls-tree -r main > "$WORK/main-after.txt"
# The new tip must be the old tip minus exactly the removed paths: same files, modes and blob ids.
python3 - "$WORK/main-before.txt" "$WORK/main-after.txt" "${REMOVE_PATHS[@]}" <<'PY'
import sys
before, after, *removed = sys.argv[1:]
def rows(path):
    out = {}
    for line in open(path, encoding="utf-8").read().splitlines():
        meta, name = line.split("\t", 1)
        out[name] = meta
    return out
def gone(name):
    return any(name == r or (r.endswith("/") and name.startswith(r)) for r in removed)
b, a = rows(before), rows(after)
want = {k: v for k, v in b.items() if not gone(k)}
if want != a:
    print("the tip differs beyond the removed paths:")
    print("  missing:", sorted(set(want) - set(a))[:10])
    print("  extra:  ", sorted(set(a) - set(want))[:10])
    print("  changed:", sorted(k for k in set(a) & set(want) if a[k] != want[k])[:10])
    sys.exit(1)
print(f"   tip: {len(b) - len(a)} files removed; the other {len(a)} are byte-identical")
PY
for t in "${TAGS[@]}"; do git -C "$M" ls-tree -r "$t" -- "${DIST}stable"; done > "$WORK/tags-dist-after.txt"
cmp -s "$WORK/tags-dist-before.txt" "$WORK/tags-dist-after.txt" \
    || die "a release tag's dist/stable changed — it must not"
echo "   ${#TAGS[@]} release tags: dist/stable unchanged"
# Nothing under a removed path may survive in ANY commit.
git -C "$M" log --all --format= --name-only | sort -u \
    | python3 -c '
import sys
removed = sys.argv[1:]
left = [l for l in sys.stdin.read().splitlines()
        if l and any(l == r or (r.endswith("/") and l.startswith(r)) for r in removed)]
if left:
    print("still in history:", left[:10]); sys.exit(1)
print("   removed paths: absent from every commit")
' "${REMOVE_PATHS[@]}"
survivors=0
while read -r oid; do
    if git -C "$M" cat-file -e "$oid" 2>/dev/null; then survivors=$((survivors + 1)); fi
done < "$WORK/strip-blobs.txt"
[ "$survivors" -eq 0 ] || die "$survivors stripped blobs are still in the object store"
echo "   stripped blobs: gone from the object store"

say "commit references in the docs"
# Docs quote commit ids as evidence, deliberately and often; every one of them changes. Map each to
# its rewritten id and leave the edit as a patch to commit AFTER the push, from a fresh clone.
MAP="$M/filter-repo/commit-map"
git -C "$M" worktree add --quiet "$WORK/tip" main
python3 - "$MAP" "$WORK/tip" <<'PY'
import bisect, pathlib, re, subprocess, sys
cmap, tip = sys.argv[1], pathlib.Path(sys.argv[2])
old2new = dict(line.split() for line in open(cmap).read().splitlines()[1:])
olds = sorted(old2new)
def resolve(short):
    i = bisect.bisect_left(olds, short)
    hits = []
    while i < len(olds) and olds[i].startswith(short):
        hits.append(olds[i])
        i += 1
    return hits[0] if len(hits) == 1 else None
pat = re.compile(r"(?<![0-9a-fA-F])[0-9a-f]{7,40}(?![0-9a-fA-F])")
suffixes = (".md", ".rs", ".cpp", ".hpp", ".h", ".c", ".sh", ".yml", ".toml", ".txt")
files = subprocess.run(["git", "-C", str(tip), "ls-files"], capture_output=True, text=True, check=True).stdout.split("\n")
stats = {"refs": 0, "files": 0}
vanished = set()
for f in files:
    if not f.endswith(suffixes):
        continue
    path = tip / f
    try:
        text = path.read_text(encoding="utf-8")
    except (UnicodeDecodeError, OSError):
        continue
    def sub(m):
        short = m.group(0)
        full = resolve(short)
        if full is None:
            return short
        new = old2new[full]
        if set(new) == {"0"}:          # the commit only touched removed files, so it is gone
            vanished.add(short)
            return short
        stats["refs"] += 1
        return new[: len(short)]
    out = pat.sub(sub, text)
    if out != text:
        path.write_text(out, encoding="utf-8")
        stats["files"] += 1
print(f"   {stats['refs']} commit references updated in {stats['files']} files")
if vanished:
    print(f"   {len(vanished)} name commits that no longer exist and were left as they are: {' '.join(sorted(vanished))}")
PY
git -C "$WORK/tip" diff > "$WORK/doc-commit-refs.patch"
git -C "$M" worktree remove --force "$WORK/tip"

after_mb="$(size_mb)"
after_commits="$(git -C "$M" rev-list --all --count)"
cat <<EOF

Rewritten mirror:  $M
  packed size      $before_mb MB -> $after_mb MB
  commits          $before_commits -> $after_commits
  doc patch        $WORK/doc-commit-refs.patch
  filter log       $WORK/filter-repo.log

NOTHING HAS BEEN PUSHED. Publishing this replaces every commit id in the repository and cannot be
undone. Read docs/HISTORY_REWRITE.md first. main is protected: allow force pushes on its branch rule
for the push, and turn that off again afterwards. The push itself is:

  gh workflow disable release     # re-pushed tags would otherwise re-run every release build
  git -C "$M" push --force "$REMOTE" 'refs/heads/*:refs/heads/*' 'refs/tags/*:refs/tags/*'
  gh workflow enable release
EOF
