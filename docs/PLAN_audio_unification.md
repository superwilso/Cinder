# One player, five sources, three outputs — making the audio features agree

*Started 2026-10-04 at the owner's request: "unify the experience across the audio features and
match features between Bluetooth, wired, radio. Make everything logical. Hardware is the only
limit." This is the ledger: what each source gets on each output today, what was changed that day,
and what is left. States are from the code and the device checklist, not from memory; where nobody
has run a combination it says so.*

## The pieces

**Sources** — what can be making sound:

| Source | How it reaches the output |
|---|---|
| Library music | Sony's PlayerService → SoundServiceFw → jack PCM, or the Bluetooth transmitter |
| Soundscape | Cinder's own generator: under music inside SoundServiceFw (the shim), or alone on its own PCM / Bluetooth socket |
| FM radio | Tuner → codec's analogue input → Sony's AudioInPlayerService (`hw:0,1`) |
| Bluetooth receiver | A phone or PC → A2DP sink → jack |
| USB-DAC | The PC's UAC stream → jack, or Cinder's bridge → LDAC |

**Outputs** — 3.5 mm jack, Bluetooth (A2DP out), and USB-DAC's own two legs above.

## Where every source stands

`yes` = works and has been run on a player; `host` = built and host-tested only; `no` = not
built; `n/a` = the hardware has no such path.

| | Library | Soundscape | FM radio | BT receiver | USB-DAC |
|---|---|---|---|---|---|
| Jack | yes | yes (alone and over music) | yes on stock+Wampy; audio leg unproven on Walkman One | yes | yes |
| Bluetooth out | yes | over music: shim attaches (log), not yet heard; alone: host | built, never run (`fm-bt`) | n/a (the radio is the input) | yes (LDAC bridge) |
| Counts as "audible" for standby, power-off, Bluetooth auto off | yes | yes | **yes since 10-04** (was no) | yes | **yes since 10-04** (was no) |
| Display power state under it (stage 1) | yes, jack and Bluetooth | yes | held off until run (§34) | yes | held off until run (§34) |
| Sleep timer stops it | yes | yes | **yes since 10-04** (was no) | no | no (the PC is the transport) |
| Sound profile per output | yes | follows the output | follows the jack profile | follows the jack profile | its own profile |
| Mono | yes (shim) | yes | **no** — not through SoundServiceFw's PCM hooks | not established | LDAC leg only (bridge) |
| Sony's DSP (EQ, DSEE, VPT…) | yes | no (mixed after it, by design) | not established | not established | not established |
| Visualiser | PCM tap (five styles) + Sony's bands | no | Sony's bands if the analyzer sees the track; not established | no | no |
| Volume | jack: codec master; Bluetooth: AVRCP | own level, and With music % | jack master | jack master | jack master, or the sink's |
| Now Playing / lock screen / transport keys | yes | its own page | its own page; keys seek stations | its own page | its own page |
| Scrobble, play count | yes | n/a | no | no | no |

## Done on 2026-10-04

1. **One definition of audible** (`Audible` / `audible_now()` in `cinder-home/src/main.cpp`). The
   codec's standby, stage 1, stage 2, auto power-off and Bluetooth auto off all ask it. The radio
   and USB-DAC were missing from every copy of the old test.
2. **"Is the codec in the path"** is its own question now: the radio and the receiver come in
   through the codec and USB-DAC to the jack goes out through it, so a Bluetooth route no longer
   implies the codec can sleep.
3. **The sleep timer stops the radio.**

## What is left, in the order that makes the player most consistent

| # | Gap | What it takes | Limit |
|---|---|---|---|
| 1 | "Not established" cells: does Sony's DSP, the analyzer and mono reach FM, the receiver and USB-DAC? | One listening/recording session per source with the jack recorded on a PC (`reference` rig: mono shows as the side channel collapsing, EQ as a band lift) | None — measurement |
| 2 | Mono for FM / receiver / USB-DAC to the jack | If (1) says the shim's PCM hook does not see them: hook the PCM those tracks open (the `hw:0,1` path is a different `snd_pcm_t`). Summing in the codec would cover every source at once, but no mono control is exposed (`amixer` on the player lists none) and the register map has not been searched for one | The shim route is known to work; the codec route is unknown |
| 3 | One Now Playing for every source | The four "own page" screens share transport, volume, sleep timer and the visualiser slot; today each draws its own. A `Source` enum in `nav.rs` that Now Playing renders from, with the source's own controls as the middle band | UI work; touches the redesign's layout files (owner's call) |
| 4 | Visualiser for FM, receiver and soundscape | The tap reads PlayerService's queue only. The shim sees every PCM write to the jack, so it can publish a second ring the tap reads — one tap for every source | Shim work + device time |
| 5 | Sleep timer for the receiver and USB-DAC | Receiver: AVRCP pause to the phone, then leave the mode. USB-DAC: nothing to pause; mute and say so | Small |
| 6 | Soundscape alone over Bluetooth, and soundscape/mono heard over Bluetooth | Checklist 32.3 | Needs ears or a Bluetooth recorder |
| 7 | FM → Bluetooth | Built, never run (`cinder_fm_bt_out`) | Needs a station and headphones |
| 8 | Volume: one scale | Jack is 0–120 on the codec, Bluetooth is the sink's 0–127 in 16–17 steps with Fine volume splitting them. A single 0–100 shown everywhere, mapped per output | UI + care with AVLS |

Rebuilding Sony's side is on the table (owner, 10-04). The two places it would pay: **(4)** — a
ring published from inside SoundServiceFw replaces both the analyzer service and the PlayerService
tap with one path that sees everything; and **(2)** only if a mono mix turns up in the codec's registers.
