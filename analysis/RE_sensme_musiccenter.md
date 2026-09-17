# SensMe — the analysis engine in Music Center for PC (2026-09-16, extended 2026-09-17)

**Question.** SensMe channels on the NW-A55 are *data-gated*: Sony's scanner reads an analysis tag
(`USR_SMFMF`) out of each music file into MTPDB (`object_ext_*` akeys 51-59), and only Sony's PC
software ever writes that tag. Can Cinder produce it for a user's library?

**Answer so far: the engine can be driven directly, on Windows, from our own code, and its output is
small.** Music Center's analysis library runs without Music Center installed or registered: a
4-minute track takes 1.3 s and produces about 6 KB (§5), which is exactly the set of chunks the
player's scanner parses to compute its axes and channels (§6). FLAC carries it as an APPLICATION
block with id `SMFM` (§7). Still open: what else Music Center puts in the tag to reach the
megabyte reported by others, the MP3/MP4 containers, and a device reading that proves the A55 turns
our tag into channels (§9).

Nothing here was written to the player. Sony's files stay out of the repository; our probe is
`tools/sensme/smfmf_probe.c`.

## 1. Getting the files out of the installer

`musiccenter_setup_2.7.3.exe` (the owner's download, 187 MB) is an InstallShield Basic MSI wrapper:

1. `7z x musiccenter_setup_2.7.3.exe` → the overlay as `[0]` (186,797,088 bytes).
2. The overlay holds 17 language transforms (`1031.mst`…) and **`MusicCenter.msi`**, a compound
   document with 512-byte sectors starting at `0x211e72`. Carve from there to the end.
   *Carving the `MSCF` signature found inside instead does NOT work*: the cab is an MSI stream, so
   its sectors are scattered, and it parses for exactly one 512-byte sector before turning to noise.
3. `7z x MusicCenter.msi` → 124 streams, including `Data1.cab` (166,671,471 bytes, 1500 files).
4. `7z x Data1.cab` → files named by MSI file key (`_A4C4B476…`). Real names come from each PE's
   version resource (`OriginalFilename`).

| File key | Name | What it is |
|---|---|---|
| `_A4C4B476B85B159B9B503B9AA56E5B7C` | **`MMLib11.dll`** (341 KB, x86) | "Sony Music Mining Library" — **the analysis engine** |
| `_C477A29DAB129F85ABE5F375ED828D40` | `OmgPcMan.dll` | carries `USR_SMFMF` and `Application/SMFMF`: the tag writer side |
| `_29DF24B3644D9057249F4E680383E8FF` | `id3parser.dll` | `CId3parser::CGeob` Get/Put: ID3 GEOB frames |
| `_81C0151CFA8D559BEBAAA37A12EB8510` | `OmgMp4LibWrapper.dll` | MP4 side, also uses `CGeob` |

`MMLib11.dll` imports only Windows system DLLs (kernel32, user32, advapi32, ole32, oleaut32,
shlwapi, rpcrt4).

## 2. The engine's interface (from its own type library)

Read with `LoadTypeLibEx(REGKIND_NONE)` + `ITypeInfo` on Windows. Both interfaces are dual
(`IDispatch`-derived), so the vtable slots below are the raw ones.

`MusicAnalysis2`, CLSID `{8DE9ED2D-8CFB-4B33-B8E3-CEE06F2903E3}`,
interface **`IMusicAnalysis2`** `{0959C485-A191-4508-A338-9E2B3813A166}`:

| slot | method |
|---|---|
| 7 | `Run()` |
| 8 | `Stop()` |
| 9–11 | `GetPriorityRange(short*, short*)` (−2..2), `SetPriority(short)`, `GetPriority(short*)` |
| 12 | `EnumParameterIDs(IEnumVARIANT**)` |
| 13, 14 | `SetParameter(int id, VARIANT)`, `GetParameter(int id, VARIANT*)` |
| 15 | `InputPCM(BYTE* buf, ULONG bytes, LONG bLast, LONG bAsynchronize)` |
| 16 | `SetInputPCMFormat(USHORT channels, ULONG rate, USHORT bits)` |
| 17 | `GetResult(IAnalysisResult2**)` |
| 18 | `EnumResultIDs(IEnumVARIANT**)` |
| 19–22 | `IsSmfmfUpdatable(BYTE*, ULONG)`, `IsSmfmfUpdatable2(VARIANT)`, `MergeSmfmf(BYTE*, ULONG, BYTE*, ULONG, VARIANT*)`, `MergeSmfmf2(VARIANT, VARIANT, VARIANT*)` |

**`IAnalysisResult2`** `{003354FE-746A-4D18-971D-BB347888AB74}`: slot 7 `EnumResultIDs(IEnumVARIANT**)`,
slot 8 `GetResultByID(int, VARIANT*)`. Events (`_IMusicAnalysisEvents2`
`{750F4B11-E128-418C-AAF7-581C3E4BB2FE}`): `OnMusicAnalysisReadyForNextInput(err)`,
`OnMusicAnalysisProgress(state, err)`. Errors: `MMER_NOERROR 0, PCMTOOLONG 1, PCMTOOSHORT 2,
OUTOFMEMORY 3, ABORT 4, ERROR 5`.

No registration is needed: `LoadLibrary("MMLib11.dll")` → `DllGetClassObject(CLSID)` →
`IClassFactory::CreateInstance(IID_IMusicAnalysis2)`.

## 3. First run (synthetic audio: a chord plus a 120 BPM kick, 90 s, 44.1 kHz/16-bit stereo)

`tools/sensme/smfmf_probe.c`, built with `i686-w64-mingw32-gcc`, run from WSL through Windows
interop (no elevation):

* Parameters: `0 = 6`, `1 = 256`, `2 = 47`, `3 = "1.1.2.643"` (engine version), `4 = false`, `5 = 0`.
* `SetInputPCMFormat(2, 44100, 16)` → `Run()` → 969 × `InputPCM` (16 KB each, synchronous) → all
  `S_OK`, **500 ms** for 90 s of audio.
* `GetResult` → **91 result IDs**. IDs 0–85 are integers (for example 0 = 1, 1 = 60, 4 = 90,
  28 = 255, 36 = 127). 86 and 87 return `E_FAIL` on this input. **89 and 90 are byte arrays:**
  * **89**, 1568 bytes: a run of chunks `GBPM`, `STBF`, `STMO`, `STHF`, `STMM`, …
  * **90**, 1100 bytes: one `STMM` chunk.
* Each chunk: `[FourCC][ "STAE" ][ "MMLW" ][01 00 80 00][u32 big-endian payload size][payload]`.
  `GBPM` carries 4 bytes, and the `STMM` chunk a 1080-byte payload.

The integer IDs are not yet mapped to meanings; the synthetic input was only meant to prove the
call sequence.

## 4. The player's reader side (A50 firmware, `libMediaStoreService.so`)

* Knows `USR_SMFMF` and `application/smfmf` (ID3 GEOB) in the MP3 parser
  (`GmpMetaParserMp3_mp3Smfmf`, `Id3GeobParser_getDataOffset`), MP4 (`MP4Parser_readSmfmf`,
  `MP43GPParser_getSmfmfOffset`), ASF/WMA (`fetchSmfmf`), OMA (`GmpOmaSmfmfReader`), and the FourCC
  `SMFM` next to the FLAC parser's strings — the FLAC APPLICATION block id (confirmed from the writer, §7).
* Parses the result through `SmuWalkmanChInfo_initBySmfmfReadFunc`, `SmuSensMeAxis_get`,
  `SmuSmfmfContents_get`, and stores `SENSMECHANNELID/TEMPO/MOOD/TYPE/STYLE/TIME`,
  `SMFMF12TONEV1/V2`, `SMFMFBEATIZER` (akeys 51-59), plus `STAE`, `STNM`, `SBZT`, `VNDM` FourCCs.

## 5. Real audio (2026-09-17)

Two FLAC tracks from the owner's library, decoded to s16le 44.1 kHz stereo with ffmpeg and fed
through the same probe (which now takes `id=value` parameter overrides after the audio path):

| Track | Length | Engine time | `GBPM` | Result 89 | Result 90 (`STMM`) | Result 88 |
|---|---|---|---|---|---|---|
| AC/DC, *Back in Black* | 255 s | **1.33 s** | **93.44** | 6,444 B | 5,968 B | 188,996 |
| Air, *La Femme d'argent* | 431 s | **1.69 s** | **79.86** | 5,996 B | 5,524 B | 23,050 |

* **The whole analysis is about 6 KB per track, and it does not grow with length.** Result 90 is the
  `STMM` chunk that result 89 already ends with, so ~6 KB is the lot. This is not the "almost a
  megabyte" of `USR_SMFMF` that Wampy's `MAKING_OF.md` reports Music Center adding to each file —
  whatever makes up that size, it is not the engine's output. What it is stays **unverified** until
  a file tagged by Music Center itself is examined (§8).
* **Speed.** Wampy's note estimates ~10 s per song through Music Center; the engine alone is under
  2 s for a 7-minute track, decode excluded.
* Result 89 is a chunk list: `GBPM` (4 B), `STBF` (36), `STSA` (12–16), `STMO` (160), `STHF` (160),
  `STMM` (5.5–6 KB). Every chunk has the 20-byte header from §3.
  * `GBPM` is a **big-endian float32 tempo** — 93.4 for a song usually quoted at 94 BPM. Integer
    result 1 is the same value rounded (93, 79).
  * `STSA` holds offsets that look like milliseconds, and result 88 is one of them exactly for the Air
    track (23,050). The player stores a **sabi** (サビ, the chorus/hook) position from SMFMF (§6), so
    this is the likely source. *Unverified* until a device reading shows the same number.
  * `STMO` and `STHF` are runs of 4-byte records (`01 00 ii vv`, `08 ii vv 00`) — per-band or
    per-segment values; not decoded.
* Parameter 4 (a boolean, default false) adds one more chunk, `GVNM` (two floats), and makes results
  86 (a float) and 87 (a short) succeed. The player knows the `GVNM` name (§6). Probably a loudness
  measure; not decoded.

## 6. What the player does with it (A50 `libMediaStoreService.so`, Ghidra, 2026-09-17)

The library is stripped PIC Thumb code whose string references go through literal pools, so Ghidra's
auto-analysis misses most of them. The functions were found by the calls that set the `akey`
numbers (a helper at file offset `0x402b4`, Ghidra `0x602b4`, takes the akey in `r0`), then created
and decompiled with `analysis/clear_bass/ghidra/DecompileAddrs.java` (`t:` addresses are Thumb).
Offsets below are Ghidra's (file offset + `0x10000`).

| Function | Log name (by source-line order) | What it stores |
|---|---|---|
| `0x50c68` | channel info (`SmuWalkmanChInfo_initBySmfmfReadFunc`) | A **channel bitmask**: `0x540dc` asks `0x568a8` for up to 14 channel ids and ORs `1 << (table[id] + 1)` |
| `0x50da4` | `fillSmfmfMeta_sabi` (`SmuWalkmanChInfo_getSabi`) | The sabi position |
| `0x50f20` | `fillSensMeAxisMeta` (`SmuSensMeAxis_get`) | Five doubles → akeys **52 TEMPO, 53 MOOD, 54 TYPE, 55 STYLE, 56 TIME** as int16, each only when ≥ 0 |
| `0x510c4` | `fillSmfmfContentsMeta` (`SmuSmfmfContents_get`) | Presence flags → akeys **57 12TONEV1, 58 12TONEV2, 59 BEATIZER** |

* **The channels are computed on the player, from the chunks.** `0x55060` walks the blob as the same
  20-byte chunk list the engine emits (`0x562e0`, which matches names with `strcmp`), and
  `0x550f0` turns chunk data into each axis with arithmetic that includes `exp`. The chunk names the
  player knows are exactly the engine's — `GBPM STBF STSA STMO STHF STMM GVNM` — plus `SBZT`, `STAE`,
  `STNM` and `VNDM`.
* So **the engine's ~6 KB result is what the player parses**; nothing else is needed to reach the
  axes and channels, as far as this code shows.
* Blobs under 20 KB (`size >> 12 < 5`) are read into memory; larger ones are streamed in 2 KB steps.
  The player therefore expects SMFMF data larger than the engine's output to exist.
* MTPDB's own `schema` table names the two that bypass the akey helper: **akey 50 `WMCHANNELINFO`**
  (data type 119) for the channel bitmask and **akey 60 `SABI`** (119); akey 51 `SENSMECHANNELID` is
  type 73. Which of 50/60 each function writes is still to be confirmed by a device reading.
* Still open on this side: the channel-id → name table (`table` in `0x540dc`, 12-byte entries), and how
  `HgrmMediaPlayerApp`'s `SensMePlayer` orders a channel (it logs `sabi_position`, falling back to
  `duration / 2`). Its 14 channels, from its own image names: shuffle all, morning, daytime, evening,
  night, midnight, active, relax, upbeat, mellow, lounge, emotional, dance, extreme.

