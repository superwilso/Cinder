// ────────────────────────────────────────────────────────────────
// player-screens.jsx — interactive route screens for the NW-A55.
// All themed via usePlayer()/theme; all wired to state + dispatch.
// ────────────────────────────────────────────────────────────────

// ─── LIBRARY + ALBUM live in library.jsx (loaded after this file) ──

// ─── QUEUE ──────────────────────────────────────────────────────
function QueueScreen() {
  const { state, dispatch, theme } = usePlayer();
  return (
    <ScreenShell>
      <StatusBarX />
      <Header title="Up Next" right={<span style={{ fontFamily: theme.fontMono, fontSize: 10, color: theme.dim }}>{tx(theme, `${TRACKS.length} · 41:24`)}</span>} />
      {TRACKS.map((t, i) => {
        const cur = i === state.trackIdx;
        return (
          <button key={t.id} onClick={() => dispatch({ type: 'PICK_TRACK', i })} style={{
            display: 'grid', gridTemplateColumns: '26px 40px 1fr auto', gap: 12, alignItems: 'center',
            width: '100%', padding: '10px 20px', background: cur ? hexA(theme.accent, .07) : 'none',
            border: 'none', borderBottom: `1px solid ${theme.rule}`, cursor: 'pointer', color: 'inherit', textAlign: 'left',
          }}>
            <span style={{ fontFamily: theme.fontMono, fontSize: 10, color: cur ? theme.accent : theme.dim }}>{cur ? '▶' : String(i + 1).padStart(2, '0')}</span>
            <Art kind={t.art} size={40} label={false} />
            <span style={{ minWidth: 0 }}>
              <span style={{ display: 'block', fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody, fontSize: theme.serif ? 16 : 14, color: cur ? theme.accent : theme.text, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, t.title)}</span>
              <span style={{ display: 'block', fontSize: 11, color: theme.dim, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, t.artist)}</span>
            </span>
            <span style={{ fontFamily: theme.fontMono, fontSize: 10, color: theme.dim }}>{t.dur}</span>
          </button>
        );
      })}
    </ScreenShell>
  );
}

// ─── BROWSE ─────────────────────────────────────────────────────
function BrowseScreen() {
  const { dispatch, theme } = usePlayer();
  const [cat, setCat] = useState('Albums');
  const cats = [['Artists', 64], ['Albums', 124], ['Genres', 11], ['Composers', 18], ['Years', 22], ['Folders', 7]];
  const letters = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ#'.split('');
  const has = new Set(['A', 'B', 'F', 'H', 'I', 'K', 'M', 'N', 'P', 'S', 'T']);
  const [sel, setSel] = useState('N');
  return (
    <ScreenShell>
      <StatusBarX />
      <Header title="Browse" />
      <SectionLabel>Browse by</SectionLabel>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3,1fr)', gap: 6, padding: '0 20px' }}>
        {cats.map(([n, c]) => {
          const active = cat === n;
          return (
            <button key={n} onClick={() => setCat(n)} style={{
              padding: '12px 12px', background: active ? theme.accent : theme.panel2,
              color: active ? theme.onAccent : theme.text, border: `1px solid ${active ? theme.accent : theme.rule}`,
              borderRadius: theme.radius, cursor: 'pointer', textAlign: 'left',
            }}>
              <div style={{ fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody, fontSize: 14 }}>{tx(theme, n)}</div>
              <div style={{ fontFamily: theme.fontMono, fontSize: 9, opacity: .7, marginTop: 3 }}>{c}</div>
            </button>
          );
        })}
      </div>
      <SectionLabel>Jump to</SectionLabel>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(9,1fr)', gap: 3, padding: '0 20px' }}>
        {letters.map(l => {
          const can = has.has(l), s = l === sel;
          return (
            <button key={l} disabled={!can} onClick={() => setSel(l)} style={{
              aspectRatio: '1', background: s ? theme.accent : 'transparent',
              color: s ? theme.onAccent : (can ? theme.text : theme.faint),
              border: `1px solid ${s ? theme.accent : (can ? theme.rule : 'transparent')}`,
              fontFamily: theme.fontMono, fontSize: 12, cursor: can ? 'pointer' : 'default', borderRadius: theme.radius,
            }}>{l}</button>
          );
        })}
      </div>
      <SectionLabel>{sel} · Artists</SectionLabel>
      {[['Neil Young', 'harvest', '6 albums'], ['Nils Frahm', 'ferns', '4 albums'], ['Nicolas Jaar', 'midnight', '2 albums']].map(([n, art, m]) => (
        <Row key={n} icon={<Art kind={art} size={36} label={false} />} label={n} sub={m} onClick={() => dispatch({ type: 'NAV', route: 'library' })} />
      ))}
    </ScreenShell>
  );
}

