// ────────────────────────────────────────────────────────────────
// finalists-c.jsx — Candidate C · "LEDGER"
// Terminal evolved: Departure Mono everywhere, phosphor amber,
// boxed sections, corner ticks, block-segment progress. `night`
// prop = darker palette + dimmed art. Lock screen separate.
// ────────────────────────────────────────────────────────────────

const CC_FONTS = { mono: "'Departure Mono', 'JetBrains Mono', monospace" };

const CC_DAY = {
  bg: '#0c0c0a', line: '#26241c', panel: '#11110d',
  ink: '#e6e2d2', dim: '#8f8b7c', faint: '#57544a',
  acc: '#f0a420', accInk: '#161002', artDim: 1, ...CC_FONTS,
};
const CC_NIGHT = {
  bg: '#000000', line: '#151310', panel: '#0a0a08',
  ink: '#7e7765', dim: '#4e493c', faint: '#332f27',
  acc: '#7c5510', accInk: '#000000', artDim: 0.28, ...CC_FONTS,
};
const ccPal = (night) => (night ? CC_NIGHT : CC_DAY);

function CCScr({ CC, children, label }) {
  return (
    <div data-screen-label={label} style={{
      width: 480, height: 800, background: CC.bg, color: CC.ink,
      fontFamily: CC.mono, letterSpacing: '.02em',
      display: 'flex', flexDirection: 'column', overflow: 'hidden', userSelect: 'none',
    }}>{children}</div>
  );
}

function CCStatus({ CC, night }) {
  return (
    <div style={{
      height: 34, display: 'flex', alignItems: 'center', justifyContent: 'space-between',
      padding: '0 18px', fontSize: 11, color: CC.dim,
      borderBottom: `1px solid ${CC.line}`, flexShrink: 0,
    }}>
      <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
        <span>14:32</span>
        <span style={{ color: CC.acc }}>[FLAC 24/96]</span>
        {night && <span style={{ color: CC.faint }}>[NIGHT]</span>}
      </div>
      <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
        <span>SHELF·2</span>
        <span>BT</span>
        <span>78% <span style={{ letterSpacing: 0 }}>▮▮▮▯</span></span>
      </div>
    </div>
  );
}

function CCBlocks({ CC, pct = 39, n = 24 }) {
  const filled = Math.round((pct / 100) * n);
  return (
    <div style={{ display: 'flex', gap: 3 }}>
      {Array.from({ length: n }, (_, i) => (
        <div key={i} style={{
          flex: 1, height: 10,
          background: i < filled ? CC.acc : CC.line,
        }}></div>
      ))}
    </div>
  );
}

function CCHeader({ CC, title, right }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '16px 18px 14px', flexShrink: 0 }}>
      <span style={{ fontSize: 22, color: CC.ink }}>&lt; {title.toUpperCase()}</span>
      {right}
    </div>
  );
}

// corner-tick frame around the art
function CCArt({ CC, size = 420 }) {
  const t = { position: 'absolute', width: 14, height: 14, borderColor: CC.acc, borderStyle: 'solid' };
  return (
    <div style={{ position: 'relative', width: size, height: size, padding: 8 }}>
      <div style={{ ...t, top: 0, left: 0, borderWidth: '2px 0 0 2px' }}></div>
      <div style={{ ...t, top: 0, right: 0, borderWidth: '2px 2px 0 0' }}></div>
      <div style={{ ...t, bottom: 0, left: 0, borderWidth: '0 0 2px 2px' }}></div>
      <div style={{ ...t, bottom: 0, right: 0, borderWidth: '0 2px 2px 0' }}></div>
      <div className="art" data-art={FTRX.art} style={{ width: '100%', height: '100%', opacity: CC.artDim }}></div>
    </div>
  );
}

