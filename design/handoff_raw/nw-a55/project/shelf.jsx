// ────────────────────────────────────────────────────────────────
// shelf.jsx — "Shelf" + Undo/Redo.
//   ShelfControls  — compact undo + shelf buttons in the status bar.
//   ShelfSheet     — iOS-style bottom sheet: save the current place to
//                    one of three shelf slots, restore instantly, and
//                    step backward/forward through history.
// Reuses globals from player.jsx / data.jsx / icons.jsx.
// ────────────────────────────────────────────────────────────────

// Describe any place ({route,trackIdx,albumIdx}) as a title/sub/art.
function describePlace(p) {
  const t = TRACKS[p.trackIdx || 0] || TRACKS[0];
  const a = ALBUMS[p.albumIdx || 0] || ALBUMS[0];
  switch (p.route) {
    case 'now':      return { title: t.title,          sub: 'Now Playing · ' + t.artist, art: t.art };
    case 'album':    return { title: a.title,          sub: 'Album · ' + a.artist,       art: a.art };
    case 'library':  return { title: 'Library',        sub: 'Albums · Songs · Artists',  icon: 'grid' };
    case 'queue':    return { title: 'Up Next',        sub: TRACKS.length + ' tracks queued', icon: 'queue' };
    case 'browse':   return { title: 'Browse',         sub: 'Artists · Albums · Genres', icon: 'search' };
    case 'eq':       return { title: 'Equalizer',      sub: '10-band · presets',         icon: 'slider' };
    case 'sound':    return { title: 'Sound Settings', sub: 'DSEE HX · DC Phase',        icon: 'volume' };
    case 'lyrics':   return { title: 'Lyrics',         sub: t.title + ' · ' + t.artist,  art: t.art };
    case 'bt':       return { title: 'Bluetooth',      sub: 'Wireless output',           icon: 'bt' };
    case 'output':   return { title: 'Output',         sub: 'Headphone routing',         icon: 'headphone' };
    case 'settings': return { title: 'Settings',       sub: 'System · Storage · About',  icon: 'list' };
    case 'viz':      return { title: 'Visualizer',     sub: 'Live spectrum styles',      icon: 'grid' };
    case 'track':    return { title: 'Track Info',     sub: t.title,                     art: t.art };
    case 'menu':     return { title: 'Menu',           sub: 'All sections',              icon: 'list' };
    default:         return { title: p.route,          sub: 'Saved place',               icon: 'shelf' };
  }
}
function PlaceIcon({ name, size }) {
  const M = { grid: IconGrid, queue: IconQueue, search: IconSearch, slider: IconSlider,
    volume: IconVolume, bt: IconBluetooth, headphone: IconHeadphone, list: IconList, shelf: IconShelf };
  const C = M[name] || IconShelf;
  return <C size={size} />;
}

// Leading tile: album art if present, otherwise a tinted icon chip.
function PlaceTile({ place, theme, size = 46 }) {
  if (place.art) {
    return <Art kind={place.art} size={size} label={false} style={{ borderRadius: theme.radius > 8 ? 9 : theme.radius, boxShadow: theme.id === 'terminal' ? `0 0 0 1px ${theme.rule}` : '0 4px 12px rgba(0,0,0,.35)' }} />;
  }
  return (
    <div style={{ width: size, height: size, borderRadius: theme.radius > 8 ? 9 : theme.radius, background: hexA(theme.accent, .14), color: theme.accent, display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0 }}>
      <PlaceIcon name={place.icon} size={Math.round(size * 0.46)} />
    </div>
  );
}

// ─── STATUS-BAR CONTROLS ────────────────────────────────────────
function ShelfControls() {
  const { state, dispatch, theme } = usePlayer();
  const canUndo = (state._past || []).length > 0;
  const filled = (state.shelves || []).filter(Boolean).length;
  const chip = (active) => ({
    display: 'inline-flex', alignItems: 'center', justifyContent: 'center', gap: 3,
    height: 18, padding: '0 5px', background: 'none', border: 'none', cursor: active ? 'pointer' : 'default',
    color: active ? theme.text : theme.faint, opacity: active ? 1 : .5, position: 'relative',
  });
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 2, marginLeft: 2 }}>
      <button title="Undo" disabled={!canUndo} onClick={() => dispatch({ type: 'UNDO' })} style={chip(canUndo)}>
        <IconUndo size={13} />
      </button>
      <button title="Shelf" onClick={() => dispatch({ type: 'SHELF_OPEN' })} style={chip(true)}>
        <IconShelf size={13} style={{ color: filled ? theme.accent : theme.text }} />
        {filled > 0 && (
          <span style={{ fontFamily: theme.fontMono, fontSize: 8, color: theme.accent, lineHeight: 1 }}>{filled}</span>
        )}
      </button>
    </span>
  );
}

