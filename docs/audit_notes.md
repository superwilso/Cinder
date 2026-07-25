# Audit Notes — v1.4

Two independent external audits were integrated into v1.4. Every factual claim in both
audits was verified against primary web sources before integration. This file is the
verification ledger.

---

## Audit 1 — findings and disposition

| Claim | Disposition | Source checked |
|---|---|---|
| Wampy license is GPLv3 + Commons Clause (not MIT) | **ACCEPTED** | Wampy LICENSE file + MAKING_OF.md verbatim quote |
| GitHub stats: C 80.9%, C++ 17.3%, 71 releases, v1.14.3 | **ACCEPTED** | GitHub repo page verified |
| FM stock range 87.5–108.0 MHz | **ACCEPTED** | Sony Help Guide |
| Sony specs say microSD/SDHC/SDXC without size ceiling | **ACCEPTED** | Sony spec page |
| Scrobbler path is `/contents/.scrobbler.log` not `/contents/CFW/.scrobbler.log` | **ACCEPTED** | Scrobbler source + desktop tool compatibility |

---

## Audit 2 — findings and disposition

| Claim | Disposition | Source checked |
|---|---|---|
| MT8590 ARM Cortex-A7 dual-core @ 1.8 GHz | **NOT ADOPTED** — no public MT8590 datasheet found; marked [Unverified] | No independent source |
| Android 5.0 (Lollipop) base | **NOT ADOPTED** — plausible but unverified; characterization softened to "Android-derived init" | getprop on device required |
| GPL kernel source has `mediatek/mt8590/icx-machine-links.c` | **CONDITIONALLY ADOPTED** as [Community-reported] — consistent with confirmed evidence; not independently checked against GPL source drop | Not independently verified |
| `ro.board.platform = mt8590`, `ro.boot.console = ttyMT1` | **NOT ADOPTED** as stated facts — marked [Unverified]; consistent with MT8590 but not independently confirmed | Requires on-device verification |

---

## MT8590 retraction — chronology

- **v1.0:** Asserted MT8590 SoC. No citation.
- **v1.3:** Retracted as "unsourceable fabrication."
- **v1.4:** Retraction reversed. `unknown321/wbrt` README explicitly names MT8590 for the
  NW-A30/A40/A50/ZX300/WM1A/WM1Z/DMP-Z1 family. Wampy `MAKING_OF.md` references MediaTek
  platform emulator requirement. USB VID `0x0E8D` is MediaTek's registered vendor ID.

The v1.3 retraction failed because the verifier checked only the Wampy repository and not
the author's adjacent `wbrt` repository. Lesson: check the author's full GitHub profile
before retracting a claim that came from the community.

---

## Trust-tier policy (v1.4 onwards)

All factual claims must carry one of:

- **[Verified]** — confirmed from ≥2 independent primary sources (manufacturer docs,
  author's own code/docs, official spec sheets)
- **[Community-reported]** — from Wampy, Rockbox, or scrobbler source; single-source;
  not independently verified against primary docs
- **[Hypothesis]** — inference from available evidence; requires on-device confirmation
- **[Unverified]** — claimed by an external audit but not independently sourced by this
  project; retained because it is plausible and consistent with verified evidence

Claims without a trust tier are an error in the document — flag and resolve.