// ─── EQUALIZER (interactive bands) ──────────────────────────────
function EqScreen() {
  const { state, dispatch, theme } = usePlayer();
  const presets = ['Off', 'A1', 'Heavy', 'Pop', 'Jazz', 'Vocal', 'Custom'];
  const setBand = (i, e) => {
    const r = e.currentTarget.getBoundingClientRect();
    const rel = 1 - (e.clientY - r.top) / r.height;
    dispatch({ type: 'SET_EQ_BAND', i, db: Math.round(rel * 24 - 12) });
  };
  return (
    <ScreenShell>
      <StatusBarX />
      <Header title="Equalizer" right={<span style={{ fontFamily: theme.fontMono, fontSize: 10, color: theme.accent }}>{tx(theme, state.eqPreset)}</span>} />
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, padding: '0 20px 4px' }}>
        {presets.map(p => {
          const a = state.eqPreset === p;
          return (
            <button key={p} onClick={() => dispatch({ type: 'SET_EQ_PRESET', name: p })} style={{
              padding: '6px 12px', borderRadius: theme.radius === 0 ? 0 : 999,
              border: `1px solid ${a ? theme.accent : theme.rule}`, background: a ? hexA(theme.accent, .1) : 'transparent',
              color: a ? theme.accent : theme.text, fontFamily: theme.fontMono, fontSize: 10, cursor: 'pointer', letterSpacing: '.06em',
            }}>{tx(theme, p)}</button>
          );
        })}
      </div>
      <SectionLabel style={{ marginBottom: 2 }}>Tap a band to set gain</SectionLabel>
      <div style={{ display: 'flex', alignItems: 'stretch', height: 360, padding: '8px 16px 0', gap: 2 }}>
        {EQ_BANDS.map((b, i) => {
          const db = state.eqBands[i];
          const pct = (db + 12) / 24;
          return (
            <div key={b.hz} style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center' }}>
              <div style={{ fontFamily: theme.fontMono, fontSize: 9, color: theme.accent, marginBottom: 4 }}>{db > 0 ? '+' : ''}{db}</div>
              <div onClick={(e) => setBand(i, e)} style={{ flex: 1, width: '100%', position: 'relative', cursor: 'pointer' }}>
                <div style={{ position: 'absolute', left: '50%', top: 0, bottom: 0, width: 2, background: theme.faint, transform: 'translateX(-50%)' }} />
                <div style={{ position: 'absolute', left: '50%', top: '50%', width: 14, height: 2, background: theme.rule, transform: 'translate(-50%,-50%)' }} />
                <div style={{ position: 'absolute', left: '50%', bottom: `calc(${pct * 100}% - 7px)`, width: 14, height: 14, borderRadius: theme.radius === 0 ? 0 : '50%', background: theme.bg, border: `2px solid ${theme.accent}`, transform: 'translateX(-50%)' }} />
              </div>
              <div style={{ fontFamily: theme.fontMono, fontSize: 8, color: theme.dim, marginTop: 6 }}>{b.hz}</div>
            </div>
          );
        })}
      </div>
      <Row label="DC Phase Linearizer" value="Type A · Low" onClick={() => {}} />
      <Row label="DSEE HX" toggle on={state.flags.dseeHX} onClick={() => dispatch({ type: 'TOGGLE_FLAG', key: 'dseeHX' })} />
    </ScreenShell>
  );
}

