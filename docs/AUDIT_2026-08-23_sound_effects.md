# Audit — sound effects / DSP chain (2026-08-23)

A deep pass over the effects surface, requested alongside the second Bluetooth sweep. Five defects,
all fixed. Companion to [`AUDIT_2026-08-23_three_reports.md`](AUDIT_2026-08-23_three_reports.md),
whose §3 holds the Bluetooth findings.

Three of the five are the same failure in different clothes: **a control that says one thing while
the DSP does another.** That is the defect class this project has already cleaned out of the volume
row, the codec row and the Bluetooth switch — the effects chain simply had not been swept for it.

---

## E1. The boot DSP reconcile was gated on a file existing

`deferred_up` pushed the EQ and the sound chain at the DSP only `if (g_settings_loaded)`, reasoning
"no point pushing defaults on a fresh install".

**The reasoning is wrong, because the DSP is not ours and does not boot empty.** It holds whatever
the stock player last left in it. With no settings file Cinder drew its own defaults — every effect
off — and never sent a single call, so the screen said one thing and the hardware did another for
the whole session.

And `g_settings_loaded` is false more often than "fresh install" suggests. It is set by reading
`/contents/cinder_settings.conf` in `render_up`, and `/contents` is vfat, is handed wholesale to the
PC for USB-MSC, and this file's own comments call it *"both corruptible and periodically absent"*.
One unreadable boot and the DSP is never reconciled.

Two of the calls behind the gate are **not user preferences at all**, which is what makes it
indefensible rather than merely wasteful — `apply_sound_fn`'s own comments say so:

* **`SetSelectUsingEq`** — the device sits on `1` (the SIX-band, which Cinder does not expose), so
  without this call *every band the EQ screen writes is stored by the service and never put in the
  path.* That is verbatim the bug the selector was added to kill, reachable again through the gate.
* **`SetBtAudioSoundEffect(1)`** — project goal #7. Without it the chain over A2DP is whatever the
  stock player last left the flag at.

Both are assertions about somebody else's state, and neither has anything to do with whether we
found a file. **Fix:** the reconcile is unconditional; only the repeat-one *restore* stays gated,
because that one really is a restore.

---

## E2. The signal-path footer did not know Source Direct exists

The Sound screen's footer is the only thing on the device that claims to say where the audio
actually goes. `sound::Sound` **had no `source_direct` field at all**, so with Source Direct on the
footer drew

```
SIGNAL PATH (A): SOURCE → EQ (Rock) → DSEE HX → VPT·CLUB → DC PHASE → AMP → 3.5MM
```

over a chain that was **entirely bypassed**, and the only warning line it could draw was
ClearAudio+'s.

This is worse than the "why can I not hear VPT" evening the Advanced screen's override banner was
written for. ClearAudio+ at least has its switch on the Sound screen; Source Direct is set two
screens away, in Sound ▸ Advanced, and left no trace on the screen the user comes back to.

**Fix:** with Source Direct on the path reads `SOURCE → AMP → 3.5MM` — nothing between them — and
the warning names it. Precedence matches the Advanced banner exactly (Source Direct outranks
ClearAudio+, because it is the outer bypass and therefore what you would have to turn off first), so
the two screens cannot tell different stories about one state.

*A note on the wording:* the first draft named where the control lives — `(SOUND ▸ ADVANCED)` — and
`fit` truncated the line at `EVERY EFFECT BY…`, cutting the one word carrying the meaning. Caught by
rendering it, not by a test. The line is now shorter than ClearAudio+'s, and a test asserts it still
ends in `BYPASSED`.

---

## E3. The footer named an EQ preset that Tone Control had replaced

Sony's Equalizer and Tone Control are **alternatives** — `SetSelectUsingEq` picks one, and
`apply_sound_fn` correctly sends `ToneControl` when the Tone row is on. The footer printed
`EQ (<preset>)` unconditionally, so with Tone Control on it named a preset that was not in the path.

