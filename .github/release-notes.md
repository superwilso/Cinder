<!--
  The GitHub release body. Rendered by .github/workflows/release.yml, which replaces the
  {{SHA256SUMS}} line below with the real checksums of the files it is about to publish.

  WHY THIS IS A FILE AND NOT A `body:` BLOCK IN THE WORKFLOW. It used to be inline YAML, and
  inline YAML cannot contain anything computed — which is how the workflow ended up carrying the
  comment "the sums go in the release body so a download can be checked without trusting the
  download itself" above a step that only ever wrote them to an attached file. Nobody could see
  the mismatch because nobody reads a release body until it is published. As a file it can be
  read, diffed and previewed locally:

      tools/preview_release_notes.sh

  Keep the {{SHA256SUMS}} line exactly as it is, on a line of its own. The workflow FAILS if it
  cannot find it, rather than publishing a release whose checksum section is silently empty.
-->
## What changed

See [CHANGELOG.md](../blob/main/CHANGELOG.md) for the curated entry for this version,
including which items were verified on hardware and which are still device-gated. The
auto-generated commit list follows below.

## Installing

Download **cinder-installer-windows-x64.exe**, connect the Walkman in USB mass-storage
mode, and run it. It finds the player, stages every Cinder binary plus `NW_WM_FW.UPG`,
then launches Sony's native updater. Follow that updater's prompts; it performs the
required USB handoff and reboots the Walkman into Cinder. No WSL, usbipd, or manual
SCSI command is required.

### Linux

`cinder-installer-linux-x64` does the whole job — **run it with `sudo`**. It stages the
files and then sends the upgrade command itself: the same 12-byte vendor SCSI command
Sony's tool ends with. Root is needed because that is a raw SCSI passthrough. Without
it, the files are still staged correctly and the installer says what did not happen.

### macOS

The installer stages the files but cannot finish. The upgrade command is a vendor SCSI
passthrough, and macOS only exposes those through an IOKit `SCSITaskUserClient`, which
the kernel refuses for an already-mounted disk. Finish from Linux or Windows; see
[install.md](../blob/main/install.md).

> **The player has no update option in its own menus.** This generation never had one —
> the upgrade is always triggered by the host over USB.

`cinder-home-install.upg` and `cinder-home-uninstall.upg` are attached for that route
and for advanced recovery use. On Windows, the one-click executable is the recommended
path — it does the handoff for you.

**Read [RECOVERY.md](../blob/main/RECOVERY.md) first.** This device has no public
DFU/EDL recovery path.

## Verifying the download

GitHub shows its own `sha256:` digest beside each file in the assets list above. That is computed
by GitHub when the file is uploaded, and it proves your download matches what GitHub stores.

The sums below are a different link in the same chain: they are computed on the build runner,
before upload, so they say what was actually built. They should agree with GitHub's digests — if
they ever do not, something happened between the build and the release, and that is worth knowing.

```
{{SHA256SUMS}}
```

On Linux or macOS, save that block as `SHA256SUMS` next to the downloads and run:

```bash
sha256sum -c SHA256SUMS
```

On Windows:

```powershell
Get-FileHash .\cinder-installer-windows-x64.exe -Algorithm SHA256
```

The installer is unsigned, so SmartScreen will warn about an unknown publisher — that
is expected for an unsigned binary and is not itself evidence of anything. Check the
hash if you want more than my word for it.
