# CODING AGENTS: READ THIS FIRST

This is a **handoff bundle** from Claude Design (claude.ai/design).

A user mocked up designs in HTML/CSS/JS using an AI design tool, then exported this bundle so a coding agent can implement the designs for real.

## What you should do — IMPORTANT

**Read the chat transcripts first.** There are 4 chat transcript(s) in `nw-a55/chats/`. The transcripts show the full back-and-forth between the user and the design assistant — they tell you **what the user actually wants** and **where they landed** after iterating. Don't skip them. The final HTML files are the output, but the chat is where the intent lives.

**Find the primary design file under `nw-a55/project/` and read it top to bottom.** The chat transcripts will tell you which file the user was last iterating on. Then **follow its imports**: open every file it pulls in (shared components, CSS, scripts) so you understand how the pieces fit together before you start implementing.

**If anything is ambiguous, ask the user to confirm before you start implementing.** It's much cheaper to clarify scope up front than to build the wrong thing.

## About the design files

The design medium is **HTML/CSS/JS** — these are prototypes, not production code. Your job is to **recreate them pixel-perfectly** in whatever technology makes sense for the target codebase (React, Vue, native, whatever fits). Match the visual output; don't copy the prototype's internal structure unless it happens to fit.

**Don't render these files in a browser or take screenshots unless the user asks you to.** Everything you need — dimensions, colors, layout rules — is spelled out in the source. Read the HTML and CSS directly; a screenshot won't tell you anything they don't.

## Bundle contents

- `nw-a55/README.md` — this file
- `nw-a55/chats/` — conversation transcripts (read these!)
- `nw-a55/project/` — the `NW-A55` project files (HTML prototypes, assets, components)


## Goals:
- Faster boot time and better battery life
- improved UI and UX
- USB DAC mode with LDAC and 3.5mm output
- Night mode with darkened ui and dimmer screen
- built in battery effcient scrobbler
- queue and shelf functionality
- keep all audio effects (DSEE HX, ect) and try to apply them to bluetooth audio
- keep using the built in sound card for battery effciency
- lock screen, that turns of touch screen but leaves the buttons active
- fix the 32 bit time issue and prevent the 2038 crash
this is not a finished list and more goals may be added