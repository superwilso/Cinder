// ────────────────────────────────────────────────────────────────
// player.jsx — interactive NW-A55.
// Theme system (3 skins) · state reducer · nav router · device
// hardware frame · shared themed primitives · Now Playing · Menu.
// Secondary screens live in player-screens.jsx.
// ────────────────────────────────────────────────────────────────

const { useReducer, useEffect, useRef, useState, useContext, createContext, useMemo } = React;

// ─── THEMES ─────────────────────────────────────────────────────
const THEMES = {
  hires: {
    id: 'hires', name: 'Hi-Res',
    bg: '#0a0b0e', panel: '#14161a', panel2: 'rgba(255,255,255,.05)',
    text: '#e8e5dc', dim: 'rgba(232,229,220,.55)', faint: 'rgba(232,229,220,.18)',
    rule: 'rgba(255,255,255,.09)', accent: '#d4a955', hot: '#c0392b', onAccent: '#1a1612',
    fontBody: "'Hanken Grotesk', sans-serif", fontDisplay: "'Hanken Grotesk', sans-serif",
    fontMono: "'JetBrains Mono', monospace",
    upper: false, serif: false, radius: 4, artScale: 1.0, badge: 'Hi-Res',
  },
  nocturne: {
    id: 'nocturne', name: 'Nocturne',
    bg: '#060608', panel: '#0c0c12', panel2: 'rgba(255,255,255,.03)',
    text: '#ede5d2', dim: 'rgba(237,229,210,.5)', faint: 'rgba(237,229,210,.16)',
    rule: 'rgba(237,229,210,.1)', accent: '#c4b6ff', hot: '#ff7a5c', onAccent: '#0a0a14',
    fontBody: "'Hanken Grotesk', sans-serif", fontDisplay: "'Instrument Serif', serif",
    fontMono: "'JetBrains Mono', monospace",
    upper: false, serif: true, radius: 2, artScale: 1.05, badge: 'Hi-Res',
  },
  terminal: {
    id: 'terminal', name: 'Terminal',
    bg: '#0d0d0d', panel: '#121212', panel2: 'rgba(240,164,32,.05)',
    text: '#e8e6dc', dim: 'rgba(232,230,220,.55)', faint: 'rgba(232,230,220,.18)',
    rule: 'rgba(232,230,220,.22)', accent: '#f0a420', hot: '#ff6e5e', onAccent: '#0d0d0d',
    fontBody: "'JetBrains Mono', monospace", fontDisplay: "'JetBrains Mono', monospace",
    fontMono: "'JetBrains Mono', monospace",
    upper: true, serif: false, radius: 0, artScale: 0.9, badge: 'NW-A55',
  },
};

// ─── APPLE SKIN (light + dark) ──────────────────────────────────
// System SF stack, iOS materials, big radii, Apple-Music pink accent.
const SF = "-apple-system, BlinkMacSystemFont, 'SF Pro Text', 'SF Pro Display', 'Helvetica Neue', system-ui, sans-serif";
const SFM = "'SF Mono', ui-monospace, 'JetBrains Mono', monospace";
const APPLE_BASE = {
  id: 'apple', name: 'Apple',
  fontBody: SF, fontDisplay: SF, fontMono: SFM,
  upper: false, serif: false, radius: 14, artScale: 1.0, badge: 'Lossless',
};
const APPLE_DARK = {
  ...APPLE_BASE, scheme: 'dark',
  bg: '#000000', panel: '#1c1c1e', panel2: 'rgba(118,118,128,.24)',
  text: '#ffffff', dim: 'rgba(235,235,245,.6)', faint: 'rgba(235,235,245,.16)',
  rule: 'rgba(255,255,255,.1)', accent: '#FF375F', hot: '#FF453A', onAccent: '#ffffff',
  ctrlOff: 'rgba(120,120,128,.36)', glass: 'rgba(28,28,30,.72)',
  bodyGrad: 'linear-gradient(150deg,#3a3a3c,#1c1c1e)',
  hwBg: '#2c2c2e', hwBorder: '#3a3a3c', hwText: '#e5e5ea', lockBg: '#000',
};
const APPLE_LIGHT = {
  ...APPLE_BASE, scheme: 'light',
  bg: '#f2f2f7', panel: '#ffffff', panel2: 'rgba(118,118,128,.12)',
  text: '#000000', dim: 'rgba(60,60,67,.6)', faint: 'rgba(60,60,67,.1)',
  rule: 'rgba(60,60,67,.16)', accent: '#FA2D48', hot: '#FF3B30', onAccent: '#ffffff',
  ctrlOff: 'rgba(120,120,128,.22)', glass: 'rgba(248,248,250,.78)',
  bodyGrad: 'linear-gradient(150deg,#e9e9ee,#c2c2c8)',
  hwBg: '#ffffff', hwBorder: 'rgba(0,0,0,.1)', hwText: '#1c1c1e', lockBg: '#000',
};
THEMES.apple = APPLE_DARK; // appears in the skin switcher

function getTheme(state) {
  if (state.skin === 'apple') return state.mode === 'light' ? APPLE_LIGHT : APPLE_DARK;
  return THEMES[state.skin] || THEMES.hires;
}

