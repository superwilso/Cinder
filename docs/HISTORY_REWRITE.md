# Rewriting the repository's history

*Prepared and rehearsed 2026-09-11. **Not pushed.** Pushing is the owner's decision, and it cannot
be undone.*

## Why

The repository is public, and until 2026-09-11 its tree carried files that are Sony's:

- **980 files extracted from Sony's player** (`analysis/ui_assets/`): 796 images, 180 QML screens,
  the English labels and a gallery page over them.
- **About 11,000 lines of Ghidra decompilation** of Sony libraries (`analysis/F_appmgr_home/*.c`,
  `analysis/G_player_ipc/player.c`, `analysis/G_bt_nfc/decomp_BtCommonServiceClient.txt`).
- In history only: **a device screenshot showing a real album cover**
  (`cinder_screen_20260826_113143.png`).

They are gone from the working tree, and the findings drawn from them stay. But a commit that deletes
a file leaves every earlier commit exactly as it was, one click away on GitHub — which is where a
takedown notice would point, and a notice against a repository can take all of it down, not one
file. Posting about the project is what makes that likely, so the time to decide is before posting.

The same rewrite drops superseded ARM build output — old revisions of `cinder-home/dist/`, the dev
channel, and the `.unstripped` debug binaries — while keeping the payload that `main` and every
release tag point at.

## What the rehearsal showed

`tools/rewrite_history.sh` against a fresh mirror of GitHub, 2026-09-11, 16 seconds:

| | Before | After |
|---|---|---|
| Packed size | 122.7 MB | **35.2 MB** |
| Commits, all refs | 168 | 165 — three touched nothing but removed files |
| Files at the tip of `main` | 1,811 | 824, **byte-identical** to before; the 987 removed are exactly the Sony files |
| `dist/stable` at each of the 10 release tags | | **Unchanged**, so `tools/verify_payload_manifest.sh` still passes on every tag |
| Superseded `dist/` blobs | | 149 stripped, 52 kept |
| Commit ids quoted in the docs | | 15 remapped across 8 files, written as a patch (step 6). Two (`3b67cd4`, `aefbca6`) name commits that no longer exist and are left as written |

The script checks each of those itself and stops if any of them fails.

> On size: the audits recorded a 1.3 GB `.git`. That was one working copy holding 1.09 GB of loose
> objects `git gc` had never packed — a clone from GitHub has been about 124 MB all along. The
> rewrite takes it to about 35 MB.

## What it does not remove: Sony's updater

`installer/sony-updater/` is **Sony's own Windows firmware updater** — `SoftwareUpdateTool.exe` and
`WmFwUpdater.dll`, with Microsoft's Visual C++ 2010 runtime beside them — embedded in every Windows
installer to send the player the command that starts its update. The rewrite leaves it alone on
purpose: removing it from history while the installer still needs it would break the release build.

It is the same exposure as the files above, and a bigger one, because it ships inside a release
download and not only the repository. Removing it means replacing it. On Linux the installer already
sends the same 12-byte vendor SCSI command itself (`installer/src/stage.rs`); Windows can do the same
through SCSI pass-through (`DeviceIoControl` with `IOCTL_SCSI_PASS_THROUGH_DIRECT`), which needs
administrator rights the installer does not ask for today, and which has to be proven on the device.
When that lands, add `installer/sony-updater/` to `REMOVE_PATHS` in the script, so both go in one
rewrite rather than two.

## Their own repository

Taking the files out of Cinder does not have to mean deleting them. They can move to a repository of
their own, history included, so that a takedown notice aimed at them has somewhere to point that is
not Cinder. The list is [`tools/sony_paths.txt`](../tools/sony_paths.txt), and the rewrite script
reads the same file, so what moves and what the rewrite removes cannot drift apart. The screenshot
and the old build output the rewrite also drops are not Sony's, and go nowhere.

Rehearsed 2026-09-11 from GitHub: **987 files, blob-identical to the Sony files at the tip of
`main`**, 3 commits, 3.4 MB packed, no tags, no remote left behind.

Build it **before the rewrite is pushed**: it is cut from the history the rewrite replaces. After the
push, clone `../cinder-before-rewrite.bundle` (step 2 below) instead of GitHub.

