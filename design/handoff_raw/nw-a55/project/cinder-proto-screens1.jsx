// ────────────────────────────────────────────────────────────────
// cinder-proto-screens1.jsx — shared chrome + Lock, Now Playing,
// Up Next, Shelf sheet. Registers into window.CSCREENS.
// All components read ctx via window.__useC (set by app file? no —
// context lives in app file; we use a global hook reference).
// To keep Babel scopes simple, the app exposes useC on window.
// ────────────────────────────────────────────────────────────────

window.CSCREENS = window.CSCREENS || {};

// ─── shared chrome ─────────────────────────────────────────────
function CStatus() {
  const c = useC(); const P = c.P;
  const track = CAL_SONGS[c.trackIdx];
  return (
    <div style={{
      height: 34, display: 'flex', alignItems: 'center', justifyContent: 'space-between',
      padding: '0 18px', fontFamily: P.mono, fontSize: 11, letterSpacing: '.06em', color: P.dim, flexShrink: 0,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
        <span>14:32</span>
        <span style={{ border: `1px solid ${P.acc}`, color: P.acc, padding: '1px 6px', fontSize: 9, letterSpacing: '.12em' }}>FLAC 24/96</span>
        {c.night && <span style={{ fontSize: 9, letterSpacing: '.18em', color: P.faint }}>NIGHT</span>}
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
        <span onClick={() => c.go('menu')} style={{ cursor: 'pointer', color: c.cur.id === 'menu' ? P.acc : P.dim, fontSize: 17, lineHeight: 1, padding: '4px 2px', fontFamily: P.sans }}>≡</span>
        <span onClick={() => c.setShelfOpen(true)} style={{ cursor: 'pointer', color: c.shelfOpen ? P.acc : P.dim }}><FIBookmark size={14} /></span>
        <span onClick={() => c.go('bluetooth')} style={{ cursor: 'pointer', color: c.bt.connected ? P.dim : P.faint }}><FIBt size={14} /></span>
        <span style={{ display: 'flex', alignItems: 'center', gap: 5, color: P.faint }}>
          <span style={{ fontSize: 10 }}>78</span><FBatt pct={78} />
        </span>
      </div>
    </div>
  );
}

function CHeader({ title, right }) {
  const c = useC(); const P = c.P;
  return (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '14px 22px 16px', flexShrink: 0 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
        <span onClick={c.back} style={{ color: P.dim, cursor: 'pointer' }}><FIBack /></span>
        <span style={{ fontSize: 27, fontWeight: 700, letterSpacing: '-.01em' }}>{title}</span>
      </div>
      {right}
    </div>
  );
}

// ─── Lock screen ───────────────────────────────────────────────
function CLock() {
  const c = useC(); const P = c.P;
  const [hint, setHint] = React.useState(false);
  const taps = React.useRef(0);
  const track = CAL_SONGS[c.trackIdx];
  const tap = () => {
    taps.current += 1;
    if (taps.current >= 2) { taps.current = 0; c.setLocked(false); return; }
    setHint(true);
    setTimeout(() => { taps.current = 0; setHint(false); }, 1200);
  };
  return (
    <div onClick={tap} style={{ position: 'absolute', inset: 0, background: P.bg, display: 'flex', flexDirection: 'column', cursor: 'pointer', zIndex: 30 }}>
      <CStatus />
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center' }}>
        <div style={{ fontFamily: P.mono, fontSize: 88, fontWeight: 300, letterSpacing: '-.02em' }}>23:41</div>
        <div style={{ marginTop: 26, fontSize: 15 }}>{track.t}</div>
        <div style={{ marginTop: 5, fontSize: 12, color: P.dim }}>{track.a}</div>
        <div style={{ width: 240, marginTop: 24 }}>
          <FProg pct={c.pct} h={2} track={P.line} fill={P.dim} />
        </div>
      </div>
      <div style={{ height: 58, display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 8, fontFamily: P.mono, fontSize: 9, letterSpacing: '.16em', color: hint ? P.acc : P.faint, flexShrink: 0 }}>
        <FILock size={12} /> {hint ? 'TAP AGAIN TO WAKE' : 'LOCKED · SIDE KEYS ACTIVE · TAP TWICE TO WAKE'}
      </div>
    </div>
  );
}

// ─── Now Playing ───────────────────────────────────────────────
function CNowPlaying() {
  const c = useC(); const P = c.P;
  const track = CAL_SONGS[c.trackIdx];
  const tool = (Icon, id, active) => (
    <span onClick={() => c.go(id)} style={{ cursor: 'pointer', color: active ? P.acc : P.dim, width: 44, height: 44, display: 'flex', alignItems: 'center', justifyContent: 'center' }}><Icon /></span>
  );
  return (
    <React.Fragment>
      <CStatus />
      {c.night ? (
        <div style={{ padding: '46px 24px 0', display: 'flex', gap: 18, alignItems: 'center' }}>
          <div className="art" data-art={track.art} style={{ width: 92, height: 92, opacity: 0.32 }}></div>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 21, fontWeight: 700, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{track.t}</div>
            <div style={{ fontSize: 14, color: P.dim, marginTop: 4 }}>{track.a}</div>
            <div style={{ fontFamily: P.mono, fontSize: 10, letterSpacing: '.08em', color: P.acc, marginTop: 9 }}>FLAC · 24bit / 96.0 kHz</div>
          </div>
        </div>
      ) : (
        <React.Fragment>
          <div className="art" data-art={track.art} style={{ width: 480, height: 480, flexShrink: 0 }}></div>
          {c.t.viz
            ? <FBars n={36} seed={2 + c.trackIdx} h={22} gap={3} color={P.acc} dimColor={P.line} style={{ margin: '10px 24px 0', opacity: c.playing ? 1 : 0.35 }} />
            : <div style={{ height: 22, margin: '10px 24px 0' }}></div>}
          <div style={{ padding: '12px 24px 0' }}>
            <div style={{ fontSize: 26, fontWeight: 700, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{track.t}</div>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginTop: 4 }}>
              <span style={{ fontSize: 15, color: P.dim }}>{track.a}</span>
              <span style={{ fontFamily: P.mono, fontSize: 10, letterSpacing: '.08em', color: P.acc }}>FLAC · 24bit / 96.0 kHz</span>
            </div>
          </div>
        </React.Fragment>
      )}
      {c.night && c.t.viz && <FBars n={36} seed={2 + c.trackIdx} h={16} gap={3} color={P.acc} dimColor={P.line} style={{ margin: '40px 24px 0', opacity: c.playing ? 1 : 0.35 }} />}
      <div style={{ padding: '14px 24px 0' }}>
        <div onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          c.setPct(Math.round(((e.clientX - r.left) / r.width) * 100));
        }} style={{ cursor: 'pointer', padding: '6px 0' }}>
          <FProg pct={c.pct} h={4} track={P.line} fill={P.acc} />
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 3, fontFamily: P.mono, fontSize: 11, color: P.dim }}>
          <span>1:47</span><span style={{ color: P.faint }}>-2:45</span>
        </div>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '4px 38px 0', flex: 1 }}>
        <span style={{ color: P.faint, cursor: 'pointer' }}><FIShuffle /></span>
        <span onClick={() => c.setTrackIdx((i) => (i + CAL_SONGS.length - 1) % CAL_SONGS.length)} style={{ color: P.ink, width: 50, height: 50, display: 'flex', alignItems: 'center', justifyContent: 'center', cursor: 'pointer' }}><FIPrev size={28} /></span>
        <span onClick={() => c.setPlaying((p) => !p)} style={{
          width: 68, height: 68, borderRadius: '50%', background: P.acc, color: P.accInk,
          display: 'flex', alignItems: 'center', justifyContent: 'center', cursor: 'pointer',
        }}>{c.playing ? <FIPause size={28} /> : <FIPlay size={28} />}</span>
        <span onClick={() => c.setTrackIdx((i) => (i + 1) % CAL_SONGS.length)} style={{ color: P.ink, width: 50, height: 50, display: 'flex', alignItems: 'center', justifyContent: 'center', cursor: 'pointer' }}><FINext size={28} /></span>
        <span style={{ color: P.acc, cursor: 'pointer' }}><FIRepeat /></span>
      </div>
      <div style={{ borderTop: `1px solid ${P.line}`, height: 62, display: 'flex', alignItems: 'center', justifyContent: 'space-around', flexShrink: 0 }}>
        <span onClick={() => c.setLiked((l) => !l)} style={{ cursor: 'pointer', color: c.liked ? P.acc : P.dim, width: 44, height: 44, display: 'flex', alignItems: 'center', justifyContent: 'center' }}><FIHeart fill={c.liked} /></span>
        {tool(FIQueue, 'upnext')}
        {tool(FIEq, 'eq')}
        {tool(FIBt, 'bluetooth')}
        {tool(FILibrary, 'library')}
      </div>
    </React.Fragment>
  );
}