// ─── STATE ──────────────────────────────────────────────────────
function durSec(t) {
  const [m, s] = t.dur.split(':').map(Number);
  return m * 60 + s;
}

const initialState = {
  skin: 'hires',
  mode: 'dark',            // apple light/dark
  route: 'now',
  stack: [],
  trackIdx: 0,
  albumIdx: 0,
  shelves: [null, null, null],
  shelfOpen: false,
  _past: [],
  _future: [],
  playing: true,
  posSec: 47,
  vol: 21,
  volPopup: false,
  volStamp: 0,
  shuffle: false,
  repeat: 'off',          // off | all | one
  viz: 'bars',
  eqPreset: 'A1',
  eqBands: EQ_BANDS.map(b => b.db),
  ldac: 'Sound Quality',
  btConnected: 'WH-1000XM5',
  locked: false,
  flags: {
    highGainMini: false, highGainBal: false, dynamicNormalizer: false,
    dseeHX: true, dcPhase: true, vinyl: false, btRx: false,
    dacDoP: true, chargeFromHost: true, avls: true,
  },
  fav: { k: true },
};

function reducer(s, a) {
  switch (a.type) {
    case 'NAV':       return { ...s, stack: [...s.stack, s.route], route: a.route };
    case 'BACK':      return s.stack.length ? { ...s, route: s.stack[s.stack.length - 1], stack: s.stack.slice(0, -1) } : { ...s, route: 'now' };
    case 'HOME':      return { ...s, route: 'now', stack: [] };
    case 'PLAY':      return { ...s, playing: !s.playing };
    case 'NEXT': {
      const i = (s.trackIdx + 1) % TRACKS.length;
      return { ...s, trackIdx: i, posSec: 0, playing: true };
    }
    case 'PREV': {
      if (s.posSec > 3) return { ...s, posSec: 0 };
      const i = (s.trackIdx - 1 + TRACKS.length) % TRACKS.length;
      return { ...s, trackIdx: i, posSec: 0, playing: true };
    }
    case 'PICK_TRACK': return { ...s, trackIdx: a.i, posSec: 0, playing: true, route: 'now', stack: [] };
    case 'SEEK':      return { ...s, posSec: a.sec };
    case 'TICK': {
      if (!s.playing || s.locked) return s;
      const d = durSec(TRACKS[s.trackIdx]);
      let p = s.posSec + a.dt;
      if (p >= d) {
        if (s.repeat === 'one') return { ...s, posSec: 0 };
        const i = (s.trackIdx + 1) % TRACKS.length;
        return { ...s, trackIdx: i, posSec: 0 };
      }
      return { ...s, posSec: p };
    }
    case 'VOL': {
      const v = Math.max(0, Math.min(120, s.vol + a.delta));
      return { ...s, vol: v, volPopup: true, volStamp: s.volStamp + 1 };
    }
    case 'VOL_HIDE':  return { ...s, volPopup: false };
    case 'SET_VIZ':   return { ...s, viz: a.kind };
    case 'SET_EQ_PRESET': {
      const presets = {
        Off:   [0,0,0,0,0,0,0,0,0,0],
        A1:    EQ_BANDS.map(b => b.db),
        Heavy: [6,5,3,1,-1,-1,1,3,5,4],
        Pop:   [-1,0,2,4,3,1,0,-1,-1,0],
        Jazz:  [3,2,1,2,-1,-1,0,1,2,3],
        Vocal: [-2,-1,0,2,4,4,3,1,0,-1],
      };
      return { ...s, eqPreset: a.name, eqBands: (presets[a.name] || s.eqBands).slice() };
    }
    case 'SET_EQ_BAND': {
      const b = s.eqBands.slice(); b[a.i] = Math.max(-12, Math.min(12, a.db));
      return { ...s, eqBands: b, eqPreset: 'Custom' };
    }
    case 'SET_LDAC':  return { ...s, ldac: a.name };
    case 'BT_CONNECT':return { ...s, btConnected: a.name };
    case 'TOGGLE_FLAG': return { ...s, flags: { ...s.flags, [a.key]: !s.flags[a.key] } };
    case 'SHUFFLE':   return { ...s, shuffle: !s.shuffle };
    case 'REPEAT':    return { ...s, repeat: s.repeat === 'off' ? 'all' : s.repeat === 'all' ? 'one' : 'off' };
    case 'FAV': {
      const id = TRACKS[s.trackIdx].id;
      return { ...s, fav: { ...s.fav, [id]: !s.fav[id] } };
    }
    case 'SKIN':      return { ...s, skin: a.name };
    case 'MODE':      return { ...s, mode: s.mode === 'light' ? 'dark' : 'light' };
    case 'OPEN_ALBUM':return { ...s, albumIdx: a.i, stack: [...s.stack, s.route], route: 'album' };
    case 'SHELF_OPEN':  return { ...s, shelfOpen: true };
    case 'SHELF_CLOSE': return { ...s, shelfOpen: false };
    case 'SHELF_SAVE': {
      const snap = { route: s.route, trackIdx: s.trackIdx, albumIdx: s.albumIdx, stack: s.stack.slice(), savedAt: Date.now() };
      const sh = s.shelves.slice();
      let slot = a.slot;
      if (slot == null) { slot = sh.findIndex(x => !x); if (slot < 0) slot = 0; }
      sh[slot] = snap;
      return { ...s, shelves: sh };
    }
    case 'SHELF_CLEAR': { const sh = s.shelves.slice(); sh[a.slot] = null; return { ...s, shelves: sh }; }
    case 'SHELF_RESTORE': {
      const snap = s.shelves[a.slot]; if (!snap) return s;
      return { ...s, route: snap.route, trackIdx: snap.trackIdx, albumIdx: snap.albumIdx ?? s.albumIdx, stack: (snap.stack || []).slice(), shelfOpen: false };
    }
    case 'LOCK':      return { ...s, locked: true, route: 'now', stack: [] };
    case 'UNLOCK':    return { ...s, locked: false };
    default:          return s;
  }
}