// ─── BOTTOM SHEET ───────────────────────────────────────────────
function ShelfSheet() {
  const { state, dispatch, theme } = usePlayer();
  const past = state._past || [], future = state._future || [];
  const canUndo = past.length > 0, canRedo = future.length > 0;
  const prevPlace = canUndo ? describePlace(past[past.length - 1]) : null;
  const nextPlace = canRedo ? describePlace(future[0]) : null;
  const here = describePlace(state);
  const slots = state.shelves || [null, null, null];
  const sheetR = theme.radius === 0 ? 0 : 22;
  const glass = theme.glass;

  const navPill = (label, place, enabled, Ico, onClick) => (
    <button disabled={!enabled} onClick={onClick} style={{
      flex: 1, display: 'flex', alignItems: 'center', gap: 10, textAlign: 'left',
      padding: '11px 13px', borderRadius: theme.radius === 0 ? 0 : 13,
      background: enabled ? theme.panel2 : 'transparent',
      border: `1px solid ${enabled ? 'transparent' : theme.rule}`,
      color: enabled ? theme.text : theme.faint, cursor: enabled ? 'pointer' : 'default', opacity: enabled ? 1 : .55,
    }}>
      <span style={{ color: enabled ? theme.accent : theme.faint, display: 'flex' }}><Ico size={18} /></span>
      <span style={{ minWidth: 0 }}>
        <span style={{ display: 'block', fontSize: 13, fontWeight: 600, fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody }}>{tx(theme, label)}</span>
        <span style={{ display: 'block', fontSize: 10, color: theme.dim, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
          {enabled ? tx(theme, place.title) : tx(theme, 'Nothing')}
        </span>
      </span>
    </button>
  );

  return (
    <div style={{ position: 'absolute', inset: 0, zIndex: 60, display: 'flex', flexDirection: 'column', justifyContent: 'flex-end' }}>
      {/* backdrop */}
      <button onClick={() => dispatch({ type: 'SHELF_CLOSE' })} style={{
        position: 'absolute', inset: 0, background: 'rgba(0,0,0,.45)', border: 'none', cursor: 'pointer',
        animation: 'fadeIn .2s ease', backdropFilter: 'blur(1px)',
      }} />
      {/* sheet */}
      <div style={{
        position: 'relative', background: glass || theme.panel,
        backdropFilter: glass ? 'blur(34px) saturate(180%)' : 'none',
        WebkitBackdropFilter: glass ? 'blur(34px) saturate(180%)' : 'none',
        borderTopLeftRadius: sheetR, borderTopRightRadius: sheetR,
        borderTop: `1px solid ${theme.rule}`, boxShadow: '0 -24px 60px rgba(0,0,0,.5)',
        padding: '10px 18px calc(20px + env(safe-area-inset-bottom))', animation: 'shelfUp .32s cubic-bezier(.32,.72,0,1)',
        maxHeight: '86%', overflowY: 'auto', color: theme.text, fontFamily: theme.fontBody,
      }}>
        {/* grabber */}
        <div style={{ width: 38, height: 5, borderRadius: 3, background: theme.faint, margin: '2px auto 12px' }} />

        {/* header */}
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 14 }}>
          <div>
            <div style={{ fontFamily: theme.fontDisplay, fontSize: theme.serif ? 30 : 24, fontWeight: theme.serif ? 400 : 700, letterSpacing: theme.upper ? '.04em' : '-.01em' }}>{tx(theme, 'Shelf')}</div>
            <div style={{ fontSize: 12, color: theme.dim, marginTop: 1, fontStyle: theme.serif ? 'italic' : 'normal' }}>{tx(theme, 'Pin a place. Jump back any time.')}</div>
          </div>
          <button onClick={() => dispatch({ type: 'SHELF_CLOSE' })} style={{
            width: 30, height: 30, borderRadius: '50%', border: 'none', cursor: 'pointer',
            background: theme.panel2, color: theme.dim, display: 'flex', alignItems: 'center', justifyContent: 'center',
          }}><IconClose size={15} /></button>
        </div>

        {/* undo / redo */}
        <div style={{ fontFamily: theme.fontMono, fontSize: 9, letterSpacing: '.18em', color: theme.accent, textTransform: 'uppercase', marginBottom: 7 }}>
          {theme.upper ? '[ HISTORY ]' : 'History'}
        </div>
        <div style={{ display: 'flex', gap: 8, marginBottom: 18 }}>
          {navPill('Undo', prevPlace || {}, canUndo, IconUndo, () => dispatch({ type: 'UNDO' }))}
          {navPill('Redo', nextPlace || {}, canRedo, IconRedo, () => dispatch({ type: 'REDO' }))}
        </div>

        {/* current view + save */}
        <div style={{ fontFamily: theme.fontMono, fontSize: 9, letterSpacing: '.18em', color: theme.accent, textTransform: 'uppercase', marginBottom: 7 }}>
          {theme.upper ? '[ NOW VIEWING ]' : 'Now viewing'}
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '10px 12px', borderRadius: theme.radius === 0 ? 0 : 15, background: hexA(theme.accent, .08), border: `1px solid ${hexA(theme.accent, .4)}`, marginBottom: 18 }}>
          <PlaceTile place={here} theme={theme} size={46} />
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 15, fontWeight: 600, fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, here.title)}</div>
            <div style={{ fontSize: 11, color: theme.dim, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, here.sub)}</div>
          </div>
          <button onClick={() => dispatch({ type: 'SHELF_SAVE' })} style={{
            display: 'flex', alignItems: 'center', gap: 6, padding: '9px 15px', borderRadius: theme.radius === 0 ? 0 : 999,
            background: theme.accent, color: theme.onAccent, border: 'none', cursor: 'pointer',
            fontFamily: theme.fontMono, fontSize: 11, fontWeight: 700, letterSpacing: '.04em', flexShrink: 0,
          }}><IconBookmark size={14} />{tx(theme, 'Save')}</button>
        </div>

        {/* shelves */}
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 7 }}>
          <span style={{ fontFamily: theme.fontMono, fontSize: 9, letterSpacing: '.18em', color: theme.accent, textTransform: 'uppercase' }}>
            {theme.upper ? '[ SHELVES ]' : 'Shelves'}
          </span>
          <span style={{ fontFamily: theme.fontMono, fontSize: 10, color: theme.dim }}>{slots.filter(Boolean).length} / {slots.length}</span>
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {slots.map((snap, i) => {
            if (!snap) {
              return (
                <button key={i} onClick={() => dispatch({ type: 'SHELF_SAVE', slot: i })} style={{
                  display: 'flex', alignItems: 'center', gap: 12, padding: '12px 14px', width: '100%', textAlign: 'left',
                  borderRadius: theme.radius === 0 ? 0 : 15, background: 'transparent',
                  border: `1.5px dashed ${theme.rule}`, cursor: 'pointer', color: theme.dim,
                }}>
                  <span style={{ width: 46, height: 46, borderRadius: theme.radius > 8 ? 9 : theme.radius, border: `1.5px dashed ${theme.rule}`, display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0 }}>
                    <IconPlus size={18} />
                  </span>
                  <span style={{ flex: 1 }}>
                    <span style={{ display: 'block', fontSize: 14, color: theme.text, fontWeight: 500, fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody }}>{tx(theme, `Shelf ${i + 1}`)}</span>
                    <span style={{ display: 'block', fontSize: 11, marginTop: 1 }}>{tx(theme, 'Empty · tap to save this view')}</span>
                  </span>
                </button>
              );
            }
            const pl = describePlace(snap);
            return (
              <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '10px 12px 10px 12px', borderRadius: theme.radius === 0 ? 0 : 15, background: theme.panel2, border: `1px solid ${theme.rule}` }}>
                <button onClick={() => dispatch({ type: 'SHELF_RESTORE', slot: i })} style={{ display: 'flex', alignItems: 'center', gap: 12, flex: 1, minWidth: 0, background: 'none', border: 'none', cursor: 'pointer', textAlign: 'left', color: 'inherit', padding: 0 }}>
                  <PlaceTile place={pl} theme={theme} size={46} />
                  <span style={{ flex: 1, minWidth: 0 }}>
                    <span style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                      <span style={{ fontFamily: theme.fontMono, fontSize: 8, letterSpacing: '.12em', color: theme.dim }}>{tx(theme, `SHELF ${i + 1}`)}</span>
                    </span>
                    <span style={{ display: 'block', fontSize: 15, fontWeight: 600, fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, pl.title)}</span>
                    <span style={{ display: 'block', fontSize: 11, color: theme.dim, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, pl.sub)}</span>
                  </span>
                  <span style={{ display: 'flex', alignItems: 'center', gap: 4, color: theme.accent, fontFamily: theme.fontMono, fontSize: 10, flexShrink: 0 }}>
                    {tx(theme, 'Open')}<IconChevron size={13} />
                  </span>
                </button>
                <button onClick={() => dispatch({ type: 'SHELF_CLEAR', slot: i })} title="Clear" style={{
                  width: 26, height: 26, borderRadius: '50%', border: 'none', cursor: 'pointer', flexShrink: 0,
                  background: hexA(theme.hot, .12), color: theme.hot, display: 'flex', alignItems: 'center', justifyContent: 'center',
                }}><IconClose size={13} /></button>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { describePlace, PlaceIcon, PlaceTile, ShelfControls, ShelfSheet });
