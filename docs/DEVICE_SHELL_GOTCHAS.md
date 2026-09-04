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

## `/data` and `/contents` are both `noexec` — only `/tmp` is executable

```
/emmc@usrdata /data     ext4 rw,nodev,noexec,noatime,...
/emmc@contents /contents vfat rw,noexec,noatime,...
tmpfs         /tmp      tmpfs rw,relatime,size=32768k
```

A binary or script pushed to `/data` or `/contents` returns **`permission denied`** however you
chmod it. The mode bits are fine; the mount is not. Two consequences:

* **Push probes to `/tmp`** (tmpfs, executable, 32 MB) and set
  `LD_LIBRARY_PATH=/system/vendor/sony/lib:/system/lib:/usr/lib`. `/tmp` is cleared by a reboot, so
  re-push after one. `install.md` used to say `/contents/cinder-probe`, which can only ever fail.
* **A script that must live on `/data`** (because it has to survive a reboot, or must not be handed
  to the PC by USB-MSC) can still be RUN — feed it to an interpreter instead of exec'ing it:
  `busybox sh /data/cinder/thing.sh`. `noexec` blocks `execve` of the file, not an interpreter
  reading it. Its shebang is then decorative.

## Detaching a long-running process from `adb`

Three things, and each one silently produces "it just died with no output":

1. **`setsid` is not on `PATH`.** It exists only as a busybox applet: `busybox setsid`.
   `nohup` is present at `/xbin/nohup` but did not survive in testing; `setsid` did.
2. **Background it AND redirect all three fds** (`>/dev/null 2>&1 </dev/null &`). adb kills the
   process group when its shell exits.
3. **Give it a moment before the shell exits** — `… & sleep 2`. Without that, adb tears the session
   down before `setsid` has established the new one and the child dies having written nothing.
   Indistinguishable from a broken script, and it is not one.

Checking whether it lived: `[ -d /proc/$p ]` with `$p` EMPTY tests `/proc`, which exists — so a
missing pidfile reads as "running". Guard the empty case explicitly.

**2026-09-04 — I CONCLUDED THIS WAS A LEFTOVER AND REMOVED IT. THAT WAS WRONG; IT IS RESTORED.**
Removing it produced the ghost immediately: the boot animation drew over the top of the Cinder UI on
the very next boot. So the paging theory in this section is CONFIRMED, not superseded — the panel
does present a page other than 0 around boot, and `fb0/pan` reading `0,0` does not prove otherwise.
The init clear fixes a stale PREVIOUS SESSION; it does nothing about another process drawing into
pages we have stopped writing.

**AND THEN SUPERSEDED PROPERLY — the flag is no longer needed and is off again.** The two questions
had been conflated: the partial blit's saving comes from writing only the CHANGED ROWS, while the
ghost came from writing only PAGE 0. They are independent. The blit now writes changed rows to
**every page**, so all three stay byte-identical (verified on device: same md5 for pages 0, 1 and 2)
and it does not matter which one the panel presents. `/contents/cinder_fb_allpages` is back to being
a pure escape hatch — it forces every row, every frame — rather than something the display depends
on. The (wrong) reasoning that removed it the first time follows:

**~~RESOLVED 2026-09-04 — and the flag was left on the device for three weeks.~~** The one-flag test
above was run (`touch`ed 2026-08-18) and then never undone, so every frame since wrote all three
pages: ~4.6 MB instead of ~1.5 MB, on a panel that only ever scans page 0. The ghost it was testing
for had a different cause entirely — mtkfb hands back the previous session's pixels across a reboot
— which was root-caused and fixed on 2026-08-26 with a `write_bytes` clear of the whole mapping at
init. The flag is removed; the boot log now reads `writing page 0 only`. If a ghost ever returns,
`touch` it again, but check the init clear first.
