// ────────────────────────────────────────────────────────────────
// cinder-proto-app.jsx — root of the NW-A55 final prototype.
// State, palette, router (screen stack), device stage with side
// keys, volume overlay, tweaks. Screens live in cinder-proto-*.jsx
// and register themselves on window.CSCREENS.
// Load order: finalists-shared.jsx → cinder-proto-screens*.jsx → this.
// ────────────────────────────────────────────────────────────────

const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "theme": "Day",
  "accent": "#f4651f",
  "viz": true
}/*EDITMODE-END*/;

// mix a hex color toward black (for the night-mode ember accent)
function cMix(hex, k) {
  const n = parseInt(hex.slice(1), 16);
  const f = (s) => Math.round(((n >> s) & 255) * k).toString(16).padStart(2, '0');
  return `#${f(16)}${f(8)}${f(0)}`;
}

function cinderPal(night, acc) {
  if (night) return {
    bg: '#000000', panel: '#0a0908', line: '#161310',
    ink: '#8d8170', dim: '#5b5347', faint: '#3b362d',
    acc: cMix(acc, 0.55), accInk: '#000000', artDim: 0.3, night: true,
    mono: "'JetBrains Mono', monospace", sans: "'Hanken Grotesk', sans-serif",
  };
  return {
    bg: '#0d0c0b', panel: '#13110f', line: '#221f1b',
    ink: '#ece7df', dim: '#95908a', faint: '#5f5a52',
    acc, accInk: '#1a0a02', artDim: 1, night: false,
    mono: "'JetBrains Mono', monospace", sans: "'Hanken Grotesk', sans-serif",
  };
}

const CCtx = React.createContext(null);
const useC = () => React.useContext(CCtx);
// share across Babel script scopes — screens files call useC at render time
Object.assign(window, { useC, CCtx, cinderPal });

function CinderApp() {
  const [t, setTweak] = useTweaks(TWEAK_DEFAULTS);
  const night = t.theme === 'Night';
  const P = cinderPal(night, t.accent);

  // navigation stack — last entry is current screen
  const [stack, setStack] = React.useState(() => {
    try { return JSON.parse(localStorage.getItem('cinder.stack')) || [{ id: 'nowplaying' }]; }
    catch { return [{ id: 'nowplaying' }]; }
  });
  const [locked, setLocked] = React.useState(false);
  const [shelfOpen, setShelfOpen] = React.useState(false);

  // playback
  const [playing, setPlaying] = React.useState(true);
  const [trackIdx, setTrackIdx] = React.useState(0);
  const [pct, setPct] = React.useState(39);
  const [liked, setLiked] = React.useState(true);

  // audio settings
  const [eq, setEq] = React.useState(FBANDS.map((b) => b.db));
  const [eqPreset, setEqPreset] = React.useState('A1');
  const [snd, setSnd] = React.useState({ dsee: true, clearaudio: false, vinyl: true, normalizer: false, vpt: 'Studio', dcphase: 'Standard A' });
  const [bt, setBt] = React.useState({ on: true, connected: 'WH-1000XM5', codec: 'LDAC' });
  const [fm, setFm] = React.useState({ freq: 88.6, preset: 1 });
  const [usbDac, setUsbDac] = React.useState(false);
  const [rx, setRx] = React.useState(false);
  const [vol, setVol] = React.useState(42);
  const [volShow, setVolShow] = React.useState(false);
  const volTimer = React.useRef(null);

  // shelf pins
  const [pins, setPins] = React.useState([
    { title: 'Album · Last Smoke Before…', sub: 'Track 4 · saved 12 min ago' },
    { title: 'Library · Artists · B', sub: 'Saved 1 hr ago' },
    null,
  ]);

  React.useEffect(() => { localStorage.setItem('cinder.stack', JSON.stringify(stack)); }, [stack]);
  React.useEffect(() => {
    if (!playing) return;
    const iv = setInterval(() => setPct((p) => (p >= 100 ? 0 : p + 0.37)), 1000);
    return () => clearInterval(iv);
  }, [playing]);

  const go = (id, params) => { setShelfOpen(false); setStack((s) => [...s.slice(-14), { id, params }]); };
  const back = () => setStack((s) => (s.length > 1 ? s.slice(0, -1) : s));
  const bumpVol = (d) => {
    setVol((v) => Math.max(0, Math.min(120, v + d)));
    setVolShow(true);
    clearTimeout(volTimer.current);
    volTimer.current = setTimeout(() => setVolShow(false), 1400);
  };

  const cur = stack[stack.length - 1];
  const Scr = window.CSCREENS[cur.id] || window.CSCREENS.nowplaying;

  const ctx = {
    P, night, t, setTweak, go, back, cur,
    locked, setLocked, shelfOpen, setShelfOpen,
    playing, setPlaying, trackIdx, setTrackIdx, pct, setPct, liked, setLiked,
    eq, setEq, eqPreset, setEqPreset, snd, setSnd, bt, setBt,
    fm, setFm, usbDac, setUsbDac, rx, setRx, vol, bumpVol,
    pins, setPins,
  };

  return (
    <CCtx.Provider value={ctx}>
      <CinderStage>
        <div style={{
          width: 480, height: 800, background: P.bg, color: P.ink, fontFamily: P.sans,
          display: 'flex', flexDirection: 'column', overflow: 'hidden', userSelect: 'none', position: 'relative',
        }}>
          {locked ? <CLock /> : <Scr params={cur.params} />}
          {!locked && shelfOpen && <CShelfSheet />}
          {volShow && (
            <div style={{
              position: 'absolute', top: 44, left: 90, right: 90, height: 44, background: P.panel,
              border: `1px solid ${P.line}`, display: 'flex', alignItems: 'center', gap: 12, padding: '0 14px', zIndex: 40,
            }}>
              <FISound size={16} />
              <div style={{ flex: 1, height: 3, background: P.line, position: 'relative' }}>
                <div style={{ position: 'absolute', inset: '0 auto 0 0', width: `${(vol / 120) * 100}%`, background: P.acc }}></div>
              </div>
              <span style={{ fontFamily: P.mono, fontSize: 11, color: P.dim }}>{vol}</span>
            </div>
          )}
        </div>
      </CinderStage>

      <TweaksPanel>
        <TweakSection label="Theme" />
        <TweakRadio label="Mode" value={t.theme} options={['Day', 'Night']} onChange={(v) => setTweak('theme', v)} />
        <TweakColor label="Accent" value={t.accent} options={['#f4651f', '#ff7a33', '#e0a43c']} onChange={(v) => setTweak('accent', v)} />
        <TweakSection label="Now Playing" />
        <TweakToggle label="Visualizer" value={t.viz} onChange={(v) => setTweak('viz', v)} />
      </TweaksPanel>
    </CCtx.Provider>
  );
}