## 7. The FLAC container (`OpcFlac.dll`, Music Center 2.7.3)

Read from the disassembly around the only two uses of the constant `0x4d464d53`:

* Sony property `0x1707` (SMFMF) on a FLAC file walks the metadata blocks with libFLAC's iterator and
  takes the block with **`type == 2` (APPLICATION) and application id `SMFM`**.
* Writing it builds a new type-2 block from the property's `VT_ARRAY | VT_UI1` byte array. No header
  is added in this function; whether the caller (`mediacore.node`) wraps the engine result before
  passing it down is **unverified**.

MP3 (`id3parser.dll`, GEOB `USR_SMFMF`, MIME `application/smfmf`) and MP4 are not traced yet.

## 8. Music Center on the owner's PC

Music Center 2.7.3 is installed (`C:\Program Files (x86)\Sony\Music Center`; the engine sits in
`AVLib\MMLib11.dll`, byte-identical to the one carved in §1). Its library is an Electron app with
NeDB files under `%APPDATA%\Sony\Music Center\db` — 4,267 tracks after a seven-minute first run on
2026-09-16, **none analysed yet**. From its JavaScript (`@z-app/media-manager`):

* 12-tone analysis is one of the "audio recognition" auto-fetch targets (`4` = `"12tone"`, already
  enabled in the owner's `registry.json`), tracked per track as `analysis.twelveTone` and shown as the
  *12 Tone Analysis* column.
* Each result is cached at `fringe\audio\<id>\smfmf.bin`, and is also written into the file through
  the media-core property `smfmf`.

**Cheapest way to settle the megabyte question and the containers:** let Music Center analyse a
copy of one FLAC and one MP3, then compare `fringe\audio\<id>\smfmf.bin` with this probe's result 89
and dump the `SMFM` block / `USR_SMFMF` GEOB.

## 9. What is left, in order

1. **One Music Center-tagged file** (§8): the megabyte question, and the exact payload for FLAC and MP3.
2. **One device proof.** Put result 89 into an `SMFM` block on a *copy* of one FLAC, copy it on, let
   the stock scanner run, and read MTPDB akeys 50-60 and 121. Three Flint-tagged copies were made for
   this on 2026-09-17 (Taxman, For No One, Chicken Grease). Mind `reference_mtpdb_rescan_hazard`:
   never reboot during a rescan, and back up `MTPDB.dat` first.
3. The channel-id table and the akeys for bitmask and sabi (§6).
4. The plan built on this: `docs/PLAN_sensme_sync.md`.
