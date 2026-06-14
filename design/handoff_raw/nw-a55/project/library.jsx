// ────────────────────────────────────────────────────────────────
// library.jsx — dense, Spotify-style Library + Apple-Music album page.
//   LibraryScreen — search, segmented tabs (Albums/Songs/Artists/
//                   Playlists), grid-density + list view, format filter.
//   AlbumScreen   — album detail: big art, Play/Shuffle, dense tracklist,
//                   and a "Save to Shelf" action.
// ────────────────────────────────────────────────────────────────

function artRadius(theme) { return theme.id === 'terminal' ? 0 : Math.min(theme.radius, 10); }

// Apple-style segmented control.
function Segmented({ items, value, onChange, theme }) {
  return (
    <div style={{
      display: 'grid', gridTemplateColumns: `repeat(${items.length},1fr)`, gap: 2,
      background: theme.panel2, borderRadius: theme.radius === 0 ? 0 : 10, padding: 3, margin: '0 20px',
    }}>
      {items.map(it => {
        const on = value === it;
        return (
          <button key={it} onClick={() => onChange(it)} style={{
            padding: '7px 4px', borderRadius: theme.radius === 0 ? 0 : 8, border: 'none', cursor: 'pointer',
            background: on ? (theme.scheme === 'light' ? '#fff' : theme.panel) : 'transparent',
            color: on ? theme.text : theme.dim, fontFamily: theme.fontMono, fontSize: 11,
            fontWeight: on ? 700 : 500, letterSpacing: theme.upper ? '.06em' : '.01em',
            boxShadow: on ? '0 1px 4px rgba(0,0,0,.18)' : 'none', transition: 'all .15s',
          }}>{tx(theme, it)}</button>
        );
      })}
    </div>
  );
}

function Chips({ items, value, onChange, theme }) {
  return (
    <div style={{ display: 'flex', gap: 6, padding: '0 20px', flexWrap: 'wrap' }}>
      {items.map(c => {
        const on = value === c;
        return (
          <button key={c} onClick={() => onChange(c)} style={{
            padding: '5px 12px', borderRadius: theme.radius === 0 ? 0 : 999,
            border: `1px solid ${on ? theme.accent : theme.rule}`, background: on ? hexA(theme.accent, .12) : 'transparent',
            color: on ? theme.accent : theme.dim, fontFamily: theme.fontMono, fontSize: 10, letterSpacing: '.04em', cursor: 'pointer',
          }}>{tx(theme, c)}</button>
        );
      })}
    </div>
  );
}

function ViewToggle({ value, onChange, theme }) {
  const opts = [['grid2', IconGrid], ['grid3', IconGrid3], ['list', IconList]];
  return (
    <div style={{ display: 'flex', gap: 2, background: theme.panel2, borderRadius: theme.radius === 0 ? 0 : 8, padding: 3 }}>
      {opts.map(([id, Ico]) => {
        const on = value === id;
        return (
          <button key={id} onClick={() => onChange(id)} style={{
            width: 30, height: 26, borderRadius: theme.radius === 0 ? 0 : 6, border: 'none', cursor: 'pointer',
            background: on ? (theme.scheme === 'light' ? '#fff' : theme.panel) : 'transparent',
            color: on ? theme.accent : theme.dim, display: 'flex', alignItems: 'center', justifyContent: 'center',
            boxShadow: on ? '0 1px 3px rgba(0,0,0,.2)' : 'none',
          }}><Ico size={15} /></button>
        );
      })}
    </div>
  );
}