// ─── HISTORY WRAPPER (undo / redo) ──────────────────────────────
const NO_HISTORY = new Set([
  'TICK', 'SEEK', 'VOL', 'VOL_HIDE', 'SHELF_OPEN', 'SHELF_CLOSE',
  'SHELF_SAVE', 'SHELF_CLEAR', 'MODE', 'UNDO', 'REDO',
]);
function stripH(s) { const { _past, _future, ...rest } = s; return rest; }
function rootReducer(s, a) {
  if (a.type === 'UNDO') {
    const past = s._past || [];
    if (!past.length) return s;
    const prev = past[past.length - 1];
    return { ...prev, shelves: s.shelves, mode: s.mode, skin: s.skin, shelfOpen: false,
      _past: past.slice(0, -1), _future: [stripH(s), ...(s._future || [])].slice(0, 50) };
  }
  if (a.type === 'REDO') {
    const fut = s._future || [];
    if (!fut.length) return s;
    const nxt = fut[0];
    return { ...nxt, shelves: s.shelves, mode: s.mode, skin: s.skin, shelfOpen: false,
      _past: [...(s._past || []), stripH(s)].slice(-50), _future: fut.slice(1) };
  }
  const next = reducer(s, a);
  if (next === s || NO_HISTORY.has(a.type)) return next;
  return { ...next, _past: [...(s._past || []), stripH(s)].slice(-50), _future: [] };
}

// ─── CONTEXT ────────────────────────────────────────────────────
const PlayerCtx = createContext(null);
const usePlayer = () => useContext(PlayerCtx);

// ─── THEMED PRIMITIVES ──────────────────────────────────────────
function tx(theme, s) { return theme.upper && typeof s === 'string' ? s.toUpperCase() : s; }

function ScreenShell({ children, pad = true, style }) {
  const { theme } = usePlayer();
  return (
    <div style={{
      position: 'absolute', inset: 0, background: theme.bg, color: theme.text,
      fontFamily: theme.fontBody, overflow: 'hidden', display: 'flex', flexDirection: 'column',
    }}>
      <div className="np-scroll" style={{ flex: 1, overflowY: 'auto', overflowX: 'hidden', padding: pad ? '0 0 16px' : 0, ...style }}>
        {children}
      </div>
    </div>
  );
}

function StatusBarX({ badge, right }) {
  const { theme, state } = usePlayer();
  return (
    <div style={{
      display: 'flex', alignItems: 'center', justifyContent: 'space-between',
      padding: '10px 20px', fontSize: 11, fontFamily: theme.fontMono,
      color: theme.dim, letterSpacing: theme.upper ? '.1em' : '.02em', flexShrink: 0,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <span>{fmtClock()}</span>
        {badge !== false && (
          <span style={{
            padding: '2px 7px', borderRadius: 999, fontSize: 9, letterSpacing: '.12em',
            background: hexA(theme.accent, 0.14), color: theme.accent,
          }}>{badge || theme.badge}</span>
        )}
        <ShelfControls />
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        {right}
        <span>{state.vol}</span>
        <span style={{ opacity: .8 }}>78%</span>
        <span style={{ display: 'inline-block', width: 18, height: 9, border: `1px solid ${theme.dim}`, borderRadius: 2, position: 'relative' }}>
          <i style={{ position: 'absolute', inset: 1, width: '78%', background: theme.dim, display: 'block' }} />
        </span>
      </div>
    </div>
  );
}

function Header({ title, right }) {
  const { theme, dispatch } = usePlayer();
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '6px 20px 14px', flexShrink: 0 }}>
      <button onClick={() => dispatch({ type: 'BACK' })} style={{
        background: 'none', border: 'none', color: theme.text, cursor: 'pointer',
        padding: 4, margin: -4, display: 'flex', alignItems: 'center',
      }}><IconBack size={20} /></button>
      <div style={{
        fontFamily: theme.fontDisplay, fontSize: theme.serif ? 30 : 22,
        fontWeight: theme.serif ? 400 : 600, letterSpacing: theme.upper ? '.04em' : 0,
      }}>{tx(theme, title)}</div>
      <div style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 10 }}>{right}</div>
    </div>
  );
}