// ─── SOUND SETTINGS ─────────────────────────────────────────────
function SoundScreen() {
  const { state, dispatch, theme } = usePlayer();
  return (
    <ScreenShell>
      <StatusBarX />
      <Header title="Sound Settings" />
      <SectionLabel>EQ / Tone</SectionLabel>
      <Row label="Equalizer" value={state.eqPreset} onClick={() => dispatch({ type: 'NAV', route: 'eq' })} />
      <Row label="Tone Control" value="Off" onClick={() => {}} />
      <Row label="DSEE HX" toggle on={state.flags.dseeHX} onClick={() => dispatch({ type: 'TOGGLE_FLAG', key: 'dseeHX' })} />
      <SectionLabel>Surround / Source</SectionLabel>
      <Row label="VPT (Surround)" value="Studio" onClick={() => {}} />
      <Row label="Dynamic Normalizer" toggle on={state.flags.dynamicNormalizer} onClick={() => dispatch({ type: 'TOGGLE_FLAG', key: 'dynamicNormalizer' })} />
      <Row label="Vinyl Processor" toggle on={state.flags.vinyl} onClick={() => dispatch({ type: 'TOGGLE_FLAG', key: 'vinyl' })} />
      <SectionLabel>Analog</SectionLabel>
      <Row label="DC Phase Linearizer" toggle on={state.flags.dcPhase} onClick={() => dispatch({ type: 'TOGGLE_FLAG', key: 'dcPhase' })} />
      <Row label="L/R Balance" value="Center" onClick={() => {}} />
    </ScreenShell>
  );
}

// ─── OUTPUT ─────────────────────────────────────────────────────
function OutputScreen() {
  const { state, dispatch, theme } = usePlayer();
  const [dest, setDest] = useState('mini');
  const opts = [
    { id: 'mini', name: '3.5 mm Stereo Mini', sub: 'Onkyo IE-FC300 detected', ok: true },
    { id: 'bal', name: '4.4 mm Balanced', sub: 'Not on this model', ok: false },
    { id: 'bt', name: 'Bluetooth', sub: `${state.btConnected} · LDAC`, ok: true },
    { id: 'usb', name: 'USB Audio', sub: 'No host connected', ok: false },
  ];
  return (
    <ScreenShell>
      <StatusBarX />
      <Header title="Output" />
      <SectionLabel>Destination</SectionLabel>
      {opts.map(o => (
        <button key={o.id} disabled={!o.ok} onClick={() => setDest(o.id)} style={{
          display: 'flex', gap: 12, width: '100%', textAlign: 'left', alignItems: 'flex-start',
          padding: '12px 20px', background: 'none', border: 'none', borderBottom: `1px solid ${theme.rule}`,
          cursor: o.ok ? 'pointer' : 'default', opacity: o.ok ? 1 : .4, color: 'inherit',
        }}>
          <RadioDot on={dest === o.id} theme={theme} />
          <span style={{ flex: 1 }}>
            <span style={{ display: 'block', fontSize: 14, color: dest === o.id ? theme.accent : theme.text, fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody }}>{tx(theme, o.name)}</span>
            <span style={{ display: 'block', fontSize: 11, color: theme.dim, marginTop: 2 }}>{tx(theme, o.sub)}</span>
          </span>
        </button>
      ))}
      <SectionLabel>Gain</SectionLabel>
      <Row label="High Gain · Stereo Mini" toggle on={state.flags.highGainMini} onClick={() => dispatch({ type: 'TOGGLE_FLAG', key: 'highGainMini' })} />
      <Row label="High Gain · Balanced" toggle on={state.flags.highGainBal} onClick={() => dispatch({ type: 'TOGGLE_FLAG', key: 'highGainBal' })} />
      <div style={{ padding: '12px 20px', fontSize: 11, color: theme.dim, lineHeight: 1.5, fontStyle: theme.serif ? 'italic' : 'normal' }}>
        {tx(theme, 'High Gain raises output by ~6 dB for low-sensitivity headphones. May raise the noise floor.')}
      </div>
    </ScreenShell>
  );
}
function RadioDot({ on, theme }) {
  return <span style={{ width: 18, height: 18, borderRadius: '50%', border: `1.5px solid ${on ? theme.accent : theme.dim}`, display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0, marginTop: 1 }}>{on && <span style={{ width: 8, height: 8, borderRadius: '50%', background: theme.accent }} />}</span>;
}