// ─── C · Now Playing ───────────────────────────────────────────
function CCNowPlaying({ night }) {
  const CC = ccPal(night);
  return (
    <CCScr CC={CC} label={`C · Now Playing${night ? ' · Night' : ''}`}>
      <CCStatus CC={CC} night={night} />
      <div style={{ display: 'flex', justifyContent: 'center', paddingTop: 14 }}>
        <CCArt CC={CC} size={428} />
      </div>
      <div style={{ padding: '16px 26px 0' }}>
        <FBars n={40} seed={6} h={26} gap={2} color={CC.acc} dimColor={CC.line} />
      </div>
      <div style={{ padding: '16px 26px 0' }}>
        <div style={{ fontSize: 23, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{FTRX.title.toUpperCase()}</div>
        <div style={{ fontSize: 12, color: CC.dim, marginTop: 5 }}>{FTRX.artist}</div>
        <div style={{ fontSize: 10, color: CC.acc, marginTop: 7 }}>FLAC · 24BIT/96.0KHZ · 2304KBPS</div>
      </div>
      <div style={{ padding: '14px 26px 0' }}>
        <CCBlocks CC={CC} pct={FTRX.pct} />
        <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 7, fontSize: 10, color: CC.dim }}>
          <span>{FTRX.cur}</span><span style={{ color: CC.faint }}>{FTRX.rem}</span>
        </div>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '6px 30px 0', flex: 1 }}>
        <span style={{ fontSize: 11, color: CC.faint }}>SHFL</span>
        <span style={{ width: 58, height: 58, border: `1px solid ${CC.line}`, display: 'flex', alignItems: 'center', justifyContent: 'center' }}><FIPrev size={24} /></span>
        <span style={{ width: 76, height: 76, background: CC.acc, color: CC.accInk, display: 'flex', alignItems: 'center', justifyContent: 'center' }}><FIPause size={28} /></span>
        <span style={{ width: 58, height: 58, border: `1px solid ${CC.line}`, display: 'flex', alignItems: 'center', justifyContent: 'center' }}><FINext size={24} /></span>
        <span style={{ fontSize: 11, color: CC.acc }}>RPT·1</span>
      </div>
      <div style={{
        borderTop: `1px solid ${CC.line}`, height: 56, display: 'flex', alignItems: 'stretch', flexShrink: 0,
        fontSize: 10, color: CC.dim, textAlign: 'center',
      }}>
        {['♥ LIKED', 'QUEUE', 'EQ', 'BT', 'SOUND'].map((t, i) => (
          <span key={t} style={{
            flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center',
            borderLeft: i ? `1px solid ${CC.line}` : 'none', color: i === 0 ? CC.acc : CC.dim,
          }}>{t}</span>
        ))}
      </div>
    </CCScr>
  );
}

// ─── C · Lock screen ───────────────────────────────────────────
function CCLock({ night }) {
  const CC = ccPal(night);
  return (
    <CCScr CC={CC} label={`C · Lock Screen${night ? ' · Night' : ''}`}>
      <CCStatus CC={CC} night={night} />
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center' }}>
        <div style={{ fontSize: 76, color: CC.ink }}>23:41</div>
        <div style={{ fontSize: 13, color: CC.ink, marginTop: 24 }}>{FTRX.title.toUpperCase()}</div>
        <div style={{ fontSize: 10, color: CC.dim, marginTop: 7 }}>{FTRX.artist}</div>
        <div style={{ width: 250, marginTop: 26 }}>
          <CCBlocks CC={CC} pct={FTRX.pct} n={20} />
        </div>
      </div>
      <div style={{ height: 54, display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 9, letterSpacing: '.14em', color: CC.faint, borderTop: `1px solid ${CC.line}`, flexShrink: 0 }}>
        [LOCKED] KEYS ACTIVE · TAP ×2 TO WAKE
      </div>
    </CCScr>
  );
}

// ─── C · Menu ──────────────────────────────────────────────────
function CCMenu({ night }) {
  const CC = ccPal(night);
  return (
    <CCScr CC={CC} label={`C · Menu${night ? ' · Night' : ''}`}>
      <CCStatus CC={CC} night={night} />
      <CCHeader CC={CC} title="Menu" right={<span style={{ fontSize: 10, color: CC.faint }}>NW-A55 v1.0</span>} />
      <div style={{ flex: 1, padding: '0 18px' }}>
        {FMENU.map((m, i) => (
          <div key={m.label} style={{
            display: 'flex', alignItems: 'center', height: 62, gap: 12,
            borderBottom: `1px solid ${CC.line}`,
          }}>
            <span style={{ color: i === 0 ? CC.acc : CC.faint, fontSize: 13, width: 16 }}>{i === 0 ? '>' : ' '}</span>
            <span style={{ fontSize: 16, flex: 1, color: i === 0 ? CC.acc : CC.ink }}>{m.label.toUpperCase()}</span>
            <span style={{ fontSize: 9, color: CC.faint }}>{m.value.toUpperCase()}</span>
          </div>
        ))}
      </div>
    </CCScr>
  );
}

