# SensMe — the analysis engine in Music Center for PC (2026-09-16)

**Question.** SensMe channels on the NW-A55 are *data-gated*: Sony's scanner reads an analysis tag
(`USR_SMFMF`) out of each music file into MTPDB (`object_ext_*` akeys 51-59), and only Sony's PC
software ever writes that tag. Can Cinder produce it for a user's library?

**Answer so far: the engine can be driven directly, on Windows, from our own code.** Music Center's
analysis library runs without Music Center installed or registered, and it analysed 90 s of
synthetic audio in 0.5 s. What is still open is the exact bytes Music Center writes into each file
format (the container around the analysis output), and a device reading that proves the A55 turns
our tag into channels.

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
  `SMFM` next to the FLAC parser's strings — plausibly a FLAC APPLICATION block ID. **Unconfirmed.**
* Parses the result through `SmuWalkmanChInfo_initBySmfmfReadFunc`, `SmuSensMeAxis_get`,
  `SmuSmfmfContents_get`, and stores `SENSMECHANNELID/TEMPO/MOOD/TYPE/STYLE/TIME`,
  `SMFMF12TONEV1/V2`, `SMFMFBEATIZER` (akeys 51-59), plus `STAE`, `STNM`, `SBZT`, `VNDM` FourCCs.

## 5. What is left, in order

1. **The container.** Which bytes go into the GEOB (and the MP4/FLAC equivalents): result 89 as is,
   or wrapped. Cheapest evidence: one file analysed by the real Music Center, then a GEOB dump. The
   alternatives are reading `OmgPcMan.dll` (x86, symbols partly present) or the A50's
   `SmuWalkmanChInfo_initBySmfmfReadFunc` (ARM).
2. **Real audio.** Decode a track with Windows Media Foundation (MP3, AAC and FLAC are built in on
   Windows 10+) and feed the engine; check that the integer results move sensibly (tempo against a
   known BPM).
3. **One device proof.** Tag a few files, copy them on, let the stock scanner run, and read MTPDB
   akeys 51-59. Mind `reference_mtpdb_rescan_hazard`: never reboot during a rescan, and back up
   `MTPDB.dat` first.
4. Then a tagger in the installer's family (Windows first, since the engine is Windows-only) and
   Cinder's SensMe screen reading `object_ext_int`.
