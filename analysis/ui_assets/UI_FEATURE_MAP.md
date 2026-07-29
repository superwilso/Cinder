# NW-A55 / NW-A50 stock UI — complete screen & feature map

Source: `HgrmMediaPlayerApp` (stock firmware v1.02). 131 QML screens, organized by
functional area. Screen IDs (`sid_NNNN`) are Sony's own identifiers. Use this as the
sitemap; see `QML_INDEX.md` for each screen's buttons and English text, and
`index.html` for the visual assets.

Display: 480×854 portrait, eglfs (OpenGL ES). Navigation is swipe + the side
buttons (back, option, vol±, play/pause, prev/next).

---

## Now Playing (the player itself)
- `sid_0201 MusicPlayerDefault` — main now-playing screen (album art, title, transport, progress).
- `sid_0201 MusicPlayerDefaultForSimpleMode` — same, "Simple Mode" stripped-down variant.
- `sid_0202 MusicPlayerSpectrum` — now-playing with spectrum analyzer visual.
- `sid_0203 MusicPlayerLevelMeter` — now-playing with analog VU level meter.
- `sid_0212 MusicPlayerDigitalLevelMeter` — now-playing with digital peak meter.
- `sid_0204 SyncLyricDisp` — synchronized (timed) lyrics view. Variants for analog-amp / volume-bar layouts.
- `sid_0206 ContentDetailedInfo` — track detail (codec, bitrate, sample rate, DSD, file path). Variants for layout.
- `sid_0208 MusicPlayerHelpGuide` — first-run gesture/help overlay for the player.
- `sid_0401 TrackSequenceView` — play queue / "playing order" list.
- `sid_0601 BookmarkListView` — bookmark list playback.

## Library / browsing
- `sid_0901 Contents` — Library top (the category grid).
- `sid_0902 LibraryIconSelect` — choose which library icons/categories show.
- `sid_0936 LibraryTopHelpGuide` — library help overlay.
- `framework/*` views — the actual browse lists: AllTrack, Album, Artist, ArtistAlbum,
  Genre, GenreArtist, ReleaseYear, ReleaseYearArtist, Composer, PlayList, PlaylistTrack,
  Track, plus **HighResolution** Top/Artist views and **folder** browsing
  (First…EighthFolderListView = up to 8 nested folder levels), StorageTopView.

## Sound / audio effects  (the "Sound Settings" menu)
- `sid_0502 UserPreset` — save/recall custom sound presets.
- `sid_0506_0507 SixBandEqualizer` / `TenBandEqualizer` — graphic EQ (6 and 10 band).
- `sid_0506_0507 ToneControl` — tone control (alternative to EQ).
- `sid_0511 DcPhaseLinearizer` — DC Phase Linearizer (analog-amp phase emulation).
- `sid_0513 DynamicNormalizer` — Dynamic Normalizer (volume leveling).
- `sid_0514 LrBalance` — left/right balance (CENTER / L+n dB / R+n dB).
- `sid_0515 Vpt` — VPT (surround) modes.
- `sid_0521 DseeAi` — DSEE HX / DSEE-AI upscaling toggle.
- `sid_0522 VinylProcessor` — Vinyl Processor (turntable emulation).
- `sid_4107_4108 SoundEnhancementSetting` — master sound-enhancement enable/route.

## Bluetooth — transmitter (LDAC OUT to headphones/speaker)
- `sid_3101 BtConnect` — pair/connect a BT audio device.
- `sid_3108 RemoconConnect` — connect a BT remote.
- `sid_3110 BtAutoConnectSetting` — auto-reconnect toggle.
- `sid_3112 BtWirelessQualitySetting` — **LDAC quality: SBC / "connection" vs "sound quality" (990 kbps) / Auto**. ← codec-quality knob.
- `sid_3114 BtInfomation` / `sid_3152 BtDeviceInfo` — connected-device info.
- `sid_3150 BtPasskeyInput` — passkey entry.

## Bluetooth — receiver (phone → Walkman as a BT DAC/amp)
- `sid_3301 BtReceiver` — BT Receiver mode on/off.
- `sid_3303 BtReceiverRegisteredManagement` — manage paired source phones.
- `sid_3304 BtReceiverPlayingQualitySetting` — receiver-side codec quality.