// ─── BLUETOOTH ──────────────────────────────────────────────────
function BluetoothScreen() {
  const { state, dispatch, theme } = usePlayer();
  return (
    <ScreenShell>
      <StatusBarX right={<IconBluetooth size={12} style={{ color: theme.accent }} />} />
      <Header title="Bluetooth" right={<Toggle on={true} onClick={() => {}} />} />
      <div style={{ margin: '0 20px', padding: '14px 16px', background: hexA(theme.accent, .06), border: `1px solid ${theme.accent}`, borderRadius: theme.radius }}>
        <div style={{ fontFamily: theme.fontMono, fontSize: 9, letterSpacing: '.14em', color: theme.accent }}>{tx(theme, 'Connected')}</div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginTop: 6 }}>
          <IconHeadphone size={20} style={{ color: theme.accent }} />
          <div>
            <div style={{ fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody, fontSize: 17 }}>{state.btConnected}</div>
            <div style={{ fontFamily: theme.fontMono, fontSize: 10, color: theme.dim, marginTop: 2 }}>LDAC · {tx(theme, state.ldac)} · 92%</div>
          </div>
        </div>
      </div>
      <SectionLabel>Wireless Playback Quality</SectionLabel>
      {LDAC_QUALITY.map(q => (
        <button key={q.label} onClick={() => dispatch({ type: 'SET_LDAC', name: q.label })} style={{
          display: 'flex', gap: 12, width: '100%', textAlign: 'left', alignItems: 'flex-start',
          padding: '11px 20px', background: 'none', border: 'none', borderBottom: `1px solid ${theme.rule}`, cursor: 'pointer', color: 'inherit',
        }}>
          <RadioDot on={state.ldac === q.label} theme={theme} />
          <span style={{ flex: 1 }}>
            <span style={{ display: 'block', fontSize: 14, color: state.ldac === q.label ? theme.accent : theme.text, fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody }}>{tx(theme, q.label)}</span>
            <span style={{ display: 'block', fontSize: 11, color: theme.dim, marginTop: 2 }}>{tx(theme, q.sub)}</span>
          </span>
        </button>
      ))}
      <SectionLabel>Paired devices</SectionLabel>
      {BT_DEVICES.filter(d => d.name !== state.btConnected).map(d => (
        <Row key={d.name} icon={<IconHeadphone size={15} />} label={d.name} sub={`${d.kind} · ${d.codec}`}
          value={`${d.rssi}/4`} onClick={() => dispatch({ type: 'BT_CONNECT', name: d.name })} />
      ))}
    </ScreenShell>
  );
}

// ─── BT RECEIVER ────────────────────────────────────────────────
function BtRxScreen() {
  const { theme } = usePlayer();
  return (
    <ScreenShell>
      <StatusBarX badge="RX" />
      <Header title="BT Receiver" />
      <div style={{ textAlign: 'center', padding: '20px 24px 8px' }}>
        <div style={{ fontFamily: theme.fontMono, fontSize: 9, letterSpacing: '.18em', color: theme.accent }}>{tx(theme, 'Broadcasting as')}</div>
        <div style={{ fontFamily: theme.fontDisplay, fontSize: 42, lineHeight: 1, marginTop: 6 }}>NW-A55</div>
        <div style={{ fontSize: 13, color: theme.dim, marginTop: 16, fontStyle: theme.serif ? 'italic' : 'normal' }}>{tx(theme, 'receiving from')}</div>
        <div style={{ fontFamily: theme.fontDisplay, fontSize: 24, marginTop: 2 }}>iPhone 15 Pro</div>
        <div style={{ height: 70, marginTop: 16 }}>
          <VizCanvas kind="bars" palette={{ accent: theme.accent, hot: theme.hot }} width={432} height={70} />
        </div>
        <div style={{ fontFamily: theme.fontMono, fontSize: 10, color: theme.accent, marginTop: 8 }}>{tx(theme, 'RX · LDAC · 990 kbps')}</div>
      </div>
      <SectionLabel>Source devices</SectionLabel>
      <Row label="iPhone 15 Pro" value="Connected" accent onClick={() => {}} />
      <Row label="MacBook Air M3" value="Paired" onClick={() => {}} />
      <div style={{ margin: '14px 20px', padding: '12px', border: `1px dashed ${theme.rule}`, fontSize: 11, color: theme.dim, lineHeight: 1.5, fontStyle: theme.serif ? 'italic' : 'normal' }}>
        {tx(theme, 'Sound Settings do not apply while the Walkman is acting as a Bluetooth receiver.')}
      </div>
    </ScreenShell>
  );
}

