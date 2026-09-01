## What this changes

<!-- One paragraph. What behaviour is different after this, and why. -->

## Why

<!-- The reasoning. This project documents WHY at an unusually high standard — a comment saying
     what a line does is worth little, one saying what goes wrong without it is worth a lot.
     If this fixes something observed on a device, quote the log line. -->

## Blast radius

<!-- Tick every box this touches. Anything ticked below the line needs a reviewer who has read
     CONTRIBUTING.md's safety section, and generally needs a device test before merge. -->

- [ ] UI / drawing only (`player/cinder-ui`) — cannot brick anything
- [ ] Library / database (`player/cinder-db`)
- [ ] Installer or host tooling
- [ ] Docs only
---
- [ ] **Runs as root** (setuid helper, `cinder-home/src/cinder-*.c`)
- [ ] **Boot path** (launcher, crash supervisor, bad-boot counter, escape ladder)
- [ ] **Sony IPC** (`cinder-audio/`, hand-recovered vtable offsets — a wrong argument shape reboots the device)
- [ ] **USB-MSC mount ordering** (an ordering mistake corrupts the user's music volume)

## Checks

Everything below runs on a laptop in under a minute. CI runs the same commands, so a red tick here
is a red tick there.

- [ ] `tools/host_syntax_check.sh`
- [ ] `tools/shell_check.sh`
- [ ] `cinder-home/harness/run.sh`
- [ ] `bash cinder-home/tools/test_launcher.sh`
- [ ] `(cd player && cargo test --release)`
- [ ] `(cd installer && cargo test --release)`

## Device verification

- [ ] Tested on hardware — model and firmware: <!-- e.g. NW-A55, fw 1.20 -->
- [ ] Not tested on hardware, and the change cannot reach the device
- [ ] Not tested on hardware, and it **can** — say so plainly here so it is not merged as if it were

<!-- If you rebuilt cinder-home/dist/, say so: those binaries are committed deliberately, and
     tools/release.sh verifies the committed payload against a fresh build before it will tag. -->