function SectionLabel({ children, style }) {
  const { theme } = usePlayer();
  return (
    <div style={{
      fontFamily: theme.fontMono, fontSize: 9, letterSpacing: '.18em',
      color: theme.accent, textTransform: 'uppercase', padding: '0 20px',
      margin: '14px 0 6px', ...style,
    }}>{theme.upper ? <>{'[ '}{children}{' ]'}</> : children}</div>
  );
}

function Toggle({ on, onClick }) {
  const { theme } = usePlayer();
  if (theme.upper) {
    return <button onClick={onClick} style={{ background: 'none', border: 'none', color: theme.accent, fontFamily: theme.fontMono, fontSize: 11, cursor: 'pointer' }}>[{on ? 'X' : ' '}] {on ? 'ON' : 'OFF'}</button>;
  }
  return (
    <button onClick={onClick} style={{
      width: 36, height: 20, borderRadius: 12, border: 'none', cursor: 'pointer', padding: 0,
      background: on ? theme.accent : (theme.ctrlOff || 'rgba(255,255,255,.14)'), position: 'relative', transition: 'background .15s',
    }}>
      <span style={{ position: 'absolute', top: 2, left: on ? 18 : 2, width: 16, height: 16, borderRadius: '50%', background: on ? theme.onAccent : '#fff', transition: 'left .15s' }} />
    </button>
  );
}

function Row({ label, value, right, onClick, toggle, on, danger, accent, sub, icon }) {
  const { theme } = usePlayer();
  return (
    <button onClick={onClick} style={{
      display: 'flex', alignItems: 'center', gap: 12, width: '100%', textAlign: 'left',
      padding: '12px 20px', background: 'none', border: 'none',
      borderBottom: `1px solid ${theme.rule}`, cursor: onClick || toggle ? 'pointer' : 'default',
      color: 'inherit', fontFamily: 'inherit',
    }}>
      {icon && <span style={{ color: theme.dim, display: 'flex' }}>{icon}</span>}
      <span style={{ flex: 1, minWidth: 0 }}>
        <span style={{ display: 'block', fontSize: 14, fontWeight: 500, color: danger ? theme.hot : accent ? theme.accent : theme.text, fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody }}>
          {tx(theme, label)}
        </span>
        {sub && <span style={{ display: 'block', fontSize: 11, color: theme.dim, marginTop: 2 }}>{tx(theme, sub)}</span>}
      </span>
      {toggle ? <Toggle on={on} onClick={(e) => { e.stopPropagation(); onClick && onClick(); }} />
        : (
          <span style={{ display: 'flex', alignItems: 'center', gap: 8, color: theme.dim }}>
            {value && <span style={{ fontFamily: theme.fontMono, fontSize: 11 }}>{tx(theme, value)}</span>}
            {right}
            {onClick && <IconChevron size={14} style={{ opacity: .5 }} />}
          </span>
        )}
    </button>
  );
}

// time helpers
function fmtClock() { return '14:32'; }

// ─── DEVICE FRAME ───────────────────────────────────────────────
function useFitScale(w, h, pad = 48) {
  const [scale, setScale] = useState(1);
  useEffect(() => {
    const fit = () => {
      const sw = (window.innerWidth - pad) / w;
      const sh = (window.innerHeight - 92 - pad) / h;
      setScale(Math.min(1.4, sw, sh));
    };
    fit();
    window.addEventListener('resize', fit);
    return () => window.removeEventListener('resize', fit);
  }, [w, h, pad]);
  return scale;
}

function HwButton({ children, onClick, wide, theme, title }) {
  return (
    <button title={title} onClick={onClick} style={{
      flex: wide ? 2 : 1, padding: '0 10px', height: 44, minWidth: 44,
      background: (theme && theme.hwBg) || '#1b1c1f', border: `1px solid ${(theme && theme.hwBorder) || '#2c2d31'}`, borderRadius: 8,
      color: (theme && theme.hwText) || '#cfcdc6', cursor: 'pointer', display: 'flex', alignItems: 'center',
      justifyContent: 'center', gap: 6, fontFamily: "'JetBrains Mono', monospace",
      fontSize: 10, letterSpacing: '.08em', boxShadow: 'inset 0 1px 0 rgba(255,255,255,.05)',
    }}>{children}</button>
  );
}