// ─── C · Bluetooth ─────────────────────────────────────────────
function CCBluetooth({ night }) {
  const CC = ccPal(night);
  return (
    <CCScr CC={CC} label={`C · Bluetooth${night ? ' · Night' : ''}`}>
      <CCStatus CC={CC} night={night} />
      <CCHeader CC={CC} title="Bluetooth" right={<span style={{ fontSize: 11, color: CC.acc }}>[ON]</span>} />
      <div style={{ margin: '0 18px', border: `1px solid ${CC.acc}`, padding: '14px 16px' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 9 }}>
          <span style={{ color: CC.acc }}>● CONNECTED</span>
          <span style={{ color: CC.dim }}>HP BATT 60%</span>
        </div>
        <div style={{ fontSize: 21, marginTop: 9 }}>WH-1000XM5</div>
        <div style={{ fontSize: 10, color: CC.dim, marginTop: 5 }}>LDAC · 96KHZ · SOUND QUALITY PREF.</div>
        <div style={{ display: 'flex', gap: 8, marginTop: 14, fontSize: 11 }}>
          <span style={{ flex: 1, height: 42, display: 'flex', alignItems: 'center', justifyContent: 'center', border: `1px solid ${CC.line}`, color: CC.dim }}>[DISCONNECT]</span>
          <span style={{ flex: 1, height: 42, display: 'flex', alignItems: 'center', justifyContent: 'center', border: `1px solid ${CC.line}`, color: CC.dim }}>[QUALITY·LDAC]</span>
        </div>
      </div>
      <div style={{ padding: '20px 18px 8px', fontSize: 9, letterSpacing: '.14em', color: CC.faint }}>PAIRED DEVICES — TAP TO CONNECT</div>
      <div style={{ padding: '0 18px' }}>
        {FPAIRED.map((d) => (
          <div key={d.name} style={{ display: 'flex', alignItems: 'center', height: 56, borderBottom: `1px solid ${CC.line}`, gap: 12 }}>
            <span style={{ flex: 1 }}>
              <span style={{ fontSize: 14, display: 'block' }}>{d.name.toUpperCase()}</span>
              <span style={{ fontSize: 9, color: CC.faint }}>{d.kind.toUpperCase()}</span>
            </span>
            <span style={{ fontSize: 11, color: CC.acc }}>[CONNECT]</span>
          </div>
        ))}
      </div>
      <div style={{ marginTop: 'auto', padding: '0 18px 16px' }}>
        <div style={{ height: 50, background: CC.acc, color: CC.accInk, display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 14 }}>
          [ PAIR NEW DEVICE ]
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 11, fontSize: 9, color: CC.faint }}>
          <span>NFC: TOUCH DEVICE TO REAR PANEL</span>
          <span style={{ color: CC.dim }}>RECEIVER MODE &gt;</span>
        </div>
      </div>
    </CCScr>
  );
}

// ─── C · Equalizer ─────────────────────────────────────────────
function CCEq({ night }) {
  const CC = ccPal(night);
  const cells = 13, center = 6;
  return (
    <CCScr CC={CC} label={`C · Equalizer${night ? ' · Night' : ''}`}>
      <CCStatus CC={CC} night={night} />
      <CCHeader CC={CC} title="Equalizer" right={<span style={{ fontSize: 11, color: CC.acc }}>[CUSTOM A1]</span>} />
      <div style={{ display: 'flex', gap: 8, padding: '0 18px 18px', fontSize: 10 }}>
        {['FLAT', 'ROCK', 'JAZZ', 'A1', 'A2'].map((p) => (
          <span key={p} style={{
            padding: '6px 11px', border: `1px solid ${p === 'A1' ? CC.acc : CC.line}`,
            color: p === 'A1' ? CC.accInk : CC.dim, background: p === 'A1' ? CC.acc : 'transparent',
          }}>{p}</span>
        ))}
      </div>
      <div style={{ display: 'flex', gap: 8, padding: '0 22px', height: 380 }}>
        {FBANDS.map((b) => {
          const span = Math.round(Math.abs(b.db) * 0.6);
          return (
            <div key={b.hz} style={{ flex: 1, display: 'flex', flexDirection: 'column' }}>
              <div style={{ textAlign: 'center', fontSize: 9, color: b.db !== 0 ? CC.acc : CC.faint, marginBottom: 8 }}>
                {b.db > 0 ? `+${b.db}` : b.db}
              </div>
              <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 3 }}>
                {Array.from({ length: cells }, (_, i) => {
                  const onAbove = b.db > 0 && i >= center - span && i < center;
                  const onBelow = b.db < 0 && i > center && i <= center + span;
                  const isCenter = i === center;
                  return (
                    <div key={i} style={{
                      flex: 1,
                      background: onAbove ? CC.acc : onBelow ? CC.dim : isCenter ? CC.faint : CC.line,
                    }}></div>
                  );
                })}
              </div>
              <div style={{ textAlign: 'center', fontSize: 9, color: CC.dim, marginTop: 8 }}>{b.hz}</div>
            </div>
          );
        })}
      </div>
      <div style={{ marginTop: 'auto', borderTop: `1px solid ${CC.line}`, height: 56, display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '0 18px', fontSize: 11, flexShrink: 0 }}>
        <span style={{ color: CC.dim }}>[RESET]</span>
        <span style={{ color: CC.acc }}>[SAVE PRESET]</span>
      </div>
    </CCScr>
  );
}

