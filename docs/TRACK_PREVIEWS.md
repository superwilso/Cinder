# Track previews

Open **Menu -> Preview** for a finite pass through the current music queue, starting at its current
index. The standalone page uses an MD-inspired silver design, with English and Simplified Chinese
labels. This is a track-selection aid, not a new audio-analysis engine.

## Controls

- **4 s / 15 s:** choose the preview length. A track with less playable time left uses the remainder.
- **Previous / Next:** move between candidates within the finite pass.
- **Pause / Continue:** pause and resume the preview without changing the normal playback intent.
- **Play this track in full:** leave preview mode, start the selected track from the beginning and
  return to Now Playing.
- **End preview:** leave preview mode. Subsequent normal playback restores the saved queue and
  bookmark; the sample position must not replace the normal bookmark.

The pass stops at its end rather than wrapping indefinitely. An empty queue disables the transport
controls. Preview cannot start in FM, USB-DAC, Bluetooth receiver or USB storage mode. Normal music
over Bluetooth output still uses the existing playback route.

## Where a preview starts

The planner reads existing Sony **SABI** data from the library database. A valid marker takes
priority; otherwise a known-length track starts at `min(45 seconds, duration - preview length)`,
bounded at zero. A track with unknown duration starts at zero. Markers near the end and short tracks
are bounded to the available duration. The page displays the actual source and start position.

No models, Python runtime, new audio decoder, online service or third-party algorithm dependency is
added to the firmware. A Sony marker is an existing candidate, not a guarantee that the segment is
musically the chorus. Songs without valid markers still use the fixed fallback.

## Maintenance map

| Area | Responsibility |
|---|---|
| `player/cinder-ui/src/zappin.rs` | Pure start/length planning and finite-session rules |
| `player/cinder-ui/src/retro_preview.rs` | Page drawing and shared drawing/touch geometry |
| `player/cinder-ui/src/nav.rs` | Navigation, page state and preview actions |
| `player/cinder-ffi/src/lib.rs` | Candidate snapshot, saved normal context, generation checks and handoff confirmation |
| `cinder-home/src/main.cpp` and `src/zappin_policy.h` | Existing Sony playback calls, sample timing, late callbacks and normal-play handoff |
| `player/cinder-ffi/src/stats.rs` and `scrobble.rs` | Keep sample playback out of ordinary listening statistics |
| `cinder-home/harness/scenarios.cpp` | Real main-loop sequencing against fake services, including failure and retry paths |

The pure planner and page are separate modules. Playback integration follows the existing navigator,
FFI runtime and main-loop structure; it introduces no second audio backend or persistent database
schema. It does add state to those shared files, so future playback changes need the regression
checks below. This is a moderate maintenance cost, rather than a display-only feature.

### Contracts to preserve when playback changes

1. A preview owns a bounded candidate snapshot and a generation. A late callback for an older
   candidate or session must not seek, resume or confirm the new one.
2. Normal bookmarks and listening clocks stay protected during previews. A metadata/URI update alone
   does not complete a normal-play handoff; playback status, URI and actual position must agree.
3. Full playback starts at zero. Queue restoration uses its saved bookmark, including an exact zero
   or a bookmark for another URI. A failed handoff leaves the sample paused and remains retryable.
4. Queue edits, shuffle/repeat changes, explicit selection and source changes cancel or hand over
   ownership intentionally. Normal queue actions outside preview mode keep their existing behaviour.
5. Labels describe state; translated text must never decide transport or source behaviour.

## Checks

The existing player test and host-preview commands include planner, navigation, font/layout and
golden coverage. The harness lists its scenarios automatically, so the `zappin-*` cases are included
in the full harness run:

```sh
python3 tools/generate_locales.py --check
(cd player && cargo test --release)
cinder-home/harness/run.sh zappin-preview zappin-full-same-uri zappin-aba-stale-uri
cinder-home/harness/run.sh zappin-timeout-play-full zappin-timeout-play-restore
```

See [CONTRIBUTING.md](../CONTRIBUTING.md) for the full checks and the ARM/device build requirements.
For device regression, exercise both lengths, pause/resume, candidate changes, Full, End followed by
normal playback, finite completion and display sleep/wake. Check both marker and fallback sources;
hearing a fixed-position sample does not validate a chorus marker.

The submitted source was tested on a Sony NW-A55. The user reported no issues in their trial and
explicitly confirmed that source toasts no longer cover the controls. A photographed marker-source
preview at 0:50 for Chouchou / 1619khz matched device log seeks of 50,838 ms. This verifies use of an
existing marker; it is not an independent assessment of chorus accuracy or every hardware scenario.
Final ARM link, dependency/GLIBC, loader and ABI checks were performed separately from hosted CI.
The committed `dist/` release payload is not changed by this source PR; a release still requires the
maintainer's normal rebuild and payload-verification process.

In the local VM, the full native harness was not green: Bluetooth assertion failures and timeouts
also reproduced on unmodified v0.3.19 (`43abfbb6`), without track previews, under the same harness/VM.
Paired reruns reproduced the initially candidate-only failing names on the baseline as well. These
observations are not attributed to this preview change. The affected upstream Bluetooth scenarios,
service fakes and VM timeout behaviour should be investigated separately; this does not turn the
full-suite results into a pass. The targeted preview/queue checks passed.
