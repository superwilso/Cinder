#!/usr/bin/env bash
# check_payload_attrs.sh — every shipped payload file must be exempt from Git's EOL rewriting.
#
#   tools/check_payload_attrs.sh
#
# WHY THIS EXISTS. The release payload is committed under cinder-home/dist/ and verified by sha256
# (tools/release.sh writes the manifest, .github/workflows/release.yml checks it). That check is
# only meaningful if the bytes on the runner are the bytes that were hashed — and for TEXT members
# they were not: `release.yml` builds on `windows-latest`, where Git's default core.autocrlf=true
# rewrites LF→CRLF at checkout for anything it auto-detects as text.
#
# v0.1.7 (run 33643552973) failed exactly there. Every binary member passed; the two text members —
# cinder-signature.sh and cinder_components.conf — failed, because they had been rewritten in
# transit. Nothing was stale. `.gitattributes` now marks cinder-home/dist/** as `-text`, and this
# script is what stops that protection from silently lapsing when a new payload member is added
# outside the covered paths.
#
# AND THE STAKES ARE NOT ONLY CI: cinder-signature.sh is executed ON THE DEVICE. A CRLF shebang is
# `#!/bin/sh\r`, so the kernel looks for an interpreter literally named "sh\r" and the script dies
# with a "not found" naming a binary that is plainly there.
#
# WHY A SCRIPT AND NOT INLINE YAML — the rule this repo already learned twice (the shellcheck gate,
# and then this very failure): a check CI runs and a contributor cannot is a check that fails for
# the first time during a release. That is precisely how the manifest gate behaved. Run it locally;
# ci.yml calls this exact file.
#
# NOTE ON SCOPE. This deliberately does NOT re-verify the sha256 manifest. Between releases dist/
# legitimately moves ahead of the manifest (a payload rebuild is its own commit), so hashing here
# would be red during ordinary work and would train people to ignore it. Line-ending exposure, by
# contrast, is always wrong.
set -uo pipefail
cd "$(dirname "$0")/.." || { echo "cannot reach the repo root" >&2; exit 2; }

fail=0
checked=0

# Every tracked file under the payload tree, plus the device-side scripts that share the shebang
# hazard. `git ls-files` rather than `find`: an untracked local artefact is not shipped and is not
# this script's business.
mapfile -t FILES < <(git ls-files -- 'cinder-home/dist/**' 'cinder-home/deploy/*.sh' 'player/deploy/*.sh' 2>/dev/null)

if [ "${#FILES[@]}" -eq 0 ]; then
    echo "  NOTE  no payload files tracked yet — nothing to check"
    exit 0
fi

echo "── payload line-ending exposure ────────────────────────────────────"
for f in "${FILES[@]}"; do
    checked=$((checked + 1))
    attr="$(git check-attr text -- "$f" | sed 's/.*: text: //')"
    # "unset" is `-text` — conversion disabled in both directions, which is what we require.
    # "unspecified" means Git will AUTO-DETECT, and auto-detection is the whole problem.
    if [ "$attr" != "unset" ]; then
        echo "  FAIL  $f"
        echo "        text attribute is '$attr'; needs '-text' in .gitattributes"
        fail=1
    fi
    # Belt and braces: a CRLF already committed would survive any attribute setting.
    #
    # TEXT BLOBS ONLY. 0x0D is ordinary data inside an ELF or a .UPG, so testing every member for
    # a bare CR flagged five binaries on the first run — cinder-battery, cinder-clock,
    # cinder-power, cinder-umount and cinder_home_uninstall.upg — none of which had anything wrong
    # with them. A blob containing NUL is binary; skip it, and look for the CRLF PAIR rather than a
    # loose CR in the rest.
    # `grep -I` IS the binary test: with it, a binary file is treated as non-matching, so an empty
    # pattern matches every text file and no binary one. The first attempt tested for a NUL byte
    # with `grep -q $'\x00'`, which cannot work — bash cannot hold NUL in a variable, so that
    # expanded to the empty pattern and the test meant nothing.
    if git show "HEAD:$f" 2>/dev/null | LC_ALL=C grep -qI ''; then
        if git show "HEAD:$f" 2>/dev/null | LC_ALL=C grep -q $'\r$'; then
            echo "  FAIL  $f"
            echo "        the committed blob has CRLF endings — re-commit it with LF"
            fail=1
        fi
    fi
done

if [ "$fail" -eq 0 ]; then
    echo "  ok    $checked payload files are -text and stored with LF"
    echo "── all clear ───────────────────────────────────────────────────────"
else
    echo "── FAILURES above ──────────────────────────────────────────────────"
fi
exit "$fail"