// ─── C · Shelf sheet (pin places + undo/redo) ──────────────────
function CCShelf({ night }) {
  const CC = ccPal(night);
  const slots = [
    { n: 1, title: 'ALBUM · LAST SMOKE BEFORE…', sub: 'TRACK 4 · SAVED 12 MIN AGO' },
    { n: 2, title: 'LIBRARY · ARTISTS · B', sub: 'SAVED 1 HR AGO' },
    { n: 3, title: null },
  ];
  return (
    <CCScr CC={CC} label={`C · Shelf${night ? ' · Night' : ''}`}>
      <CCStatus CC={CC} night={night} />
      {/* dimmed now-playing behind */}
      <div style={{ flex: 1, position: 'relative', overflow: 'hidden' }}>
        <div style={{ padding: '14px 26px 0', opacity: 0.22 }}>
          <CCArt CC={CC} size={428} />
        </div>
        {/* sheet */}
        <div style={{
          position: 'absolute', left: 0, right: 0, bottom: 0,
          background: CC.panel, borderTop: `2px solid ${CC.acc}`, padding: '14px 18px 18px',
        }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 14 }}>
            <span style={{ fontSize: 16 }}>SHELF</span>
            <span style={{ fontSize: 11, color: CC.faint }}>[CLOSE ×]</span>
          </div>

          <div style={{ fontSize: 9, letterSpacing: '.16em', color: CC.acc, marginBottom: 7 }}>[ HISTORY ]</div>
          <div style={{ display: 'flex', gap: 8, marginBottom: 16 }}>
            <span style={{ flex: 1, border: `1px solid ${CC.line}`, padding: '10px 12px', color: CC.ink }}>
              <span style={{ display: 'block', fontSize: 11 }}>‹ UNDO</span>
              <span style={{ display: 'block', fontSize: 9, color: CC.dim, marginTop: 4 }}>LIBRARY · ALBUMS</span>
            </span>
            <span style={{ flex: 1, border: `1px solid ${CC.line}`, padding: '10px 12px', color: CC.faint }}>
              <span style={{ display: 'block', fontSize: 11 }}>REDO ›</span>
              <span style={{ display: 'block', fontSize: 9, marginTop: 4 }}>—</span>
            </span>
          </div>

          <div style={{ fontSize: 9, letterSpacing: '.16em', color: CC.acc, marginBottom: 7 }}>[ THIS PLACE ]</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, border: `1px solid ${CC.line}`, padding: '10px 12px', marginBottom: 16 }}>
            <span style={{ flex: 1 }}>
              <span style={{ display: 'block', fontSize: 12 }}>NOW PLAYING · ATLAS HANDS</span>
              <span style={{ display: 'block', fontSize: 9, color: CC.dim, marginTop: 4 }}>1:47 / 4:32</span>
            </span>
            <span style={{ background: CC.acc, color: CC.accInk, fontSize: 10, padding: '8px 12px' }}>[PIN]</span>
          </div>

          <div style={{ fontSize: 9, letterSpacing: '.16em', color: CC.acc, marginBottom: 7 }}>[ PINNED · 2/3 ]</div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            {slots.map((s) => (
              <div key={s.n} style={{
                display: 'flex', alignItems: 'center', gap: 12, padding: '10px 12px',
                border: `1px ${s.title ? 'solid' : 'dashed'} ${CC.line}`,
              }}>
                <span style={{ color: s.title ? CC.acc : CC.faint, fontSize: 12 }}>{s.n}</span>
                {s.title ? (
                  <span style={{ flex: 1 }}>
                    <span style={{ display: 'block', fontSize: 11 }}>{s.title}</span>
                    <span style={{ display: 'block', fontSize: 9, color: CC.dim, marginTop: 3 }}>{s.sub}</span>
                  </span>
                ) : (
                  <span style={{ flex: 1, fontSize: 10, color: CC.faint }}>EMPTY SLOT — PIN HERE</span>
                )}
                {s.title && <span style={{ fontSize: 10, color: CC.acc }}>[GO]</span>}
              </div>
            ))}
          </div>
        </div>
      </div>
    </CCScr>
  );
}

Object.assign(window, { CCNowPlaying, CCLock, CCMenu, CCBluetooth, CCEq, CCShelf });