// ─── USB-DAC ────────────────────────────────────────────────────
function UsbDacScreen() {
  const { state, dispatch, theme } = usePlayer();
  return (
    <ScreenShell>
      <StatusBarX badge="USB-DAC" right={<span style={{ fontFamily: theme.fontMono, fontSize: 9 }}>RX · PCM</span>} />
      <Header title="USB-DAC" />
      <div style={{ textAlign: 'center', padding: '10px 0 0' }}>
        <div style={{ fontFamily: theme.fontMono, fontSize: 9, letterSpacing: '.18em', color: theme.dim }}>{tx(theme, 'Sample Rate')}</div>
        <div style={{ fontFamily: theme.fontDisplay, fontSize: 84, lineHeight: 1, color: theme.accent, letterSpacing: '-.03em', marginTop: 4 }}>96.0</div>
        <div style={{ fontFamily: theme.fontMono, fontSize: 12, color: theme.dim, marginTop: -2 }}>{tx(theme, 'kHz · 24-bit · PCM')}</div>
      </div>
      <SectionLabel>Receive level</SectionLabel>
      <div style={{ height: 80, padding: '0 20px' }}>
        <VizCanvas kind="mirror" palette={{ accent: theme.accent, hot: theme.hot }} width={440} height={80} />
      </div>
      <SectionLabel>USB-DAC settings</SectionLabel>
      <Row label="DAC Filter" value="Slow Roll-off" onClick={() => {}} />
      <Row label="Charge from connected device" toggle on={state.flags.chargeFromHost} onClick={() => dispatch({ type: 'TOGGLE_FLAG', key: 'chargeFromHost' })} />
      <Row label="DSD over PCM (DoP)" toggle on={state.flags.dacDoP} onClick={() => dispatch({ type: 'TOGGLE_FLAG', key: 'dacDoP' })} />
    </ScreenShell>
  );
}

// ─── SETTINGS ───────────────────────────────────────────────────
function SettingsScreen() {
  const { state, dispatch, theme } = usePlayer();
  const navMap = { 'Sound Settings': 'sound', 'Bluetooth': 'bt', 'BT Receiver': 'btrx', 'USB Connection': 'usbdac', 'Reset / Format': 'reset' };
  return (
    <ScreenShell>
      <StatusBarX />
      <Header title="Settings" />
      {SETTINGS.map(grp => (
        <React.Fragment key={grp.group}>
          <SectionLabel>{grp.group}</SectionLabel>
          {grp.items.map((it, i) => {
            if (it.type === 'toggle') {
              const key = it.label === 'High Gain Output' ? 'highGainMini' : it.label === 'BT Receiver' ? 'btRx' : 'avls';
              return <Row key={i} label={it.label} toggle on={state.flags[key]} onClick={() => dispatch({ type: 'TOGGLE_FLAG', key })} />;
            }
            const route = navMap[it.label];
            return <Row key={i} label={it.label} value={it.value} onClick={() => route ? dispatch({ type: 'NAV', route }) : null} />;
          })}
        </React.Fragment>
      ))}
    </ScreenShell>
  );
}