```sh
# git-filter-repo: the same pinned release the rewrite script checks
curl -fsSLo ~/git-filter-repo https://raw.githubusercontent.com/newren/git-filter-repo/v2.47.0/git-filter-repo
echo "67447413e273fc76809289111748870b6f6072f08b17efe94863a92d810b7d94  $HOME/git-filter-repo" | sha256sum -c -

# A fresh copy of Cinder, cut down to Sony's files and the commits that touched them
git clone --single-branch --no-tags https://github.com/superwilso/Cinder.git ~/cinder-sony-analysis
cd ~/cinder-sony-analysis
python3 ~/git-filter-repo --paths-from-file ~/sony/tools/sony_paths.txt
git ls-files | wc -l        # 987

# Say what it is, then publish
cat > README.md <<'EOF'
# Cinder — Sony reference files

Files taken from Sony's NW-A50-series Walkman firmware while building
[Cinder](https://github.com/superwilso/Cinder), a replacement music player for that device:

- `analysis/ui_assets/` — the images, QML screens and English labels of Sony's music player
  (HgrmMediaPlayerApp), with an index and a gallery page over them.
- `analysis/F_appmgr_home/*.c`, `analysis/G_player_ipc/player.c` and
  `analysis/G_bt_nfc/decomp_BtCommonServiceClient.txt` — Ghidra decompilations of Sony libraries,
  made to learn how to talk to the firmware's services.

They moved here from Cinder's repository in September 2026. What was learned from them is written up
in Cinder's own documentation; Cinder does not need these files to build or run.

These files are Sony's. Cinder's MIT license does not cover them and no license is granted for them
here. They are kept for interoperability research.
EOF
git add README.md
git commit -m "Say what these files are and whose they are"
gh repo create superwilso/cinder-sony-analysis --public --source . --remote origin --push
```

**This separates repositories, not accounts.** GitHub records a takedown against the account that
owns the repository, and closes the accounts of repeat infringers — which would take Cinder with it.
Owning the Sony repository from a different GitHub account is what actually keeps the two apart.

## What pushing breaks

- **Every commit id changes**, on every branch and tag. Links to commits, and hashes quoted in issues,
  pull requests and messages, stop resolving.
- **Every existing clone is on the old history.** Re-clone after the push. Pulling or pushing from an
  old clone brings the old history back.
- **GitHub keeps the old commits** behind references nobody can push to — `refs/pull/*` for the ten
  pull requests — and in its caches, so old commit URLs keep working until GitHub removes them. Only
  GitHub Support can do that; GitHub's documentation on removing sensitive data describes the
  request.
- **Release tags move** to the rewritten commits. Their payloads are unchanged and release downloads
  are stored separately, so the assets stay as they are — but the `release` workflow runs on every
  pushed tag, so it has to be switched off for the push, or it rebuilds all ten releases.
- **Branch protection refuses it.** `main` is protected with force pushes disabled (checked
  2026-09-11), so the push is rejected until that is changed for the duration.
- **Forks:** none existed on 2026-09-11. Stars are unaffected.

## The order

0. **Move Sony's files to their own repository** ([above](#their-own-repository)), while GitHub's
   history still holds them. Optional — skip it and they are simply gone from Cinder.
1. **Commit the tree changes first, as an ordinary commit and push**: the deleted Sony files, the
   `.gitignore` rules that keep them out, and the docs that now point elsewhere. That alone takes
   them off the tip, is reversible, and is worth doing even if nothing below ever happens. A local
   copy of everything removed is in `artifacts/sony_extracted/`, which git ignores.
2. **Back up the repository as it is**, outside the clone:
   `git bundle create ../cinder-before-rewrite.bundle --all` holds the entire old history in one file.
3. **Optionally delete the merged `claude/*` branches** on GitHub. Nothing on them is not on `main`.
4. **Run the script**: `tools/rewrite_history.sh` (or pass a directory). It mirrors GitHub, rewrites,
   checks, writes the doc patch, and prints the push commands. It pushes nothing.
5. **Push — the step that cannot be undone.** In GitHub, Settings ▸ Branches ▸ the rule for `main`:
   allow force pushes. Then:

   ```sh
   gh workflow disable release
   git -C ../cinder-rewrite/Cinder.git push --force https://github.com/superwilso/Cinder.git \
       'refs/heads/*:refs/heads/*' 'refs/tags/*:refs/tags/*'
   gh workflow enable release
   ```

   and turn force pushes off again.
6. **Re-clone**, apply the commit-reference patch — `git apply ../cinder-rewrite/doc-commit-refs.patch`
   — and commit it the usual way.
7. **Ask GitHub Support** to drop cached views and the pull-request references to the old commits.
8. **Retire the old working copy** once the new clone builds. If it has to stay a while,
   `git gc --prune=now` gives back the gigabyte of loose objects.