The same fact the project spent a day recovering ("every band the EQ screen has written since June
was stored by the service and never in the path") was being contradicted by the footer three screens
later. **Fix:** the stage reads `TONE` when Tone Control is in the path, `EQ (<preset>)` otherwise.

---

## E4. DSEE HX Custom and DSEE AI claimed nothing about their parent

Both rows are only in the path while **DSEE HX itself** is on — its switch is on the Sound screen.
`SetDseeHxCustomMode` tunes an upscaler, it does not start one.

Their two immediate neighbours on the same screen both declare their parent: Vinyl Character says
*"Vinyl Processor is off"*, and the Tone band editor says *"Three bands — Tone Control is off"*. The
DSEE rows, three lines away, said nothing. **Fix:** same sentence shape, same screen.

---

## E5. DSEE AI shipped as a working feature against the project's own evidence

`analysis/RE_dsp_effects_surface.md`, on this exact row:

> **DSEE AI** — Present in the API. Whether the A50 has the hardware is **UNVERIFIED** — treat like
> high gain until heard.

`cinder_effects.h` repeats it. And the row shipped drawn exactly like the toggles that do work:
*"DSEE AI — Upscaling with real-time source analysis"*, with a plain switch.

The precedent is in STATUS.md, in capitals, and it cost real device time:

> **High gain output — REMOVED.** numid 28/29 accept `high`, read back 1 and persist, and the codec
> ignores it: the A50 output stage lacks the ZX/WM1 hardware. *On this device a mixer control
> accepting a write is not evidence the feature works.*

DSEE AI is in exactly that position — an exported symbol whose write lands on hardware that
(DSEE AI arrived with the ZX500/A100 generation) may well not have it.

**Fix: the row says so, and is not removed.** Unlike high gain, nobody has measured DSEE AI inert —
the honest state is "we do not know", which a subtitle can carry and a bare toggle cannot. Removing
it would be asserting a measurement nobody has made.

**To settle it on device:** `cinder-probe --fx` prints `DSEE AI=n`, but a read-back is exactly the
evidence high gain proved worthless. It needs an ear test: DSEE HX on, a lossy source, and A/B the
AI toggle while `apply_sound_fn` is re-asserting (the probe's `--vpt`-style hold is the pattern —
cinder-home will otherwise overwrite it once a second). If it is inaudible **and** stock's own menu
on this model has no DSEE AI entry, remove the row the way high gain was removed.

---

## What was checked and found sound

Not everything swept turned up a defect; these were examined and are correct as they stand.

| Area | Finding |
|---|---|
| `fx_dirty` slot allocation | 18 slots into `g_fx_last[40]`; no overflow, and no second caller to collide with. |
| `fx_cache_drop` coverage | Called at boot **and** on BT reconnect, and both `apply_eq_fn` and `apply_sound_fn` follow it — the EQ is genuinely re-asserted, not just invalidated. |
| Uncached calls | `SetSelectUsingEq` and `SetBtAudioSoundEffect` are deliberately outside the cache. Correct: their whole job is re-assertion, and caching them would make the re-assert a no-op. |
| Settings persistence | Every Advanced value (`adv`, `dsee_mode`, `vinyl_type`, `tone`, `vpt_mode`, `dc_type`, `balance100`) is in `settings_body` and parsed back. |
| `apply_bt_codec` | Preference pushed before the connect, then `GetSoundStatus` re-read. Correct — A2DP negotiates during setup. |
| `bt_resync_volume` | Verify-first (reads the mixer, writes only on disagreement), so it cannot fight a level the user just set. |

---

## Verification

| Change | Verified how |
|---|---|
| `signal_path` (E2, E3) | Extracted as a **pure function** with 6 host tests — it was previously checkable only by looking at a screenshot. Plus a new UI-overflow sweep across 4 × 3 override combinations at every UI scale, which is what caught the truncated warning's successor. |
| Footer rendering | `cargo run -p cinder-host` in both themes: `out/sound_source_direct_*.png`, `out/sound_tone_control_*.png`. |
| DSEE row subtitles (E4, E5) | Overflow matrix (the Advanced sweep already covers every flag/mode/type combination at every scale). |
| Unconditional boot reconcile (E1) | **Not host-testable** — Sony IPC on the boot path. Confirm on device by `deferred_up: apply sound chain` appearing in the log on a boot with no `cinder_settings.conf`. |

Host gates after these changes: **404 Rust tests** (6 new), the 8-case overflow matrix, and 4 C++
self-tests. The C++ change is not compiled here — `build.sh` needs the glibc-2.23 sysroot and
libc++ 3.9.0 headers, absent in this environment.
