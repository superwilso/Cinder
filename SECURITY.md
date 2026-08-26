# Security policy

Cinder replaces the Home application on a Sony NW-A50-series Walkman. It ships twelve setuid-root
helpers, an installer that drives Sony's own firmware updater, and a launcher that runs on the boot
path. **This device has no public DFU or EDL recovery path**: a bad boot is recovered by the
escape ladder described below, or by an eMMC restore, or not at all.

That is the context for everything here. Please read it before reporting, and before contributing
anything that touches the boot path.

## Reporting a vulnerability

Open a **private** advisory: <https://github.com/superwilso/Cinder/security/advisories/new>.
Do not open a public issue for anything in the categories below.

Please include the firmware version, the Cinder channel (`stable` or `dev`), and — if the device
still boots — the tail of `/contents/cinderhome.log`.

Expect a first response within a week. This is a hobby project maintained by one person; there is
no SLA, and there is no bug bounty.

## What is in scope

The parts where a defect can cost someone their device or their root:

* **The setuid helpers** (`cinder-power`, `cinder-msc`, `cinder-clock`, `cinder-fm`,
  `cinder-voltable`, `cinder-gpunode`, `cinder-umount`, …). They run as root on behalf of an
  unprivileged UI. Argument handling, path handling and anything reachable from `/contents`
  (which is FAT, world-writable, and shared with any PC the player is plugged into) matter most.
* **The launcher and the escape ladder** — `install_cinderhome.sh`'s launcher, the bad-boot
  counter, the auto-revert, the crash supervisor, the kill switch. A defect that disarms an escape
  is more serious than one that crashes the app, because the app crashing is what the escapes are
  for.
* **The installer** — it writes `NW_WM_FW.UPG` to a device root and triggers Sony's updater.
* **Anything that can make the device unbootable**, whether or not an attacker is involved.

## What is out of scope

* **The device is unlocked by design.** Cinder is itself an unofficial modification; "an attacker
  with physical access and a USB cable can change the firmware" is the premise, not a finding.
* **Sony's own services and firmware.** Cinder drives closed Sony binaries over IPC. Defects in
  those belong to Sony. Report them here only where Cinder can reasonably defend against them.
* **The unsigned installer.** Known and documented (`docs/SHORTCOMINGS.md` D7). Code-signing
  certificates cost money this project does not have. The release publishes `SHA256SUMS`; verify
  the download against it.
* **Committed binary payload.** `cinder-home/dist/` holds prebuilt ARM binaries because building
  them needs a glibc-2.23 + libc++-3.9.0 cross toolchain. `tools/release.sh` rebuilds and compares
  them byte-for-byte before it will tag — but see D4/D7: that guard is currently opt-in.

## Rules that exist because something went wrong

These are not style preferences. Each was written after a real incident, and a change that breaks
one is a security-relevant change even if it looks cosmetic:

1. **Never write `/proc/regmon/<chip>/value`.** Selecting a register through `target` is a read;
   writing `value` changes audio hardware under the running player, and the codec is the one part
   of this device with no software recovery path.
2. **Never write to `BtTransmitterService`'s PCM socket without the type-1 handshake.** PCM sent
   while the connection is still parsing frames is read as a type and a length, and a garbage
   length reaches `operator new[]` inside a core Sony service. That rebooted the device twice.
3. **Never guess vtable slot indices** into Sony services. Recover them, or leave the feature off.
4. **An escape must depend on less than what it rescues.** See `docs/AUDIT_2026-07-26.md`.

## Verifying a download

```sh
sha256sum -c SHA256SUMS
```

`SHA256SUMS` is attached to each release. It proves the download matches what the release workflow
produced. It does **not** prove the workflow built this source tree — there is no build
attestation yet (D7).