function LibraryScreen() {
  const { state, dispatch, theme } = usePlayer();
  const [tab, setTab] = useState('Albums');
  const [view, setView] = useState('grid2');
  const [fmt, setFmt] = useState('All');
  const [q, setQ] = useState('');
  const aR = artRadius(theme);
  const ql = q.trim().toLowerCase();
  const matchA = (a) => (fmt === 'All' || a.fmt === fmt) && (!ql || (a.title + a.artist).toLowerCase().includes(ql));
  const matchT = (t) => (fmt === 'All' || t.codec === fmt) && (!ql || (t.title + t.artist + t.album).toLowerCase().includes(ql));
  const albums = ALBUMS.map((a, i) => ({ a, i })).filter(({ a }) => matchA(a));
  const songs = TRACKS.map((t, i) => ({ t, i })).filter(({ t }) => matchT(t));
  const cols = view === 'grid3' ? 3 : 2;

  return (
    <ScreenShell>
      <StatusBarX />
      <Header title="Library" right={<span style={{ fontFamily: theme.fontMono, fontSize: 10, color: theme.dim }}>{tx(theme, `${ALBUMS.length} · ${TRACKS.length}`)}</span>} />

      {/* search */}
      <div style={{ padding: '0 20px 12px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '9px 13px', borderRadius: theme.radius === 0 ? 0 : 11, background: theme.panel2 }}>
          <IconSearch size={15} style={{ color: theme.dim }} />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder={tx(theme, 'Search albums, songs, artists')} style={{
            flex: 1, background: 'none', border: 'none', outline: 'none', color: theme.text,
            fontFamily: theme.fontBody, fontSize: 14,
          }} />
          {q && <button onClick={() => setQ('')} style={{ background: 'none', border: 'none', cursor: 'pointer', color: theme.dim, display: 'flex' }}><IconClose size={14} /></button>}
        </div>
      </div>

      <Segmented items={['Albums', 'Songs', 'Artists', 'Playlists']} value={tab} onChange={setTab} theme={theme} />

      {/* controls row */}
      {(tab === 'Albums' || tab === 'Songs') && (
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 10, padding: '12px 20px 4px' }}>
          <Chips items={['All', 'FLAC', 'DSD', 'MP3']} value={fmt} onChange={setFmt} theme={theme} />
          {tab === 'Albums' && <ViewToggle value={view} onChange={setView} theme={theme} />}
        </div>
      )}

      {/* ALBUMS */}
      {tab === 'Albums' && (view === 'list' ? (
        <div style={{ padding: '6px 0 0' }}>
          {albums.map(({ a, i }) => (
            <button key={i} onClick={() => dispatch({ type: 'OPEN_ALBUM', i })} style={{
              display: 'flex', alignItems: 'center', gap: 12, width: '100%', textAlign: 'left',
              padding: '8px 20px', background: 'none', border: 'none', borderBottom: `1px solid ${theme.rule}`, cursor: 'pointer', color: 'inherit',
            }}>
              <Art kind={a.art} size={52} label={false} style={{ borderRadius: aR }} />
              <span style={{ flex: 1, minWidth: 0 }}>
                <span style={{ display: 'block', fontSize: 14, fontWeight: 600, fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, a.title)}</span>
                <span style={{ display: 'block', fontSize: 11, color: theme.dim, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, `${a.artist} · ${a.yr}`)}</span>
              </span>
              <span style={{ fontFamily: theme.fontMono, fontSize: 9, color: a.fmt === 'DSD' || a.fmt === 'FLAC' ? theme.accent : theme.dim }}>{a.fmt}</span>
              <IconChevron size={14} style={{ color: theme.faint }} />
            </button>
          ))}
        </div>
      ) : (
        <div style={{ display: 'grid', gridTemplateColumns: `repeat(${cols},1fr)`, gap: cols === 3 ? 8 : 12, padding: '8px 20px' }}>
          {albums.map(({ a, i }) => (
            <button key={i} onClick={() => dispatch({ type: 'OPEN_ALBUM', i })} style={{ background: 'none', border: 'none', padding: 0, cursor: 'pointer', textAlign: 'left' }}>
              <Art kind={a.art} size={196} label={false} style={{ width: '100%', aspectRatio: '1', height: 'auto', borderRadius: aR, boxShadow: theme.id === 'terminal' ? `0 0 0 1px ${theme.rule}` : '0 8px 20px rgba(0,0,0,.4)' }} />
              <div style={{ marginTop: 6, minWidth: 0 }}>
                <div style={{ fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody, fontSize: cols === 3 ? 11 : 13, fontWeight: theme.serif ? 400 : 600, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, a.title)}</div>
                <div style={{ fontSize: cols === 3 ? 9 : 10, color: theme.dim, fontFamily: theme.fontMono, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, a.artist)}</div>
              </div>
            </button>
          ))}
        </div>
      ))}

      {/* SONGS — dense list */}
      {tab === 'Songs' && (
        <div style={{ padding: '6px 0 0' }}>
          {songs.map(({ t, i }) => {
            const cur = i === state.trackIdx;
            return (
              <button key={t.id} onClick={() => dispatch({ type: 'PICK_TRACK', i })} style={{
                display: 'grid', gridTemplateColumns: '44px 1fr auto auto', gap: 11, alignItems: 'center',
                width: '100%', padding: '7px 20px', background: cur ? hexA(theme.accent, .07) : 'none',
                border: 'none', borderBottom: `1px solid ${theme.rule}`, cursor: 'pointer', color: 'inherit', textAlign: 'left',
              }}>
                <Art kind={t.art} size={44} label={false} style={{ borderRadius: aR }} />
                <span style={{ minWidth: 0 }}>
                  <span style={{ display: 'block', fontSize: 14, fontWeight: 500, color: cur ? theme.accent : theme.text, fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, t.title)}</span>
                  <span style={{ display: 'block', fontSize: 11, color: theme.dim, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, `${t.artist} · ${t.album}`)}</span>
                </span>
                <span style={{ fontFamily: theme.fontMono, fontSize: 8, color: t.codec === 'MP3' ? theme.dim : theme.accent, border: `1px solid ${t.codec === 'MP3' ? theme.rule : hexA(theme.accent, .5)}`, padding: '1px 5px', borderRadius: 4 }}>{t.codec}</span>
                <span style={{ fontFamily: theme.fontMono, fontSize: 11, color: theme.dim }}>{t.dur}</span>
              </button>
            );
          })}
        </div>
      )}

      {/* ARTISTS */}
      {tab === 'Artists' && (
        <div style={{ padding: '6px 0 0' }}>
          {artistsFromAlbums().filter(ar => !ql || ar.name.toLowerCase().includes(ql)).map((ar, k) => {
            const ai = ALBUMS.findIndex(a => a.artist === ar.name);
            return (
              <button key={k} onClick={() => dispatch({ type: 'OPEN_ALBUM', i: ai < 0 ? 0 : ai })} style={{
                display: 'flex', alignItems: 'center', gap: 13, width: '100%', textAlign: 'left',
                padding: '9px 20px', background: 'none', border: 'none', borderBottom: `1px solid ${theme.rule}`, cursor: 'pointer', color: 'inherit',
              }}>
                <Art kind={ar.art} size={48} label={false} style={{ borderRadius: '50%' }} />
                <span style={{ flex: 1, minWidth: 0 }}>
                  <span style={{ display: 'block', fontSize: 15, fontWeight: 600, fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody }}>{tx(theme, ar.name)}</span>
                  <span style={{ display: 'block', fontSize: 11, color: theme.dim }}>{tx(theme, `${ar.count} album${ar.count > 1 ? 's' : ''}`)}</span>
                </span>
                <IconChevron size={15} style={{ color: theme.faint }} />
              </button>
            );
          })}
        </div>
      )}

      {/* PLAYLISTS */}
      {tab === 'Playlists' && (
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12, padding: '10px 20px' }}>
          {PLAYLISTS.map((p, k) => (
            <button key={k} onClick={() => { setTab('Songs'); }} style={{ background: 'none', border: 'none', padding: 0, cursor: 'pointer', textAlign: 'left' }}>
              <div style={{ position: 'relative' }}>
                <Art kind={p.art} size={196} label={false} style={{ width: '100%', aspectRatio: '1', height: 'auto', borderRadius: aR, boxShadow: theme.id === 'terminal' ? `0 0 0 1px ${theme.rule}` : '0 8px 20px rgba(0,0,0,.4)' }} />
                <span style={{ position: 'absolute', left: 8, bottom: 8, width: 30, height: 30, borderRadius: '50%', background: hexA('#000', .4), backdropFilter: 'blur(6px)', color: '#fff', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                  {p.icon === 'heart' ? <IconHeartFill size={14} /> : p.icon === 'clock' ? <IconClock size={14} /> : p.icon === 'moon' ? <IconMoon size={14} /> : p.icon === 'shuffle' ? <IconShuffle size={14} /> : p.icon === 'check' ? <IconCheck size={14} /> : <IconCheck size={14} />}
                </span>
              </div>
              <div style={{ marginTop: 7, minWidth: 0 }}>
                <div style={{ fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody, fontSize: 14, fontWeight: 600, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, p.name)}</div>
                <div style={{ fontSize: 10, color: theme.dim, fontFamily: theme.fontMono, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, p.sub)}</div>
              </div>
            </button>
          ))}
        </div>
      )}

      {(tab === 'Albums' && albums.length === 0) || (tab === 'Songs' && songs.length === 0) ? (
        <div style={{ textAlign: 'center', padding: '40px 24px', color: theme.dim, fontSize: 13, fontStyle: theme.serif ? 'italic' : 'normal' }}>{tx(theme, `No results for "${q}"`)}</div>
      ) : null}
    </ScreenShell>
  );
}