## USB / USB-DAC  ← relevant to your LDAC-from-USB-DAC goal
- `sid_1201 MSC` — USB Mass Storage (file transfer mode).
- `sid_1401 DAC` — **USB-DAC mode playing screen** (when PC/phone feeds the Walkman as an external DAC).
- `sid_1404 DACSetting` — **USB-DAC settings** (e.g. "charge from connected device").
- `sid_4113 USBSettingScreen` — USB connection mode (auto-mount MSC etc.).
- `sid_4121 DacFilterSelect` — PCM digital-filter / roll-off select.
- `window/UsbDacDeviceWindow` — the top-level window that hosts USB-DAC mode.
- Note: the index shows the policy strings **"USB Audio Output — Sound Settings do not
  apply to USB audio output"** and **"Cannot display during USB audio output."** These
  are the app-layer gates around the USB-DAC path (see CLAUDE.md Part H3).

## Output routing / amp
- `sid_4115 OutputSetting` — output destination selection.
- `sid_4116 HighGainOutput` — high-gain toggle for stereo-mini and balanced jacks.
- `sid_4109 AudioConnectSetting` — "Audio Device Connection Settings" (BT vs wired, wireless quality).
- `sid_4105 DsdPlaySetting` — DSD playback method (DoP / PCM convert).
- `sid_4106 PlaySetting` — playback options (gapless, etc.).

## Recorder & FM  (NW-A50 has a mic + FM tuner)
- `sid_1601 RecorderTop`, `sid_1602 RecorderSynchro`, `sid_1603 RecorderManual`,
  `sid_1604 RecorderSetting`, `sid_1608 MetaEdit` — voice/line recorder + tagging.
- `sid_1701 FmRadio`, `sid_1702 FmRadioSetting` — FM tuner.
- `sid_1501/1502 LanguageStudy*` — A-B repeat / pitch language-learning player.

## Noise-cancel / ambient  (for NC-capable bundled headphones)
- `sid_4201 NcSetting` — Noise Cancelling.
- `sid_4202 AsmSetting` — Ambient Sound Mode (+ level).

## Device settings (the gear menu)
- `sid_4102_4103 SettingTop` — Settings root.
- `sid_4111 DeviceSetting` — device options.
- `sid_4110 LanguageStudySetting`, `sid_5301 LanguageSetting`, `sid_7202 IMESetting` — language/input.
- `sid_4118 AutoShutdownSetting` — auto power-off.
- `sid_4901 ScreenOffTimerSetting` — screen-off timer.
- `sid_4119 SdMountUmountSetting`, `sid_4120 SdInitializeSetting` — SD card mount/format.
- `sid_4101 InitializeTop` — Reset/Format hub (Reset settings, Format storage, Format SD,
  Rebuild DB, Restore factory config).
- `sid_4401–4403 Date*` — clock set + date format (YYYY-MM-DD / MM-DD-YYYY / DD-MM-YYYY).
- `sid_5601 DeviceInfo`, `sid_5602 RegulationInfo` — about / legal.

## First-run wizard & guides
- `sid_5403_5405 InitialSettingWizardStart`, `sid_5404 …Finish`,
  `sid_5406 …HighQualitySoundGuide`, `sid_5501 HighQualitySoundGuide`.

## System popups / chrome (shared components)
- VolumePopup, BatteryPopup, LowBatteryAlert, ShutdownLogo, DatabaseUpdatePopup,
  AsmMiconErrorAlert, ImePanelPopup, navigationBar (back button), Blank/BlackBack.

---

### How the UI is built (for re-implementing your own)
- **QML + Qt Quick 2.3** over **eglfs** (GPU). Each screen is a `*Window` → `ScreenBase`/
  `SettingBase`/`SoundSettingBase` → content. Lists use a shared `SwipeGrid`/framework view.
- **Strings** are numeric IDs (`qsTr("200019")`) resolved per-language from
  `vendor/sony/translations/HgrmMediaPlayerApp_<lang>.qm`. English map = `labels_en.json` (729 entries).
- **Styling** comes from a `viewstyle` object: `viewstyle.textcolor.L1/L2/L3`,
  `viewstyle.textsize.SS/S/L`, `viewstyle.font_family` (SST + SST_Fixed). Dark theme, white/gray text.
- **Images**: 796 PNGs compiled into the binary under `qrc:/assets/images/...` (carved to `assets/`).
