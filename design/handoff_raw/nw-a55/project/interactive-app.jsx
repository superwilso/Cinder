// ────────────────────────────────────────────────────────────────
// interactive-app.jsx — root. Wires state/context, the playback
// clock, the visualizer play-gate, and the skin toolbar + device.
// (Reuses the global React hooks declared in player.jsx — do not
//  redeclare them here, or Babel global scope will collide.)
// ────────────────────────────────────────────────────────────────

function Toolbar() {
  const { state, dispatch, theme } = usePlayer();
  return (
    <div style={{
      display: 'flex', alignItems: 'center', gap: 16, padding: '12px 20px',
      background: '#0b0b0d', borderBottom: '1px solid #1c1d20', flexShrink: 0,
      fontFamily: "'JetBrains Mono', monospace",
    }}>
      <div style={{ color: '#e8e6dc', fontSize: 13, letterSpacing: '.04em', fontWeight: 600 }}>
        NW-A55 <span style={{ color: '#6a6b70' }}>· interactive</span>
      </div>

      {/* Skin switcher */}
      <div style={{ display: 'flex', gap: 2, background: '#151619', borderRadius: 8, padding: 3, marginLeft: 8 }}>
        {Object.values(THEMES).map(t => {
          const on = state.skin === t.id;
          return (
            <button key={t.id} onClick={() => dispatch({ type: 'SKIN', name: t.id })} style={{
              padding: '6px 14px', borderRadius: 6, border: 'none', cursor: 'pointer',
              fontFamily: 'inherit', fontSize: 11, letterSpacing: '.04em',
              background: on ? t.accent : 'transparent', color: on ? t.onAccent : '#9a9b9f',
              fontWeight: on ? 700 : 400,
            }}>{t.name}</button>
          );
        })}
      </div>

      {/* Apple light / dark toggle */}
      {state.skin === 'apple' && (
        <div style={{ display: 'flex', gap: 2, background: '#151619', borderRadius: 8, padding: 3 }}>
          {[['dark', IconMoon], ['light', IconSun]].map(([m, Ico]) => {
            const on = state.mode === m;
            return (
              <button key={m} title={m === 'dark' ? 'Dark mode' : 'Light mode'} onClick={() => { if (state.mode !== m) dispatch({ type: 'MODE' }); }} style={{
                display: 'flex', alignItems: 'center', gap: 6, padding: '6px 12px', borderRadius: 6, border: 'none', cursor: 'pointer',
                fontFamily: 'inherit', fontSize: 11, textTransform: 'capitalize',
                background: on ? theme.accent : 'transparent', color: on ? '#fff' : '#9a9b9f', fontWeight: on ? 700 : 400,
              }}><Ico size={13} />{m}</button>
            );
          })}
        </div>
      )}

      {/* Visualizer quick cycle */}
      <button onClick={() => cycleViz(state, dispatch)} style={{
        display: 'flex', alignItems: 'center', gap: 8, padding: '6px 12px', borderRadius: 8,
        background: '#151619', border: '1px solid #232428', color: '#cfcdc6', cursor: 'pointer',
        fontFamily: 'inherit', fontSize: 11,
      }}>
        <span style={{ color: '#6a6b70' }}>viz</span>
        <span style={{ color: theme.accent }}>{vizLabel(state.viz)}</span>
        <span>↻</span>
      </button>

      <div style={{ marginLeft: 'auto', color: '#56575c', fontSize: 11, whiteSpace: 'nowrap' }}>
        Shelf + Undo live in the status bar
      </div>
    </div>
  );
}

function InteractiveApp() {
  const [state, dispatch] = useReducer(rootReducer, initialState);
  const theme = getTheme(state);

  // playback clock — advance 4×/sec while playing
  useEffect(() => {
    const id = setInterval(() => dispatch({ type: 'TICK', dt: 0.25 }), 250);
    return () => clearInterval(id);
  }, []);

  // drive the visualizer's energy gate
  useEffect(() => {
    VizEngine.setPlaying(state.playing && !state.locked);
  }, [state.playing, state.locked]);

  const ctx = useMemo(() => ({ state, dispatch, theme }), [state, theme]);

  return (
    <PlayerCtx.Provider value={ctx}>
      <div style={{ height: '100vh', display: 'flex', flexDirection: 'column', background: '#0b0b0d' }}>
        <Toolbar />
        <Device />
      </div>
    </PlayerCtx.Provider>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<InteractiveApp />);