function Device() {
  const { state, dispatch, theme } = usePlayer();
  const SW = 480, SH = 800;
  const deckH = 96;
  const bezel = 18;
  const bodyW = SW + bezel * 2;
  const bodyH = SH + bezel * 2 + deckH;
  const scale = useFitScale(bodyW, bodyH);

  return (
    <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', flex: 1, overflow: 'hidden' }}>
      <div style={{ transform: `scale(${scale})`, transformOrigin: 'center center' }}>
        <div style={{
          width: bodyW, height: bodyH, background: theme.bodyGrad || 'linear-gradient(150deg,#26272b,#161719)',
          borderRadius: 30, padding: bezel, boxSizing: 'border-box',
          boxShadow: '0 40px 90px rgba(0,0,0,.6), inset 0 1px 0 rgba(255,255,255,.08)',
          position: 'relative',
        }}>
          {/* Side volume rocker (visual) */}
          <div style={{ position: 'absolute', right: -3, top: 150, width: 3, height: 70, background: '#3a3b3f', borderRadius: 2 }} />
          {/* Screen */}
          <div style={{ width: SW, height: SH, borderRadius: 10, overflow: 'hidden', position: 'relative', background: theme.bg, boxShadow: 'inset 0 0 0 1px rgba(0,0,0,.6)' }}>
            <Router />
            {state.volPopup && <VolumeOverlay />}
            {state.shelfOpen && <ShelfSheet />}
            {state.locked && <LockScreen />}
          </div>

          {/* Hardware deck */}
          <div style={{ height: deckH, display: 'flex', alignItems: 'center', gap: 8, padding: '0 6px' }}>
            <HwButton theme={theme} title="Menu / Home" onClick={() => dispatch(state.locked ? { type: 'UNLOCK' } : { type: 'HOME' })}>
              <IconList size={16} />
            </HwButton>
            <HwButton theme={theme} title="Back" onClick={() => dispatch({ type: 'BACK' })}>
              <IconBack size={16} />
            </HwButton>
            <HwButton theme={theme} title="Volume down" onClick={() => dispatch({ type: 'VOL', delta: -3 })}>VOL −</HwButton>
            <HwButton theme={theme} title="Previous" onClick={() => dispatch({ type: 'PREV' })}><IconPrev size={16} /></HwButton>
            <HwButton theme={theme} wide title="Play / Pause" onClick={() => dispatch({ type: 'PLAY' })}>
              {state.playing ? <IconPause size={18} /> : <IconPlay size={18} />}
            </HwButton>
            <HwButton theme={theme} title="Next" onClick={() => dispatch({ type: 'NEXT' })}><IconNext size={16} /></HwButton>
            <HwButton theme={theme} title="Volume up" onClick={() => dispatch({ type: 'VOL', delta: 3 })}>VOL +</HwButton>
            <HwButton theme={theme} title={state.locked ? 'HOLD on' : 'HOLD off'} onClick={() => dispatch(state.locked ? { type: 'UNLOCK' } : { type: 'LOCK' })}>
              <IconLock size={15} style={{ color: state.locked ? theme.accent : '#cfcdc6' }} />
            </HwButton>
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── ROUTER ─────────────────────────────────────────────────────
function Router() {
  const { state } = usePlayer();
  const R = state.route;
  switch (R) {
    case 'now':       return <NowPlaying />;
    case 'menu':      return <Menu />;
    case 'viz':       return <VizPicker />;
    case 'library':   return <LibraryScreen />;
    case 'album':     return <AlbumScreen />;
    case 'queue':     return <QueueScreen />;
    case 'browse':    return <BrowseScreen />;
    case 'eq':        return <EqScreen />;
    case 'sound':     return <SoundScreen />;
    case 'output':    return <OutputScreen />;
    case 'bt':        return <BluetoothScreen />;
    case 'btrx':      return <BtRxScreen />;
    case 'usbdac':    return <UsbDacScreen />;
    case 'settings':  return <SettingsScreen />;
    case 'reset':     return <ResetScreen />;
    case 'wizard':    return <WizardScreen />;
    case 'track':     return <TrackScreen />;
    case 'lyrics':    return <LyricsScreen />;
    case 'night':     return <NightScreen />;
    default:          return <NowPlaying />;
  }
}

// ─── VOLUME OVERLAY ─────────────────────────────────────────────
function VolumeOverlay() {
  const { state, dispatch, theme } = usePlayer();
  useEffect(() => {
    const id = setTimeout(() => dispatch({ type: 'VOL_HIDE' }), 1500);
    return () => clearTimeout(id);
  }, [state.volStamp]);
  const cells = 30, lit = Math.round(state.vol / 120 * cells);
  return (
    <div style={{ position: 'absolute', left: 24, right: 24, top: 300, padding: '22px 24px', background: theme.panel, border: `1px solid ${theme.accent}`, borderRadius: theme.radius, boxShadow: '0 20px 50px rgba(0,0,0,.5)' }}>
      <div style={{ fontFamily: theme.fontMono, fontSize: 9, letterSpacing: '.18em', color: theme.accent }}>{theme.upper ? '[ VOLUME ]' : 'VOLUME'}</div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 8, marginTop: 4 }}>
        <span style={{ fontFamily: theme.fontDisplay, fontSize: 56, color: theme.accent, lineHeight: 1 }}>{state.vol}</span>
        <span style={{ fontFamily: theme.fontMono, fontSize: 13, color: theme.dim }}>/ 120</span>
      </div>
      <div style={{ display: 'flex', gap: 2, marginTop: 14 }}>
        {Array.from({ length: cells }).map((_, i) => (
          <div key={i} style={{ flex: 1, height: 14, background: i < lit ? (i >= 25 ? theme.hot : theme.accent) : theme.faint }} />
        ))}
      </div>
    </div>
  );
}

