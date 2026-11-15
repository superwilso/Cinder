<!--
  The part of every GitHub release body that does not change. Keep it short: the detail lives in
  install.md and RECOVERY.md, and is not repeated per release. .github/workflows/release.yml puts
  the version's own page above this (.github/release-notes/vX.Y.Z.md) or a pointer to the
  CHANGELOG, and GitHub's commit list below.
-->
## Downloads

| File | What it is |
|---|---|
| `cinder-installer-windows-x64.exe` | Install, update or uninstall. Connect the Walkman in USB mass-storage mode and run it. |
| `cinder-installer-linux-x64` | The same, run with `sudo`. |
| `cinder-installer-macos` | The same, run with `sudo`. Experimental: not yet run on a real Mac. |
| `cinder-rescue-*` | Experimental. Backs up the player's memory, and restores it after a boot loop. [RECOVERY.md](../blob/main/RECOVERY.md), step 3. |
| `cinder-extras.zip` | The `.UPG` packages for flashing by hand, and libusb's licence. |
| `cinder-payload.tar.gz` | The player-side files every installer carries, built on GitHub from this tag. |

An update keeps the options already chosen on the player. Steps, the command line and the options:
[install.md](../blob/main/install.md). **Read [RECOVERY.md](../blob/main/RECOVERY.md) first.**

## Checking a download

`sha256sum -c --ignore-missing SHA256SUMS`, and `gh attestation verify <file> -R superwilso/Cinder`
for where it was built. Windows and the details: [install.md, Checking a download](../blob/main/install.md#checking-a-download).
The `.exe` is unsigned, so SmartScreen warns about it.

The Mac installer and cinder-rescue link [libusb](https://libusb.info) 1.0.27 statically (LGPL 2.1;
licence in `cinder-extras.zip`, source at <https://github.com/libusb/libusb/tree/v1.0.27>). To use
another libusb, build `installer/` or `rescue/` from this tag's source.
