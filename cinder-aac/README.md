# cinder-aac

AAC Bluetooth transmit for the Sony NW-A50 series. Out of the box the player can *receive* AAC but
only *sends* SBC, aptX, aptX HD and LDAC. This project adds AAC as a send codec, for headphones
that have none of the others (AirPods, Beats, most budget pairs) and would otherwise fall back
to SBC.

It is kept apart from [Cinder](https://github.com/superwilso/Cinder) on purpose: it touches Sony's
Bluetooth stack, so it lives in a repo of its own. Cinder includes it as a git submodule and
installs it as the optional `bt-aac` component; a Cinder build without it simply leaves that
component out.

## How it works

- The player already ships Fraunhofer's AAC encoder (`/system/lib/libFraunhoferAAC.so`,
  `aacEncOpen` / `aacEncEncode`), licensed with the device. Nothing in Sony's firmware calls it.
- This project hooks the existing stack at run time so that it advertises an AAC source endpoint
  and encodes with that on-device library.
- It ships no encoder and no Sony code.

## Rules for this repo

1. **No Sony bytes.** No binaries, no decompiled listings, no copied tables or strings beyond what
   a hook needs to recognise its target (a hash, an offset).
2. **Notes in our own words.** Describe what a function does; don't paste what Ghidra printed.
3. Everything runs against the copy of the firmware already on the owner's player.

Reverse engineering here is for interoperability: making an independent program (this one) work
with the player's Bluetooth stack (UK CDPA 1988 s.50B / s.50BA, EU Directive 2009/24/EC art. 6,
US 17 U.S.C. 1201(f)).

## Status

Works on the owner's player (stock 1.02 / Walkman One 3.02). AAC is added alongside Sony's four
(it shares a slot with aptX HD, swapped at each Bluetooth start). Install it through Cinder's
installer (Bluetooth: send AAC), or by hand with `./install.sh` for development. Also here: SBC XQ
(Dual Channel at bitpool 47, about 550 kbit/s), and fdk-aac 2.x as the encoder
(`build_fdk.sh`), which caps each frame to fit one packet. Details: [docs/STACK.md](docs/STACK.md).