// ─── RESET / FORMAT ─────────────────────────────────────────────
function ResetScreen() {
  const { theme } = usePlayer();
  const [confirm, setConfirm] = useState(null);
  return (
    <ScreenShell>
      <StatusBarX />
      <Header title="Reset / Format" />
      {RESET_ITEMS.map(r => (
        <button key={r.label} onClick={() => setConfirm(confirm === r.label ? null : r.label)} style={{
          display: 'block', width: '100%', textAlign: 'left', padding: '13px 20px', background: confirm === r.label ? hexA(theme.hot, .08) : 'none',
          border: 'none', borderBottom: `1px solid ${theme.rule}`, cursor: 'pointer', color: 'inherit',
        }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <span style={{ fontSize: 14, fontWeight: 500, color: r.destructive ? theme.hot : theme.text, fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody }}>{tx(theme, r.label)}</span>
            <IconChevron size={13} style={{ color: theme.dim }} />
          </div>
          <div style={{ fontSize: 11, color: theme.dim, marginTop: 4, lineHeight: 1.4 }}>{tx(theme, r.desc)}</div>
          {confirm === r.label && (
            <div style={{ marginTop: 10, display: 'flex', gap: 8 }}>
              <span style={{ padding: '7px 14px', border: `1px solid ${theme.rule}`, fontSize: 11, fontFamily: theme.fontMono }}>{tx(theme, 'Cancel')}</span>
              <span style={{ padding: '7px 14px', background: r.destructive ? theme.hot : theme.accent, color: r.destructive ? '#fff' : theme.onAccent, fontSize: 11, fontFamily: theme.fontMono }}>{tx(theme, r.destructive ? 'Erase' : 'Confirm')}</span>
            </div>
          )}
        </button>
      ))}
    </ScreenShell>
  );
}

// ─── SETUP WIZARD ───────────────────────────────────────────────
function WizardScreen() {
  const { state, dispatch, theme } = usePlayer();
  const feats = [
    { key: 'dseeHX', label: 'DSEE HX', sub: 'Restore high-frequency detail to MP3/AAC.' },
    { key: 'dcPhase', label: 'DC Phase Linearizer', sub: 'Emulate analog-amp phase response.' },
    { key: 'dynamicNormalizer', label: 'Dynamic Normalizer', sub: 'Even out loudness across tracks.' },
  ];
  return (
    <ScreenShell>
      <StatusBarX badge="SETUP" />
      <div style={{ display: 'flex', justifyContent: 'space-between', fontFamily: theme.fontMono, fontSize: 9, padding: '4px 20px 0', color: theme.dim }}>
        {WIZARD_STEPS.map((s, i) => (
          <div key={s.key} style={{ flex: 1, textAlign: 'center', color: s.done ? theme.accent : (i === 2 ? theme.text : theme.dim) }}>
            <div style={{ height: 2, background: s.done ? theme.accent : (i === 2 ? theme.text : theme.faint), marginBottom: 5 }} />
            0{s.n} · {tx(theme, s.label)}
          </div>
        ))}
      </div>
      <div style={{ padding: '20px 24px 0' }}>
        <div style={{ fontFamily: theme.fontDisplay, fontSize: theme.serif ? 40 : 26, fontWeight: theme.serif ? 400 : 700, lineHeight: 1.05 }}>{tx(theme, 'High-Quality Sound')}</div>
        <div style={{ fontSize: 13, color: theme.dim, marginTop: 10, lineHeight: 1.5, fontStyle: theme.serif ? 'italic' : 'normal' }}>
          {tx(theme, 'Turn these on for the recommended out-of-box sound. You can change them any time in Sound Settings.')}
        </div>
      </div>
      <div style={{ marginTop: 12 }}>
        {feats.map(f => (
          <Row key={f.key} label={f.label} sub={f.sub} toggle on={state.flags[f.key]} onClick={() => dispatch({ type: 'TOGGLE_FLAG', key: f.key })} />
        ))}
      </div>
      <div style={{ display: 'flex', gap: 8, padding: '20px 24px' }}>
        <button onClick={() => dispatch({ type: 'HOME' })} style={{ flex: 1, padding: 13, background: 'none', border: `1px solid ${theme.rule}`, color: theme.text, fontFamily: theme.fontMono, fontSize: 12, cursor: 'pointer', borderRadius: theme.radius }}>{tx(theme, 'Skip')}</button>
        <button onClick={() => dispatch({ type: 'HOME' })} style={{ flex: 2, padding: 13, background: theme.accent, border: 'none', color: theme.onAccent, fontFamily: theme.fontMono, fontWeight: 600, fontSize: 12, cursor: 'pointer', borderRadius: theme.radius }}>{tx(theme, 'Continue →')}</button>
      </div>
    </ScreenShell>
  );
}

// ─── TRACK INFO ─────────────────────────────────────────────────
function TrackScreen() {
  const { state, dispatch, theme } = usePlayer();
  const t = TRACKS[state.trackIdx];
  const isFav = !!state.fav[t.id];
  const rows = [
    ['Codec', t.codec === 'DSD' ? 'DSD · DSF' : t.codec],
    ['Sample rate', t.rate],
    ['Bit depth', t.codec === 'DSD' ? '1-bit' : `${t.bits}-bit`],
    ['Bitrate', `${t.bitrate} kbps`],
    ['Channels', '2 · Stereo'],
  ];
  return (
    <ScreenShell>
      <StatusBarX />
      <Header title="Track Info" right={<ActionBtn theme={theme} active={isFav} onClick={() => dispatch({ type: 'FAV' })}>{isFav ? <IconHeartFill size={18} /> : <IconHeart size={18} />}</ActionBtn>} />
      <div style={{ display: 'flex', gap: 16, padding: '0 20px 14px' }}>
        <Art kind={t.art} size={110} label={false} />
        <div style={{ minWidth: 0 }}>
          <div style={{ fontFamily: theme.fontDisplay, fontSize: theme.serif ? 22 : 17, fontWeight: theme.serif ? 400 : 600, lineHeight: 1.1 }}>{tx(theme, t.title)}</div>
          <div style={{ fontSize: 12, color: theme.dim, marginTop: 4, fontStyle: theme.serif ? 'italic' : 'normal' }}>{tx(theme, t.artist)}</div>
          <div style={{ fontFamily: theme.fontMono, fontSize: 9, color: theme.dim, marginTop: 4 }}>{tx(theme, t.album)}</div>
        </div>
      </div>
      <SectionLabel>Format</SectionLabel>
      {rows.map(([k, v]) => (
        <div key={k} style={{ display: 'flex', justifyContent: 'space-between', padding: '9px 20px', borderBottom: `1px solid ${theme.rule}` }}>
          <span style={{ fontSize: 13, color: theme.dim }}>{tx(theme, k)}</span>
          <span style={{ fontFamily: theme.fontMono, fontSize: 11 }}>{tx(theme, v)}</span>
        </div>
      ))}
      <SectionLabel>File</SectionLabel>
      <div style={{ padding: '0 20px', fontFamily: theme.fontMono, fontSize: 10, color: theme.dim, wordBreak: 'break-all', lineHeight: 1.5 }}>
        /Music/{t.artist}/{t.album}/03 {t.title}.flac
      </div>
      <div style={{ display: 'flex', justifyContent: 'space-between', padding: '9px 20px', marginTop: 8, borderTop: `1px solid ${theme.rule}` }}>
        <span style={{ fontSize: 13, color: theme.dim }}>{tx(theme, 'Size')}</span>
        <span style={{ fontFamily: theme.fontMono, fontSize: 11 }}>84.2 MB · SD Card</span>
      </div>
    </ScreenShell>
  );
}

// ─── SYNCED LYRICS ──────────────────────────────────────────────
function parseT(s) { const [m, x] = s.split(':').map(Number); return m * 60 + x; }
function LyricsScreen() {
  const { state, theme, dispatch } = usePlayer();
  const t = TRACKS[state.trackIdx];
  const cur = LYRICS.reduce((acc, l, i) => parseT(l.t) <= state.posSec ? i : acc, -1);
  return (
    <ScreenShell>
      <StatusBarX badge="Synced Lyrics" />
      <Header title="Lyrics" />
      <div style={{ padding: '4px 28px 80px' }}>
        <div style={{ fontFamily: theme.fontDisplay, fontSize: theme.serif ? 22 : 16, marginBottom: 18 }}>{tx(theme, t.title)} <span style={{ color: theme.dim, fontSize: 13 }}>· {tx(theme, t.artist)}</span></div>
        {LYRICS.map((l, i) => {
          const isCur = i === cur;
          const dist = Math.abs(i - cur);
          const op = isCur ? 1 : Math.max(0.22, 0.7 - dist * 0.12);
          return (
            <div key={i} style={{
              fontFamily: isCur ? theme.fontDisplay : (theme.serif ? theme.fontDisplay : theme.fontBody),
              fontSize: isCur ? 24 : 17, fontStyle: isCur && theme.serif ? 'italic' : 'normal',
              lineHeight: 1.4, marginBottom: 16, opacity: op, color: isCur ? theme.accent : theme.text, transition: 'all .3s',
            }}>
              <span style={{ fontFamily: theme.fontMono, fontSize: 9, color: theme.dim, marginRight: 10 }}>{l.t}</span>
              {tx(theme, l.line)}
            </div>
          );
        })}
      </div>
      <div style={{ position: 'absolute', left: 0, right: 0, bottom: 0, padding: '12px 24px', background: hexA(theme.bg, .9), borderTop: `1px solid ${theme.rule}`, display: 'flex', alignItems: 'center', gap: 16, justifyContent: 'center' }}>
        <button onClick={() => dispatch({ type: 'PREV' })} style={iconBtn(theme)}><IconPrev size={20} /></button>
        <button onClick={() => dispatch({ type: 'PLAY' })} style={{ width: 40, height: 40, borderRadius: '50%', border: `1px solid ${theme.accent}`, background: 'none', color: theme.accent, cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>{state.playing ? <IconPause size={18} /> : <IconPlay size={18} />}</button>
        <button onClick={() => dispatch({ type: 'NEXT' })} style={iconBtn(theme)}><IconNext size={20} /></button>
      </div>
    </ScreenShell>
  );
}

// ─── NIGHT MODE ─────────────────────────────────────────────────
function NightScreen() {
  const { state, dispatch, theme } = usePlayer();
  const t = TRACKS[state.trackIdx];
  const go = (key) => {
    if (key === 'vol') dispatch({ type: 'VOL', delta: 0 });
    else if (key === 'bright') {}
    else dispatch({ type: 'NAV', route: key === 'lib' ? 'library' : key });
  };
  return (
    <div style={{ position: 'absolute', inset: 0, background: '#000', color: theme.text, fontFamily: theme.fontBody, overflow: 'auto' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', padding: '12px 20px', fontFamily: theme.fontMono, fontSize: 10, color: theme.accent }}>
        <span>14:32 · {tx(theme, 'NIGHT')}</span>
        <button onClick={() => dispatch({ type: 'HOME' })} style={{ background: 'none', border: 'none', color: theme.accent, fontFamily: theme.fontMono, fontSize: 10, cursor: 'pointer' }}>{tx(theme, 'EXIT ✕')}</button>
      </div>
      <div style={{ padding: '14px 24px 0' }}>
        <div style={{ fontFamily: theme.fontDisplay, fontSize: 92, lineHeight: 1, color: theme.accent, letterSpacing: '-.03em' }}>14:32</div>
        <div style={{ fontSize: 13, color: theme.dim, marginTop: 4, fontStyle: theme.serif ? 'italic' : 'normal' }}>{tx(theme, 'Thursday, 27 May')}</div>
      </div>
      <div style={{ padding: '18px 24px 0' }}>
        <div style={{ fontFamily: theme.fontMono, fontSize: 9, letterSpacing: '.16em', color: theme.accent }}>{tx(theme, 'NOW PLAYING')}</div>
        <div style={{ fontFamily: theme.fontDisplay, fontSize: 20, marginTop: 4 }}>{tx(theme, t.title)}</div>
        <div style={{ fontSize: 12, color: theme.dim, marginTop: 2 }}>{tx(theme, t.artist)}</div>
      </div>
      <div style={{ padding: '20px 24px 24px' }}>
        <div style={{ fontFamily: theme.fontMono, fontSize: 9, letterSpacing: '.16em', color: theme.accent, marginBottom: 10 }}>{tx(theme, 'QUICK ACCESS')}</div>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
          {NIGHT_TILES.map(tile => (
            <button key={tile.key} onClick={() => go(tile.key)} style={{
              padding: '14px', border: `1px solid ${hexA(theme.accent, .25)}`, background: hexA(theme.accent, .04),
              cursor: 'pointer', textAlign: 'left', color: 'inherit', borderRadius: theme.radius,
            }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, color: theme.accent }}>
                {tile.key === 'bt' && <IconBluetooth size={14} />}
                {tile.key === 'lib' && <IconGrid size={14} />}
                {tile.key === 'queue' && <IconQueue size={14} />}
                {tile.key === 'eq' && <IconSlider size={14} />}
                {tile.key === 'vol' && <IconVolume size={14} />}
                {tile.key === 'bright' && <span style={{ fontSize: 14 }}>☼</span>}
                <span style={{ fontFamily: theme.fontMono, fontSize: 10, letterSpacing: '.1em', textTransform: 'uppercase' }}>{tile.label}</span>
              </div>
              <div style={{ fontSize: 13, marginTop: 6, color: theme.dim }}>{tx(theme, tile.sub)}</div>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

Object.assign(window, {
  QueueScreen, BrowseScreen, EqScreen, SoundScreen, OutputScreen,
  BluetoothScreen, BtRxScreen, UsbDacScreen, SettingsScreen, ResetScreen,
  WizardScreen, TrackScreen, LyricsScreen, NightScreen, RadioDot,
});
