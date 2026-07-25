// ────────────────────────────────────────────────────────────────
// finalists-app.jsx — canvas wiring for the three finalist
// directions. Each section: day screens, Shelf sheet, lock screen,
// then the same key screens in Night theme (darker, art dimmed).
// ────────────────────────────────────────────────────────────────

function FinalistsApp() {
  return (
    <DesignCanvas title="NW-A55 · Final Design Candidates">
      <DCSection
        id="cand-a"
        title="A · Cinder — Hi-Res evolved"
        subtitle="Warm amber on near-black. Dense, instrument-like. Night theme = same layouts, pure-black bg, dimmed art, muted accent.">
        <DCArtboard id="a-np" label="Now Playing" width={480} height={800}><CANowPlaying /></DCArtboard>
        <DCArtboard id="a-lib-songs" label="Library · Songs" width={480} height={800}><CALibrarySongs /></DCArtboard>
        <DCArtboard id="a-lib-albums" label="Library · Albums" width={480} height={800}><CALibraryAlbums /></DCArtboard>
        <DCArtboard id="a-lib-artists" label="Library · Artists" width={480} height={800}><CALibraryArtists /></DCArtboard>
        <DCArtboard id="a-artist" label="Artist Page" width={480} height={800}><CAArtistPage /></DCArtboard>
        <DCArtboard id="a-menu" label="Menu" width={480} height={800}><CAMenu /></DCArtboard>
        <DCArtboard id="a-bt" label="Bluetooth" width={480} height={800}><CABluetooth /></DCArtboard>
        <DCArtboard id="a-eq" label="Equalizer" width={480} height={800}><CAEq /></DCArtboard>
        <DCArtboard id="a-shelf" label="Shelf (pin + undo/redo)" width={480} height={800}><CAShelf /></DCArtboard>
        <DCArtboard id="a-lock" label="Lock Screen" width={480} height={800}><CALock /></DCArtboard>
        <DCArtboard id="a-np-n" label="Now Playing · Night" width={480} height={800}><CANowPlaying night /></DCArtboard>
        <DCArtboard id="a-menu-n" label="Menu · Night" width={480} height={800}><CAMenu night /></DCArtboard>
        <DCArtboard id="a-lock-n" label="Lock · Night" width={480} height={800}><CALock night /></DCArtboard>
      </DCSection>

      <DCSection
        id="cand-b"
        title="B · Nocturne — dark editorial"
        subtitle="Instrument Serif display type, lavender accent, generous whitespace. Night theme mutes the lavender and pulls everything to black.">
        <DCArtboard id="b-np" label="Now Playing" width={480} height={800}><CBNowPlaying /></DCArtboard>
        <DCArtboard id="b-menu" label="Menu" width={480} height={800}><CBMenu /></DCArtboard>
        <DCArtboard id="b-bt" label="Bluetooth" width={480} height={800}><CBBluetooth /></DCArtboard>
        <DCArtboard id="b-eq" label="Equalizer" width={480} height={800}><CBEq /></DCArtboard>
        <DCArtboard id="b-shelf" label="Shelf (pin + undo/redo)" width={480} height={800}><CBShelf /></DCArtboard>
        <DCArtboard id="b-lock" label="Lock Screen" width={480} height={800}><CBLock /></DCArtboard>
        <DCArtboard id="b-np-n" label="Now Playing · Night" width={480} height={800}><CBNowPlaying night /></DCArtboard>
        <DCArtboard id="b-menu-n" label="Menu · Night" width={480} height={800}><CBMenu night /></DCArtboard>
        <DCArtboard id="b-lock-n" label="Lock · Night" width={480} height={800}><CBLock night /></DCArtboard>
      </DCSection>

      <DCSection
        id="cand-c"
        title="C · Ledger — terminal evolved"
        subtitle="Departure Mono everywhere, phosphor amber, boxed sections, block-segment progress. Night theme drops to embers-on-black.">
        <DCArtboard id="c-np" label="Now Playing" width={480} height={800}><CCNowPlaying /></DCArtboard>
        <DCArtboard id="c-menu" label="Menu" width={480} height={800}><CCMenu /></DCArtboard>
        <DCArtboard id="c-bt" label="Bluetooth" width={480} height={800}><CCBluetooth /></DCArtboard>
        <DCArtboard id="c-eq" label="Equalizer" width={480} height={800}><CCEq /></DCArtboard>
        <DCArtboard id="c-shelf" label="Shelf (pin + undo/redo)" width={480} height={800}><CCShelf /></DCArtboard>
        <DCArtboard id="c-lock" label="Lock Screen" width={480} height={800}><CCLock /></DCArtboard>
        <DCArtboard id="c-np-n" label="Now Playing · Night" width={480} height={800}><CCNowPlaying night /></DCArtboard>
        <DCArtboard id="c-menu-n" label="Menu · Night" width={480} height={800}><CCMenu night /></DCArtboard>
        <DCArtboard id="c-lock-n" label="Lock · Night" width={480} height={800}><CCLock night /></DCArtboard>
      </DCSection>
    </DesignCanvas>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<FinalistsApp />);