// ─── Up Next ───────────────────────────────────────────────────
function CUpNext() {
  const c = useC(); const P = c.P;
  return (
    <React.Fragment>
      <CStatus />
      <CHeader title="Up Next" right={<span style={{ fontFamily: P.mono, fontSize: 10, color: P.faint, letterSpacing: '.08em' }}>{CAL_SONGS.length} TRACKS · 41:24</span>} />
      <div style={{ flex: 1, overflow: 'hidden' }}>
        {CAL_SONGS.map((s, i) => {
          const now = i === c.trackIdx;
          return (
            <div key={s.t} onClick={() => { c.setTrackIdx(i); c.setPlaying(true); c.go('nowplaying'); }} style={{
              display: 'flex', alignItems: 'center', gap: 13, height: 62, padding: '0 22px',
              borderBottom: `1px solid ${P.line}`, cursor: 'pointer',
              background: now ? P.panel : 'transparent',
            }}>
              <span style={{ fontFamily: P.mono, fontSize: 10, color: now ? P.acc : P.faint, width: 18 }}>{now ? '▶' : String(i + 1).padStart(2, '0')}</span>
              <div className="art" data-art={s.art} style={{ width: 40, height: 40, opacity: P.artDim }}></div>
              <span style={{ flex: 1, minWidth: 0 }}>
                <span style={{ display: 'block', fontSize: 15, fontWeight: 600, color: now ? P.acc : P.ink, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{s.t}</span>
                <span style={{ display: 'block', fontSize: 11, color: P.dim, marginTop: 2 }}>{s.a}</span>
              </span>
              <span style={{ fontFamily: P.mono, fontSize: 10, color: P.faint }}>{s.d}</span>
              <span style={{ color: P.faint, fontSize: 14, letterSpacing: '-2px' }}>≡</span>
            </div>
          );
        })}
      </div>
      <div style={{ borderTop: `1px solid ${P.line}`, height: 56, display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '0 22px', flexShrink: 0 }}>
        <span style={{ fontSize: 13, fontWeight: 600, color: P.dim, cursor: 'pointer' }}>Clear queue</span>
        <span style={{ fontSize: 13, fontWeight: 700, color: P.acc, cursor: 'pointer' }}>Save as playlist</span>
      </div>
    </React.Fragment>
  );
}

// ─── Shelf sheet (overlay) ─────────────────────────────────────
function CShelfSheet() {
  const c = useC(); const P = c.P;
  const track = CAL_SONGS[c.trackIdx];
  const cap = { fontFamily: P.mono, fontSize: 9, letterSpacing: '.18em', color: P.acc, marginBottom: 8 };
  const pinHere = () => {
    const slot = c.pins.findIndex((p) => !p);
    if (slot === -1) return;
    const next = [...c.pins];
    next[slot] = { title: `Now Playing · ${track.t}`, sub: 'Just now' };
    c.setPins(next);
  };
  return (
    <div onClick={() => c.setShelfOpen(false)} style={{ position: 'absolute', inset: 0, background: 'rgba(0,0,0,.55)', zIndex: 20, display: 'flex', flexDirection: 'column', justifyContent: 'flex-end' }}>
      <div onClick={(e) => e.stopPropagation()} style={{ background: P.panel, borderTop: `1px solid ${P.acc}`, padding: '16px 22px 20px' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
          <span style={{ fontSize: 20, fontWeight: 700, display: 'flex', alignItems: 'center', gap: 9 }}><FIBookmark size={16} /> Shelf</span>
          <span onClick={() => c.setShelfOpen(false)} style={{ fontFamily: P.mono, fontSize: 10, color: P.faint, cursor: 'pointer', padding: 8 }}>CLOSE ×</span>
        </div>
        <div style={cap}>HISTORY</div>
        <div style={{ display: 'flex', gap: 10, marginBottom: 18 }}>
          <span onClick={c.back} style={{ flex: 1, border: `1px solid ${P.line}`, padding: '10px 13px', cursor: 'pointer' }}>
            <span style={{ display: 'block', fontSize: 13, fontWeight: 600 }}>‹ Undo</span>
            <span style={{ display: 'block', fontFamily: P.mono, fontSize: 9, color: P.dim, marginTop: 4 }}>Previous screen</span>
          </span>
          <span style={{ flex: 1, border: `1px solid ${P.line}`, padding: '10px 13px', color: P.faint }}>
            <span style={{ display: 'block', fontSize: 13, fontWeight: 600 }}>Redo ›</span>
            <span style={{ display: 'block', fontFamily: P.mono, fontSize: 9, marginTop: 4 }}>—</span>
          </span>
        </div>
        <div style={cap}>THIS PLACE</div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, border: `1px solid ${P.line}`, padding: '11px 13px', marginBottom: 18 }}>
          <span style={{ flex: 1 }}>
            <span style={{ display: 'block', fontSize: 14, fontWeight: 600 }}>Now Playing · {track.t}</span>
            <span style={{ display: 'block', fontFamily: P.mono, fontSize: 9, color: P.dim, marginTop: 4 }}>1:47 / {track.d}</span>
          </span>
          <span onClick={pinHere} style={{ background: P.acc, color: P.accInk, fontSize: 12, fontWeight: 700, padding: '9px 14px', cursor: 'pointer' }}>Pin</span>
        </div>
        <div style={cap}>PINNED · {c.pins.filter(Boolean).length}/3</div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 9 }}>
          {c.pins.map((s, i) => (
            <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 13, padding: '10px 13px', border: `1px ${s ? 'solid' : 'dashed'} ${P.line}` }}>
              <span style={{ fontFamily: P.mono, fontSize: 11, color: s ? P.acc : P.faint }}>{i + 1}</span>
              {s ? (
                <React.Fragment>
                  <span style={{ flex: 1 }}>
                    <span style={{ display: 'block', fontSize: 13, fontWeight: 600 }}>{s.title}</span>
                    <span style={{ display: 'block', fontFamily: P.mono, fontSize: 9, color: P.dim, marginTop: 3 }}>{s.sub}</span>
                  </span>
                  <span onClick={() => { c.setShelfOpen(false); c.go('nowplaying'); }} style={{ fontFamily: P.mono, fontSize: 10, color: P.acc, cursor: 'pointer', padding: 6 }}>GO ›</span>
                  <span onClick={() => c.setPins(c.pins.map((p, j) => (j === i ? null : p)))} style={{ fontFamily: P.mono, fontSize: 10, color: P.faint, cursor: 'pointer', padding: 6 }}>×</span>
                </React.Fragment>
              ) : (
                <span style={{ flex: 1, fontSize: 12, color: P.faint }}>Empty slot — pin here</span>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

Object.assign(window.CSCREENS, { nowplaying: CNowPlaying, upnext: CUpNext });
Object.assign(window, { CStatus, CHeader, CLock, CShelfSheet });
