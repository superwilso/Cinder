# The device's shell is not your shell

*Written 2026-08-18 after an install reported complete success while doing three things wrong. All
three were the same kind of mistake: assuming a utility on the NW-A55 behaves the way it does on a
desktop. None of them produced an error the script noticed.*

## The incident, briefly

`tools/cinder-install.sh --full` printed `install complete — device rebooting`. In fact:

* the running `cinder-home` was **never killed**, so the new binary was written *under* the live
  process — `/proc/<pid>/exe` went `(deleted)` and the old code kept running;
* the device **never rebooted**;
* `cinder-umount` was reinstalled **without its setuid bit**, silently losing the only thing it
  exists for;
* and five of the six setuid helpers, including the new `cinder-fm`, **were never installed at
  all** — the loop aborted after the first one.

Every one of those was reported as success or not reported at all.

## 1. There is no `pidof`. There is no `pgrep`.

```
$ adb shell pidof cinder-home
pidof: not found
```

It exits non-zero, so **`pidof X >/dev/null 2>&1` reads as "X is not running" for every X**, always.
`cinder-install.sh` used it three times: to decide whether to kill the app, to wait for it to die,
and to escalate to `SIGKILL`. All three read "not running", so it skipped the kill entirely and then
swapped the binary under the live process.

Use `ps`, matching the full install path so the grep cannot match its own command line:

```sh
cinder_pid() { ps 2>/dev/null | grep "$INSTALL_PATH" | grep -v grep | awk '{print $2}' | head -1; }
```

`cinder-home/tools/screenshot.sh` **already knew this** — *"`ps | grep` because this busybox has no
pidof/pgrep"* — written weeks earlier. The knowledge existed in one file and never propagated, which
is the actual lesson: a device-environment fact belongs somewhere both scripts read.

Still outstanding: `player/deploy/install_cinder.sh` (lines ~120-142) uses `pidof
HgrmMediaPlayerApp` four times and will silently read "not running" the same way.

## 2. `rm` and `mv` do not accept `-f`

```
$ rm -f /data/local/tmp/x
rm failed for -f, No such file or directory
$ mv -f a b
failed on '-f' - Invalid cross-device link
```

The flag is parsed as a **filename**. `rm -f` on a missing file therefore *fails* — the exact
opposite of what `-f` means — and under a script that stops on error, it stops. That is what killed
the helper loop after the first iteration.

```sh
rm "$path" 2>/dev/null        # not: rm -f "$path"
mv "$tmp" "$dest"             # not: mv -f "$tmp" "$dest"
```

Note this applies only to code running **on the device**. In `cinder-install.sh` the host-side lines
(`rm -f "$SWAP_SCRIPT"`) run under WSL and are fine — the same file contains both, which is exactly
how the distinction gets missed.

## 3. `chown` clears the setuid bit — so it must come FIRST

```sh
chmod 4755 f && chown root:root f     # WRONG: ends up 0755, silently
chown root:root f && chmod 4755 f     # right
```

POSIX drops set-user-ID on a successful `chown` by a non-privileged-enough caller, and the device's
`chown` does it unconditionally. The swap script did it the wrong way round and then **printed
`(4755 root:root)` from its own intent rather than from the filesystem**, so the log asserted the
thing that had just failed.

Report what is on disk, never what you asked for:

```sh
echo "[swap] installed helper: $h ($(ls -l "$DEST/$h" | cut -c1-10))"
```

`cinder-home/deploy/install_cinderhome.sh` — the `.UPG` path — had this right everywhere already
(`temp -> chown root -> chmod 4755 -> mv`). Only the adb swap path was inverted, so the two install
routes produced *different* permissions from the same inputs.

## 4. An unquoted heredoc executes its own COMMENTS

`cinder-install.sh` builds the on-device swap script with an unquoted heredoc, so the host expands
`$var`, `` `cmd` `` and `$( )` **while generating it** — including inside comments. Both of these
were live code:

```sh
# NO `-f` ANYWHERE ON THIS DEVICE.        -> backticks ran; the line became "NO  ANYWHERE"
# ... so it runs `ls -l /` and splices    -> $( ) ran on the HOST and spliced a whole
#     that output into the loop                directory listing into the middle of the loop
```

The second one was a comment *explaining this exact bug*, and it caused it. The generated script
still ran and still reported success, having skipped most of the loop.

Escape every dollar, keep backticks out of the prose, and gate the output — `cinder-install.sh` now
refuses to upload a script containing a line that looks like a directory listing, and runs `sh -n`
over it first.

## 5. Killing the app races the reboot

The swap script killed `cinder-home` and then installed the setuid helpers. Killing the foreground
app makes appmgr `android_reboot` the device, so the reboot landed **in the middle of the helper
loop** — one run installed 1 of 6 helpers, the next 3 of 6, both reporting success. Nothing in the
output distinguished them.

The helpers are ordinary files with no relationship to the running process, so they now go in
**before** anything is killed. Sequence: backup → remount rw → helpers → arm no-respawn → kill →
swap the binary → remount ro → reboot.

## The rule these share

All three failures are the same shape as the ones in `project_cinder_badboot_latch`: **a check that
cannot fail is not a check.** `pidof` missing looks like "not running"; `rm -f` missing looks like a
hard error where the flag was meant to prevent one; a log line built from intent looks like proof.

So, for anything running on the device:

* verify by **reading back the result**, not by trusting a return code or an echo;
* prefer `ps`, plain `rm`/`mv` with `2>/dev/null`, and `ls -l` after a `chmod`;
* and when a script has both host-side and device-side commands in it, mark which is which.

---

## Appendix — a transient ghost frame on screen change (2026-08-18, open)

Reported from the device: pressing into Library showed *"a shadow of the home screen under it"*,
and it was **clean again shortly after**. Recorded here because the investigation ruled things out
that are worth not re-checking.

**Not** two processes: `ps` showed exactly one `cinder-home` and no `HgrmMediaPlayerApp`.

**Not** Cinder's canvas: `library.rs` opens its render with `c.fill(t.bg)`, and a screenshot taken
through the `app` route — which is what cinder-home actually rendered — was clean.

So it is below the canvas, in the present path, and it self-heals. That shape fits the framebuffer:

```
fb0 virtual_size 480x2400  ->  3 pages of 480x800
cinder-ffi: fb 480x800 32bpp stride 1920 pages 3 (writing page 0 only) — flip-on-blit
```

Cinder writes **page 0 only** and pins `yoffset = 0`, on the reasoning that the panel never pans
(`/sys/class/graphics/fb0/pan` did read `0,0`). If mtkfb ever presents another page, pages 1 and 2
still hold *an older screen* — which is precisely a ghost of the previous one. It clearing by itself
is consistent too: the renderer does a forced full repaint 1×/s for life, so a stale page is
overwritten within a second.

**The one-flag test, no rebuild:** `touch /contents/cinder_fb_allpages` and restart. That restores
writing every page (the original behaviour; ~4.6 MB/frame instead of ~1.5 MB). If the ghost stops,
the paging theory is confirmed and the fix is to make all-pages the default, or better, to write all
pages only on a screen CHANGE and page 0 for steady-state repaints.

Not yet run — the device was disconnected before the test. If it recurs, that is the first thing to try.