// ─── LOCK SCREEN ────────────────────────────────────────────────
function LockScreen() {
  const { state, dispatch, theme } = usePlayer();
  const t = TRACKS[state.trackIdx];
  return (
    <div onClick={() => dispatch({ type: 'UNLOCK' })} style={{ position: 'absolute', inset: 0, background: theme.lockBg || '#000', color: '#fff', fontFamily: theme.fontBody, padding: '32px 28px', display: 'flex', flexDirection: 'column', cursor: 'pointer' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', fontFamily: theme.fontMono, fontSize: 10, color: theme.accent }}>
        <span>{theme.upper ? '[LOCK] HOLD' : '⊘ HOLD'}</span><span>78%</span>
      </div>
      <div style={{ marginTop: 60 }}>
        <div style={{ fontFamily: theme.fontDisplay, fontSize: 130, lineHeight: .95, color: theme.accent, letterSpacing: '-.03em' }}>14:32</div>
        <div style={{ fontFamily: theme.serif ? theme.fontDisplay : theme.fontMono, fontStyle: theme.serif ? 'italic' : 'normal', fontSize: 18, color: theme.dim, marginTop: 6 }}>{tx(theme, 'Thursday, 27 May')}</div>
      </div>
      <div style={{ marginTop: 'auto' }}>
        <div style={{ fontFamily: theme.fontMono, fontSize: 9, letterSpacing: '.16em', color: theme.accent }}>{theme.upper ? '[ NOW PLAYING ]' : 'NOW PLAYING'}</div>
        <div style={{ fontFamily: theme.fontDisplay, fontSize: 26, marginTop: 6 }}>{tx(theme, t.title)}</div>
        <div style={{ fontSize: 13, color: theme.dim, marginTop: 2 }}>{tx(theme, t.artist)}</div>
        <div style={{ height: 90, marginTop: 14 }}>
          <VizCanvas kind={state.viz} palette={{ accent: theme.accent, hot: theme.hot }} width={424} height={90} />
        </div>
        <div style={{ textAlign: 'center', fontFamily: theme.fontMono, fontSize: 9, letterSpacing: '.24em', color: theme.dim, marginTop: 16 }}>
          {theme.upper ? '▼ TAP / HOLD TO WAKE' : 'TAP TO UNLOCK'}
        </div>
      </div>
    </div>
  );
}

// ─── NOW PLAYING ────────────────────────────────────────────────
function NowPlaying() {
  const { state, dispatch, theme } = usePlayer();
  const t = TRACKS[state.trackIdx];
  const d = durSec(t);
  const pct = Math.min(1, state.posSec / d);
  const art = Math.round(248 * theme.artScale);
  const isFav = !!state.fav[t.id];

  return (
    <ScreenShell pad={false}>
      <StatusBarX right={<span style={{ fontFamily: theme.fontMono, fontSize: 9 }}>{tx(theme, t.codec === 'DSD' ? 'DSD 5.6M' : `${t.codec} ${t.bits}/${t.rate.replace(' kHz','k').replace(' MHz','M')}`)}</span>} />

      {/* top bar: menu · direct Library/Up Next · info */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '0 14px 6px', flexShrink: 0 }}>
        <button onClick={() => dispatch({ type: 'NAV', route: 'menu' })} style={iconBtn(theme)}><IconList size={20} /></button>
        <div style={{ marginLeft: 'auto', marginRight: 'auto', display: 'flex', gap: 8 }}>
          <QuickPill theme={theme} icon={<IconGrid size={13} />} label="Library" onClick={() => dispatch({ type: 'NAV', route: 'library' })} />
          <QuickPill theme={theme} icon={<IconQueue size={13} />} label="Up Next" onClick={() => dispatch({ type: 'NAV', route: 'queue' })} />
        </div>
        <button onClick={() => dispatch({ type: 'NAV', route: 'track' })} style={iconBtn(theme)}><IconMore size={20} /></button>
      </div>

      {/* album art — as large as possible; visualizer overlaid at the base (tap to cycle) */}
      <div style={{ padding: '2px 14px 0', flexShrink: 0 }}>
        <button onClick={() => cycleViz(state, dispatch)} title="Tap art to change visualizer" style={{
          position: 'relative', display: 'block', width: '100%', padding: 0, border: 'none', background: 'none', cursor: 'pointer',
          borderRadius: artRadius(theme), overflow: 'hidden',
          boxShadow: theme.id === 'terminal' ? `0 0 0 1px ${theme.accent}` : '0 22px 50px rgba(0,0,0,.5)',
        }}>
          <Art kind={t.art} size={452} label={false} style={{ width: '100%', height: 'auto', aspectRatio: '1', borderRadius: 0 }} />
          <span style={{ position: 'absolute', left: 0, right: 0, bottom: 0, height: 96, display: 'block', pointerEvents: 'none', background: 'linear-gradient(to top, rgba(0,0,0,.62), rgba(0,0,0,.16) 58%, transparent)' }} />
          <span style={{ position: 'absolute', left: 0, right: 0, bottom: 0, height: 72, display: 'block', pointerEvents: 'none' }}>
            <VizCanvas kind={state.viz} palette={{ accent: theme.accent, hot: theme.hot }} width={452} height={72} />
          </span>
          <span style={{ position: 'absolute', right: 9, bottom: 8, fontFamily: theme.fontMono, fontSize: 8, letterSpacing: '.12em', color: 'rgba(255,255,255,.82)' }}>
            {tx(theme, vizLabel(state.viz))} ↻
          </span>
        </button>
      </div>

      {/* title / artist */}
      <div style={{ padding: '12px 24px 0', textAlign: theme.serif ? 'left' : 'center', flexShrink: 0 }}>
        <div style={{ fontFamily: theme.fontDisplay, fontSize: theme.serif ? 32 : 22, fontWeight: theme.serif ? 400 : 700, lineHeight: 1.05, letterSpacing: theme.upper ? '.02em' : 0, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, t.title)}</div>
        <div style={{ fontSize: 14, color: theme.dim, marginTop: 4, fontStyle: theme.serif ? 'italic' : 'normal', fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody }}>{tx(theme, t.artist)}</div>
      </div>

      {/* progress */}
      <div style={{ padding: '12px 24px 0', flexShrink: 0 }}>
        <div onClick={(e) => seekClick(e, d, dispatch)} style={{ height: 4, background: theme.faint, borderRadius: 2, cursor: 'pointer', position: 'relative' }}>
          <div style={{ position: 'absolute', left: 0, top: 0, bottom: 0, width: `${pct * 100}%`, background: theme.accent, borderRadius: 2 }} />
          <div style={{ position: 'absolute', left: `${pct * 100}%`, top: '50%', width: 10, height: 10, borderRadius: '50%', background: theme.accent, transform: 'translate(-50%,-50%)' }} />
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between', fontFamily: theme.fontMono, fontSize: 10, color: theme.dim, marginTop: 6 }}>
          <span>{fmtTime(state.posSec)}</span>
          <span>−{fmtTime(d - state.posSec)}</span>
        </div>
      </div>

      {/* transport */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '14px 30px 0', flexShrink: 0 }}>
        <button onClick={() => dispatch({ type: 'SHUFFLE' })} style={iconBtn(theme, state.shuffle ? theme.accent : theme.dim)}><IconShuffle size={18} /></button>
        <button onClick={() => dispatch({ type: 'PREV' })} style={iconBtn(theme)}><IconPrev size={28} /></button>
        <button onClick={() => dispatch({ type: 'PLAY' })} style={{
          width: 62, height: 62, borderRadius: '50%', border: `1.5px solid ${theme.accent}`,
          background: (theme.id === 'hires' || theme.id === 'apple') ? theme.accent : 'transparent',
          color: (theme.id === 'hires' || theme.id === 'apple') ? theme.onAccent : theme.accent,
          display: 'flex', alignItems: 'center', justifyContent: 'center', cursor: 'pointer',
        }}>{state.playing ? <IconPause size={24} /> : <IconPlay size={24} />}</button>
        <button onClick={() => dispatch({ type: 'NEXT' })} style={iconBtn(theme)}><IconNext size={28} /></button>
        <button onClick={() => dispatch({ type: 'REPEAT' })} style={iconBtn(theme, state.repeat !== 'off' ? theme.accent : theme.dim)}>
          <IconRepeat size={18} />{state.repeat === 'one' && <span style={{ fontSize: 8, position: 'absolute' }}>1</span>}
        </button>
      </div>

      {/* bottom actions */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-around', padding: '16px 24px 4px', marginTop: 'auto', flexShrink: 0, borderTop: `1px solid ${theme.rule}` }}>
        <ActionBtn theme={theme} active={isFav} onClick={() => dispatch({ type: 'FAV' })}>{isFav ? <IconHeartFill size={18} /> : <IconHeart size={18} />}</ActionBtn>
        <ActionBtn theme={theme} onClick={() => dispatch({ type: 'NAV', route: 'lyrics' })}><span style={{ fontFamily: theme.fontMono, fontSize: 10 }}>{tx(theme, 'Lyrics')}</span></ActionBtn>
        <ActionBtn theme={theme} onClick={() => dispatch({ type: 'NAV', route: 'eq' })}><IconSlider size={18} /></ActionBtn>
        <ActionBtn theme={theme} onClick={() => dispatch({ type: 'NAV', route: 'queue' })}><IconQueue size={18} /></ActionBtn>
        <ActionBtn theme={theme} onClick={() => dispatch({ type: 'NAV', route: 'bt' })}><IconBluetooth size={18} /></ActionBtn>
      </div>
    </ScreenShell>
  );
}

function ActionBtn({ children, onClick, active, theme }) {
  return <button onClick={onClick} style={{ background: 'none', border: 'none', cursor: 'pointer', color: active ? theme.accent : theme.dim, display: 'flex', alignItems: 'center', padding: 6 }}>{children}</button>;
}
function QuickPill({ theme, icon, label, onClick }) {
  return (
    <button onClick={onClick} style={{
      display: 'flex', alignItems: 'center', gap: 6, padding: '6px 13px',
      borderRadius: theme.radius === 0 ? 0 : 999, background: theme.panel2,
      border: `1px solid ${theme.rule}`, color: theme.text, cursor: 'pointer',
      fontFamily: theme.fontMono, fontSize: 10, letterSpacing: '.04em', whiteSpace: 'nowrap',
    }}>
      <span style={{ color: theme.accent, display: 'flex' }}>{icon}</span>{tx(theme, label)}
    </button>
  );
}
function iconBtn(theme, color) {
  return { background: 'none', border: 'none', cursor: 'pointer', color: color || theme.text, display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 4, position: 'relative' };
}
function vizLabel(id) { return (VIZ_KINDS.find(k => k.id === id) || {}).label || id; }
function cycleViz(state, dispatch) {
  const i = VIZ_KINDS.findIndex(k => k.id === state.viz);
  dispatch({ type: 'SET_VIZ', kind: VIZ_KINDS[(i + 1) % VIZ_KINDS.length].id });
}
function seekClick(e, d, dispatch) {
  const r = e.currentTarget.getBoundingClientRect();
  const p = (e.clientX - r.left) / r.width;
  dispatch({ type: 'SEEK', sec: Math.max(0, Math.min(d, p * d)) });
}

// ─── MENU ───────────────────────────────────────────────────────
function Menu() {
  const { dispatch, theme, state } = usePlayer();
  const t = TRACKS[state.trackIdx];
  const items = [
    { r: 'now',      label: 'Now Playing', icon: <IconPlay size={16} />,    sub: t.title },
    { r: 'library',  label: 'Library',     icon: <IconGrid size={16} />,    sub: '124 albums' },
    { r: 'queue',    label: 'Up Next',     icon: <IconQueue size={16} />,   sub: '9 tracks' },
    { r: 'browse',   label: 'Browse',      icon: <IconSearch size={16} />,  sub: 'Artists · Albums · Genres' },
    { r: 'eq',       label: 'Equalizer',   icon: <IconSlider size={16} />,  sub: `Preset ${state.eqPreset}` },
    { r: 'sound',    label: 'Sound Settings', icon: <IconVolume size={16} />, sub: 'DSEE HX · DC Phase' },
    { r: 'viz',      label: 'Visualizer',  icon: <IconGrid size={16} />,    sub: vizLabel(state.viz) },
    { r: 'bt',       label: 'Bluetooth',   icon: <IconBluetooth size={16} />, sub: state.btConnected },
    { r: 'output',   label: 'Output',      icon: <IconHeadphone size={16} />, sub: '3.5 mm Stereo' },
    { r: 'usbdac',   label: 'USB-DAC',     icon: <IconWifi size={16} />,    sub: 'PC → Walkman' },
    { r: 'btrx',     label: 'BT Receiver', icon: <IconBluetooth size={16} />, sub: 'Phone → Walkman' },
    { r: 'settings', label: 'Settings',    icon: <IconList size={16} />,    sub: 'System · Storage · About' },
    { r: 'night',    label: 'Night Mode',  icon: <IconLock size={16} />,    sub: 'Dark · quick access' },
  ];
  return (
    <ScreenShell>
      <StatusBarX />
      <Header title="Menu" />
      {items.map(it => (
        <Row key={it.r} icon={it.icon} label={it.label} sub={it.sub} onClick={() => dispatch({ type: it.r === 'now' ? 'HOME' : 'NAV', route: it.r })} />
      ))}
      <Row icon={<IconShelf size={16} />} label="Shelf" sub={`${state.shelves.filter(Boolean).length} saved · Undo history`} accent onClick={() => dispatch({ type: 'SHELF_OPEN' })} />
    </ScreenShell>
  );
}

// ─── VISUALIZER PICKER ──────────────────────────────────────────
function VizPicker() {
  const { state, dispatch, theme } = usePlayer();
  const pal = { accent: theme.accent, hot: theme.hot };
  return (
    <ScreenShell>
      <StatusBarX />
      <Header title="Visualizer" />
      <SectionLabel>Live preview · tap to select</SectionLabel>
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8, padding: '0 16px' }}>
        {VIZ_KINDS.map(k => {
          const sel = state.viz === k.id;
          return (
            <button key={k.id} onClick={() => dispatch({ type: 'SET_VIZ', kind: k.id })} style={{
              padding: 8, background: sel ? hexA(theme.accent, .08) : theme.panel2,
              border: `1px solid ${sel ? theme.accent : theme.rule}`, borderRadius: theme.radius,
              cursor: 'pointer', textAlign: 'left',
            }}>
              <div style={{ height: 70, marginBottom: 6 }}>
                <VizCanvas kind={k.id} palette={pal} width={196} height={70} />
              </div>
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                <span style={{ fontFamily: theme.fontMono, fontSize: 10, color: sel ? theme.accent : theme.dim, letterSpacing: '.04em' }}>{tx(theme, k.label)}</span>
                {sel && <IconCheck size={13} style={{ color: theme.accent }} />}
              </div>
            </button>
          );
        })}
      </div>
      <div style={{ padding: '14px 20px 0', color: theme.dim, fontSize: 11, lineHeight: 1.5, fontStyle: theme.serif ? 'italic' : 'normal' }}>
        {tx(theme, 'The selected style appears on Now Playing and the Lock screen, and reacts to playback in real time.')}
      </div>
    </ScreenShell>
  );
}

Object.assign(window, {
  THEMES, APPLE_DARK, APPLE_LIGHT, getTheme, reducer, rootReducer, initialState, PlayerCtx, usePlayer, tx, durSec,
  ScreenShell, StatusBarX, Header, SectionLabel, Toggle, Row, iconBtn, ActionBtn,
  Device, Router, NowPlaying, Menu, VizPicker, VolumeOverlay, LockScreen,
  vizLabel, cycleViz,
});