// ─── device stage: scaled 480×800 canvas + side-key rail ───────
function CinderStage({ children }) {
  const [scale, setScale] = React.useState(1);
  const W = 480 + 64, H = 800; // rail width included
  React.useEffect(() => {
    const fit = () => setScale(Math.min(window.innerWidth / (W + 32), window.innerHeight / (H + 32), 1.1));
    fit(); window.addEventListener('resize', fit);
    return () => window.removeEventListener('resize', fit);
  }, []);
  return (
    <div style={{ position: 'fixed', inset: 0, background: '#050505', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
      <div style={{ transform: `scale(${scale})`, display: 'flex', alignItems: 'center' }}>
        <div style={{ border: '1px solid #1c1c1c', boxShadow: '0 0 0 6px #000', overflow: 'hidden' }}>{children}</div>
        <CSideKeys />
      </div>
    </div>
  );
}

function CSideKeys() {
  const c = useC();
  const key = (label, onTap, tall) => (
    <div onClick={onTap} style={{
      width: 30, height: tall ? 72 : 42, background: '#111', border: '1px solid #222',
      display: 'flex', alignItems: 'center', justifyContent: 'center', cursor: 'pointer',
      color: '#666', fontFamily: "'JetBrains Mono', monospace", fontSize: 8, letterSpacing: '.04em',
      writingMode: 'vertical-rl',
    }}>{label}</div>
  );
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10, marginLeft: 10 }}>
      {key('PWR', () => c.setLocked((l) => !l))}
      {key('VOL+', () => c.bumpVol(2))}
      {key('VOL−', () => c.bumpVol(-2))}
      {key('⏮', () => c.setTrackIdx((i) => (i + CAL_SONGS.length - 1) % CAL_SONGS.length))}
      {key('⏯', () => c.setPlaying((p) => !p), true)}
      {key('⏭', () => c.setTrackIdx((i) => (i + 1) % CAL_SONGS.length))}
    </div>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<CinderApp />);
