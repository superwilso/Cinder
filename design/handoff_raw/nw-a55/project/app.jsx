// App — Design Canvas composition for the NW-A55 UI exploration.
//
// Three directions × 20 screens each.
//   01 · Hi-Res     — gold/black, faithful Walkman audiophile.
//   02 · Nocturne   — dark editorial, soft violet, serif display.
//   03 · Terminal   — amber phosphor, ASCII brutalism.
//
// Plus a Night Mode triplet at the end (darkest possible, no album art,
// quick-access tiles for Bluetooth / Albums / EQ / etc.).

function App() {
  // 480×800 portrait is the device's real screen orientation (3.1" TFT).
  const W = 480, H = 800;

  // Shared row of screens — every direction implements all of these.
  const SCREENS = [
    'np-hero',   'np-dense',   'np-meter',
    'lyrics',    'track',
    'library',   'queue',      'browse',
    'eq',        'sound',      'output',
    'bt',        'bt-rx',      'usb-dac',
    'settings',  'reset',      'volume',
    'wizard',    'lock',
  ];

  return (
    <DesignCanvas>

      {/* ─── DIR 1 · HI-RES ──────────────────────────────── */}
      <DCSection id="hires" title="01 · Hi-Res" subtitle="Faithful Walkman feel — deep black, gold hi-res accent, mono metadata. Audiophile-leaning, dense by default.">
        {/* Now Playing */}
        <DCArtboard id="hires-np-hero"     label="Now Playing · Hero"      width={W} height={H}><HiRes_NowPlayingHero /></DCArtboard>
        <DCArtboard id="hires-np-dense"    label="Now Playing · Dense"     width={W} height={H}><HiRes_NowPlayingDense /></DCArtboard>
        <DCArtboard id="hires-np-meter"    label="Now Playing · VU Meter"  width={W} height={H}><HiRes_NowPlayingMeter /></DCArtboard>
        <DCArtboard id="hires-lyrics"      label="Synced Lyrics"           width={W} height={H}><HiRes_Lyrics /></DCArtboard>
        <DCArtboard id="hires-track"       label="Track Info"              width={W} height={H}><HiRes_TrackDetail /></DCArtboard>

        {/* Library / Browse */}
        <DCArtboard id="hires-library"     label="Library"                 width={W} height={H}><HiRes_Library /></DCArtboard>
        <DCArtboard id="hires-queue"       label="Up Next"                 width={W} height={H}><HiRes_Queue /></DCArtboard>
        <DCArtboard id="hires-browse"      label="Browse"                  width={W} height={H}><HiRes_Search /></DCArtboard>

        {/* Sound */}
        <DCArtboard id="hires-eq"          label="Equalizer · 10-band"     width={W} height={H}><HiRes_EQ /></DCArtboard>
        <DCArtboard id="hires-sound"       label="Sound Settings"          width={W} height={H}><HiRes_SoundSettings /></DCArtboard>
        <DCArtboard id="hires-output"      label="Output Routing"          width={W} height={H}><HiRes_Output /></DCArtboard>

        {/* Connectivity */}
        <DCArtboard id="hires-bt"          label="Bluetooth · LDAC"        width={W} height={H}><HiRes_Bluetooth /></DCArtboard>
        <DCArtboard id="hires-bt-rx"       label="BT Receiver Mode"        width={W} height={H}><HiRes_BTReceiver /></DCArtboard>
        <DCArtboard id="hires-usb-dac"     label="USB-DAC Mode"            width={W} height={H}><HiRes_UsbDac /></DCArtboard>

        {/* System */}
        <DCArtboard id="hires-settings"    label="Settings"                width={W} height={H}><HiRes_Settings /></DCArtboard>
        <DCArtboard id="hires-reset"       label="Reset / Format"          width={W} height={H}><HiRes_Reset /></DCArtboard>
        <DCArtboard id="hires-volume"      label="Volume popup"            width={W} height={H}><HiRes_Volume /></DCArtboard>
        <DCArtboard id="hires-wizard"      label="Setup Wizard · Sound"    width={W} height={H}><HiRes_Wizard /></DCArtboard>
        <DCArtboard id="hires-lock"        label="Lock · HOLD engaged"     width={W} height={H}><HiRes_Lock /></DCArtboard>
      </DCSection>

      {/* ─── DIR 2 · NOCTURNE ─────────────────────────────── */}
      <DCSection id="nocturne" title="02 · Nocturne" subtitle="Dark editorial — near-black field, serif display, soft violet accent. Generous space; signature visualization is a single thin waveform line.">
        <DCArtboard id="nc-np-hero"        label="Now Playing · Hero"      width={W} height={H}><Nc_NowPlayingHero /></DCArtboard>
        <DCArtboard id="nc-np-dense"       label="Now Playing · Dense"     width={W} height={H}><Nc_NowPlayingDense /></DCArtboard>
        <DCArtboard id="nc-np-meter"       label="Now Playing · Meters"    width={W} height={H}><Nc_NowPlayingMeter /></DCArtboard>
        <DCArtboard id="nc-lyrics"         label="Synced Lyrics"           width={W} height={H}><Nc_Lyrics /></DCArtboard>
        <DCArtboard id="nc-track"          label="Track Info"              width={W} height={H}><Nc_TrackDetail /></DCArtboard>

        <DCArtboard id="nc-library"        label="Library"                 width={W} height={H}><Nc_Library /></DCArtboard>
        <DCArtboard id="nc-queue"          label="Up Next"                 width={W} height={H}><Nc_Queue /></DCArtboard>
        <DCArtboard id="nc-browse"         label="Browse"                  width={W} height={H}><Nc_Search /></DCArtboard>

        <DCArtboard id="nc-eq"             label="Equalizer · 10-band"     width={W} height={H}><Nc_EQ /></DCArtboard>
        <DCArtboard id="nc-sound"          label="Sound Settings"          width={W} height={H}><Nc_SoundSettings /></DCArtboard>
        <DCArtboard id="nc-output"         label="Output Routing"          width={W} height={H}><Nc_Output /></DCArtboard>

        <DCArtboard id="nc-bt"             label="Bluetooth · LDAC"        width={W} height={H}><Nc_Bluetooth /></DCArtboard>
        <DCArtboard id="nc-bt-rx"          label="BT Receiver Mode"        width={W} height={H}><Nc_BTReceiver /></DCArtboard>
        <DCArtboard id="nc-usb-dac"        label="USB-DAC Mode"            width={W} height={H}><Nc_UsbDac /></DCArtboard>

        <DCArtboard id="nc-settings"       label="Settings"                width={W} height={H}><Nc_Settings /></DCArtboard>
        <DCArtboard id="nc-reset"          label="Reset / Format"          width={W} height={H}><Nc_Reset /></DCArtboard>
        <DCArtboard id="nc-volume"         label="Volume popup"            width={W} height={H}><Nc_Volume /></DCArtboard>
        <DCArtboard id="nc-wizard"         label="Setup Wizard · Sound"    width={W} height={H}><Nc_Wizard /></DCArtboard>
        <DCArtboard id="nc-lock"           label="Lock · HOLD engaged"     width={W} height={H}><Nc_Lock /></DCArtboard>
      </DCSection>

      {/* ─── DIR 3 · TERMINAL ────────────────────────────── */}
      <DCSection id="terminal" title="03 · Terminal" subtitle="Retro / brutalist — pixel mono, amber phosphor, ASCII meters. Dense-by-default; everything is a readout.">
        <DCArtboard id="tm-np-hero"        label="Now Playing · Hero"      width={W} height={H}><Tm_NowPlayingHero /></DCArtboard>
        <DCArtboard id="tm-np-dense"       label="Now Playing · Dense"     width={W} height={H}><Tm_NowPlayingDense /></DCArtboard>
        <DCArtboard id="tm-np-meter"       label="Now Playing · Meters"    width={W} height={H}><Tm_NowPlayingMeter /></DCArtboard>
        <DCArtboard id="tm-lyrics"         label="Synced Lyrics"           width={W} height={H}><Tm_Lyrics /></DCArtboard>
        <DCArtboard id="tm-track"          label="Track Info"              width={W} height={H}><Tm_TrackDetail /></DCArtboard>

        <DCArtboard id="tm-library"        label="Library"                 width={W} height={H}><Tm_Library /></DCArtboard>
        <DCArtboard id="tm-queue"          label="Up Next"                 width={W} height={H}><Tm_Queue /></DCArtboard>
        <DCArtboard id="tm-browse"         label="Browse"                  width={W} height={H}><Tm_Search /></DCArtboard>

        <DCArtboard id="tm-eq"             label="Equalizer · 10-band"     width={W} height={H}><Tm_EQ /></DCArtboard>
        <DCArtboard id="tm-sound"          label="Sound Settings"          width={W} height={H}><Tm_SoundSettings /></DCArtboard>
        <DCArtboard id="tm-output"         label="Output Routing"          width={W} height={H}><Tm_Output /></DCArtboard>

        <DCArtboard id="tm-bt"             label="Bluetooth · LDAC"        width={W} height={H}><Tm_Bluetooth /></DCArtboard>
        <DCArtboard id="tm-bt-rx"          label="BT Receiver Mode"        width={W} height={H}><Tm_BTReceiver /></DCArtboard>
        <DCArtboard id="tm-usb-dac"        label="USB-DAC Mode"            width={W} height={H}><Tm_UsbDac /></DCArtboard>

        <DCArtboard id="tm-settings"       label="Settings"                width={W} height={H}><Tm_Settings /></DCArtboard>
        <DCArtboard id="tm-reset"          label="Reset / Format"          width={W} height={H}><Tm_Reset /></DCArtboard>
        <DCArtboard id="tm-volume"         label="Volume popup"            width={W} height={H}><Tm_Volume /></DCArtboard>
        <DCArtboard id="tm-wizard"         label="Setup Wizard · Sound"    width={W} height={H}><Tm_Wizard /></DCArtboard>
        <DCArtboard id="tm-lock"           label="Lock · HOLD engaged"     width={W} height={H}><Tm_Lock /></DCArtboard>
      </DCSection>

      {/* ─── NIGHT MODE (cross-direction) ────────────────── */}
      <DCSection id="night" title="04 · Night Mode" subtitle="Darkest possible — pure black, no album art, quick-access tiles for Bluetooth, Albums, EQ, Volume, Brightness. One per direction.">
        <DCArtboard id="hires-night"       label="Hi-Res · Night Mode"     width={W} height={H}><HiRes_Night /></DCArtboard>
        <DCArtboard id="nc-night"          label="Nocturne · Night Mode"   width={W} height={H}><Nc_Night /></DCArtboard>
        <DCArtboard id="tm-night"          label="Terminal · Night Mode"   width={W} height={H}><Tm_Night /></DCArtboard>
      </DCSection>

      <DCPostIt top={40} left={40}>
        <b>NW-A55 · UI explorations.</b> Three directions × 19 screens each, plus a Night Mode triplet.
        Every artboard is exact device size: <b>480 × 800 px</b> portrait (Sony NW-A55, 3.1″ TFT).
        Screen vocabulary follows Sony's stock firmware (DSEE HX, DC Phase Linearizer, LDAC quality, USB-DAC, BT Receiver, etc.) — lightly rewritten for clarity.
        Click any artboard's expand icon to view fullscreen.
      </DCPostIt>

    </DesignCanvas>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App />);