// ─── ALBUM PAGE (Apple Music style) ─────────────────────────────
function AlbumScreen() {
  const { state, dispatch, theme } = usePlayer();
  const a = ALBUMS[state.albumIdx] || ALBUMS[0];
  const list = tracksForAlbum(state.albumIdx);
  const rep = (() => { const i = TRACKS.findIndex(t => t.art === a.art); return i < 0 ? 0 : i; })();
  const total = list.reduce((acc, t) => { const [m, s] = t.dur.split(':').map(Number); return acc + m * 60 + s; }, 0);
  const mins = Math.round(total / 60);
  const aR = artRadius(theme);
  const playing = state.route === 'now';

  const bigBtn = (label, Ico, primary, onClick) => (
    <button onClick={onClick} style={{
      flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 8,
      padding: '13px 0', borderRadius: theme.radius === 0 ? 0 : 12,
      background: primary ? theme.accent : hexA(theme.accent, .14),
      color: primary ? theme.onAccent : theme.accent, border: 'none', cursor: 'pointer',
      fontFamily: theme.fontMono, fontSize: 13, fontWeight: 700, letterSpacing: '.04em',
    }}><Ico size={17} />{tx(theme, label)}</button>
  );

  return (
    <ScreenShell>
      <StatusBarX />
      {/* back row */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '2px 16px 8px' }}>
        <button onClick={() => dispatch({ type: 'BACK' })} style={{ display: 'flex', alignItems: 'center', gap: 3, background: 'none', border: 'none', color: theme.accent, cursor: 'pointer', fontFamily: theme.fontBody, fontSize: 14, padding: 4 }}>
          <IconBack size={20} />{tx(theme, 'Library')}
        </button>
        <button onClick={() => { dispatch({ type: 'SHELF_SAVE' }); dispatch({ type: 'SHELF_OPEN' }); }} title="Save to Shelf" style={{ display: 'flex', alignItems: 'center', gap: 5, background: theme.panel2, border: 'none', color: theme.accent, cursor: 'pointer', fontFamily: theme.fontMono, fontSize: 10, padding: '6px 11px', borderRadius: 999 }}>
          <IconBookmark size={13} />{tx(theme, 'Shelf')}
        </button>
      </div>

      {/* hero */}
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', padding: '4px 24px 0', textAlign: 'center' }}>
        <Art kind={a.art} size={Math.round(216 * theme.artScale)} label={false} style={{ borderRadius: aR, boxShadow: theme.id === 'terminal' ? `0 0 0 1px ${theme.accent}` : '0 20px 44px rgba(0,0,0,.5)' }} />
        <div style={{ fontFamily: theme.fontDisplay, fontSize: theme.serif ? 32 : 24, fontWeight: theme.serif ? 400 : 700, lineHeight: 1.1, marginTop: 16, letterSpacing: theme.upper ? '.02em' : '-.01em' }}>{tx(theme, a.title)}</div>
        <div style={{ fontSize: 15, color: theme.accent, marginTop: 4, fontWeight: 600, fontStyle: theme.serif ? 'italic' : 'normal' }}>{tx(theme, a.artist)}</div>
        <div style={{ fontFamily: theme.fontMono, fontSize: 10, color: theme.dim, marginTop: 6, letterSpacing: '.04em' }}>
          {tx(theme, `${a.fmt} · ${a.yr} · ${list.length} songs · ${mins} min`)}
        </div>
      </div>

      {/* actions */}
      <div style={{ display: 'flex', gap: 10, padding: '16px 20px 6px' }}>
        {bigBtn('Play', IconPlay, true, () => dispatch({ type: 'PICK_TRACK', i: rep }))}
        {bigBtn('Shuffle', IconShuffle, false, () => { dispatch({ type: 'SHUFFLE' }); dispatch({ type: 'PICK_TRACK', i: rep }); })}
      </div>

      {/* tracklist */}
      <div style={{ padding: '6px 0 0' }}>
        {list.map((t, k) => (
          <button key={k} onClick={() => dispatch({ type: 'PICK_TRACK', i: rep })} style={{
            display: 'grid', gridTemplateColumns: '26px 1fr auto', gap: 12, alignItems: 'center',
            width: '100%', padding: '11px 22px', background: 'none', border: 'none',
            borderBottom: `1px solid ${theme.rule}`, cursor: 'pointer', color: 'inherit', textAlign: 'left',
          }}>
            <span style={{ fontFamily: theme.fontMono, fontSize: 12, color: theme.dim, textAlign: 'center' }}>{t.n}</span>
            <span style={{ fontSize: 14, fontWeight: 500, fontFamily: theme.serif ? theme.fontDisplay : theme.fontBody, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{tx(theme, t.title)}</span>
            <span style={{ fontFamily: theme.fontMono, fontSize: 11, color: theme.dim }}>{t.dur}</span>
          </button>
        ))}
      </div>

      <div style={{ padding: '14px 22px 6px', fontFamily: theme.fontMono, fontSize: 10, color: theme.dim, lineHeight: 1.6 }}>
        {tx(theme, `${list.length} songs · ${mins} minutes`)}<br />
        {tx(theme, `${a.artist} · ${a.fmt === 'DSD' ? 'DSD 5.6 MHz' : a.fmt === 'FLAC' ? 'FLAC 24-bit/96 kHz' : 'MP3 256 kbps'} · ${a.yr}`)}
      </div>
    </ScreenShell>
  );
}

Object.assign(window, { LibraryScreen, AlbumScreen, Segmented, Chips, ViewToggle, artRadius });
