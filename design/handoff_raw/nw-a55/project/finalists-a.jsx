// ────────────────────────────────────────────────────────────────
// finalists-a.jsx — Candidate A · "CINDER"
// Evolved Hi-Res direction: near-black, warm amber accent, dense
// instrument-like data display. Hanken Grotesk UI + JetBrains Mono.
// Every screen takes a `night` prop: same layout, darker palette,
// album art dimmed, accents muted. Lock screen is its own screen.
// ────────────────────────────────────────────────────────────────

const CA_FONTS = { mono: "'JetBrains Mono', monospace", sans: "'Hanken Grotesk', sans-serif" };

// Accent tuned to the orange NW-A55 body; neutrals warmed to match
// (no blue cast — the dark UI should read as the same object as the
// orange chassis). Night = embers: same hue, far lower luminance.
const CA_DAY = {
  bg: '#0d0c0b', panel: '#13110f', line: '#221f1b',
  ink: '#ece7df', dim: '#95908a', faint: '#5f5a52',
  acc: '#f4651f', accInk: '#1a0a02', artDim: 1, ...CA_FONTS,
};
const CA_NIGHT = {
  bg: '#000000', panel: '#0a0908', line: '#161310',
  ink: '#8d8170', dim: '#5b5347', faint: '#3b362d',
  acc: '#8f3d10', accInk: '#000000', artDim: 0.28, ...CA_FONTS,
};
const caPal = (night) => (night ? CA_NIGHT : CA_DAY);

function CAScr({ CA, children, label }) {
  return (
    <div data-screen-label={label} style={{
      width: 480, height: 800, background: CA.bg, color: CA.ink, fontFamily: CA.sans,
      display: 'flex', flexDirection: 'column', overflow: 'hidden', userSelect: 'none',
    }}>{children}</div>
  );
}

function CAStatus({ CA, night }) {
  return (
    <div style={{
      height: 34, display: 'flex', alignItems: 'center', justifyContent: 'space-between',
      padding: '0 18px', fontFamily: CA.mono, fontSize: 11, letterSpacing: '.06em',
      color: CA.dim, flexShrink: 0,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
        <span>14:32</span>
        <span style={{
          border: `1px solid ${CA.acc}`, color: CA.acc, padding: '1px 6px',
          fontSize: 9, letterSpacing: '.12em',
        }}>FLAC 24/96</span>
        {night && <span style={{ fontSize: 9, letterSpacing: '.18em', color: CA.faint }}>NIGHT</span>}
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
        <FIBookmark size={14} />
        <FIBt size={14} />
        <span style={{ display: 'flex', alignItems: 'center', gap: 5, color: CA.faint }}>
          <span style={{ fontSize: 10 }}>78</span><FBatt pct={78} />
        </span>
      </div>
    </div>
  );
}

function CAHeader({ CA, title, right }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '14px 22px 16px', flexShrink: 0 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
        <span style={{ color: CA.dim }}><FIBack /></span>
        <span style={{ fontSize: 27, fontWeight: 700, letterSpacing: '-.01em' }}>{title}</span>
      </div>
      {right}
    </div>
  );
}

// ─── A · Now Playing ───────────────────────────────────────────
function CANowPlaying({ night }) {
  const CA = caPal(night);
  return (
    <CAScr CA={CA} label={`A · Now Playing${night ? ' · Night' : ''}`}>
      <CAStatus CA={CA} night={night} />
      {night ? (
        <div style={{ padding: '46px 24px 0', display: 'flex', gap: 18, alignItems: 'center' }}>
          <div className="art" data-art={FTRX.art} style={{ width: 92, height: 92, opacity: 0.32 }}></div>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 21, fontWeight: 700, letterSpacing: '-.01em', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{FTRX.title}</div>
            <div style={{ fontSize: 14, color: CA.dim, marginTop: 4 }}>{FTRX.artist}</div>
            <div style={{ fontFamily: CA.mono, fontSize: 10, letterSpacing: '.08em', color: CA.acc, marginTop: 9 }}>{FTRX.codec} · {FTRX.spec}</div>
          </div>
        </div>
      ) : (
        <React.Fragment>
          <div className="art" data-art={FTRX.art} style={{ width: 480, height: 480, opacity: CA.artDim, flexShrink: 0 }}></div>
          <FBars n={36} seed={2} h={22} gap={3} color={CA.acc} dimColor={CA.line} style={{ margin: '10px 24px 0' }} />
          <div style={{ padding: '12px 24px 0' }}>
            <div style={{ fontSize: 26, fontWeight: 700, letterSpacing: '-.01em', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{FTRX.title}</div>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginTop: 4 }}>
              <span style={{ fontSize: 15, color: CA.dim }}>{FTRX.artist}</span>
              <span style={{ fontFamily: CA.mono, fontSize: 10, letterSpacing: '.08em', color: CA.acc }}>{FTRX.codec} · {FTRX.spec}</span>
            </div>
          </div>
        </React.Fragment>
      )}
      {night && <FBars n={36} seed={2} h={16} gap={3} color={CA.acc} dimColor={CA.line} style={{ margin: '40px 24px 0' }} />}
      <div style={{ padding: '14px 24px 0' }}>
        <FProg pct={FTRX.pct} h={4} track={CA.line} fill={CA.acc} />
        <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 7, fontFamily: CA.mono, fontSize: 11, color: CA.dim }}>
          <span>{FTRX.cur}</span><span style={{ color: CA.faint }}>{FTRX.rem}</span>
        </div>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '4px 38px 0', flex: 1 }}>
        <span style={{ color: CA.faint }}><FIShuffle /></span>
        <span style={{ color: CA.ink, width: 50, height: 50, display: 'flex', alignItems: 'center', justifyContent: 'center' }}><FIPrev size={28} /></span>
        <span style={{
          width: 68, height: 68, borderRadius: '50%', background: CA.acc, color: CA.accInk,
          display: 'flex', alignItems: 'center', justifyContent: 'center',
        }}><FIPause size={28} /></span>
        <span style={{ color: CA.ink, width: 50, height: 50, display: 'flex', alignItems: 'center', justifyContent: 'center' }}><FINext size={28} /></span>
        <span style={{ color: CA.acc }}><FIRepeat /></span>
      </div>
      <div style={{
        borderTop: `1px solid ${CA.line}`, height: 62, display: 'flex', alignItems: 'center',
        justifyContent: 'space-around', color: CA.dim, flexShrink: 0,
      }}>
        <span style={{ color: CA.acc }}><FIHeart fill /></span>
        <FIQueue /><FIEq /><FIBt /><FISound />
      </div>
    </CAScr>
  );
}

// ─── A · Lock screen (was "night mode") ────────────────────────
function CALock({ night }) {
  const CA = caPal(night);
  return (
    <CAScr CA={CA} label={`A · Lock Screen${night ? ' · Night' : ''}`}>
      <CAStatus CA={CA} night={night} />
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center' }}>
        <div style={{ fontFamily: CA.mono, fontSize: 88, fontWeight: 300, letterSpacing: '-.02em', color: CA.ink }}>23:41</div>
        <div style={{ marginTop: 26, fontSize: 15, color: CA.ink }}>{FTRX.title}</div>
        <div style={{ marginTop: 5, fontSize: 12, color: CA.dim }}>{FTRX.artist}</div>
        <div style={{ width: 240, marginTop: 24 }}>
          <FProg pct={FTRX.pct} h={2} track={CA.line} fill={CA.dim} />
          <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 6, fontFamily: CA.mono, fontSize: 9, color: CA.dim }}>
            <span>{FTRX.cur}</span><span>{FTRX.dur}</span>
          </div>
        </div>
      </div>
      <div style={{
        height: 58, display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 8,
        fontFamily: CA.mono, fontSize: 9, letterSpacing: '.16em', color: CA.faint, flexShrink: 0,
      }}>
        <FILock size={12} /> LOCKED · SIDE KEYS ACTIVE · TAP TWICE TO WAKE
      </div>
    </CAScr>
  );
}

// ─── A · Menu ──────────────────────────────────────────────────
function CAMenu({ night }) {
  const CA = caPal(night);
  return (
    <CAScr CA={CA} label={`A · Menu${night ? ' · Night' : ''}`}>
      <CAStatus CA={CA} night={night} />
      <CAHeader CA={CA} title="Menu" right={<span style={{ fontFamily: CA.mono, fontSize: 10, color: CA.faint, letterSpacing: '.1em' }}>NW-A55</span>} />
      <div style={{ flex: 1, overflow: 'hidden' }}>
        {FMENU.map((m, i) => {
          const Ico = FICONS[m.icon];
          return (
            <div key={m.label} style={{
              display: 'flex', alignItems: 'center', gap: 16, height: 63, padding: '0 22px',
              borderTop: i === 0 ? `1px solid ${CA.line}` : 'none',
              borderBottom: `1px solid ${CA.line}`,
            }}>
              <span style={{ color: i === 0 ? CA.acc : CA.dim }}><Ico /></span>
              <span style={{ fontSize: 17, fontWeight: 600, flex: 1 }}>{m.label}</span>
              <span style={{ fontFamily: CA.mono, fontSize: 10, color: CA.faint, letterSpacing: '.04em' }}>{m.value}</span>
              <span style={{ color: CA.faint }}><FIChev /></span>
            </div>
          );
        })}
      </div>
    </CAScr>
  );
}

// ─── A · Bluetooth ─────────────────────────────────────────────
function CABluetooth({ night }) {
  const CA = caPal(night);
  return (
    <CAScr CA={CA} label={`A · Bluetooth${night ? ' · Night' : ''}`}>
      <CAStatus CA={CA} night={night} />
      <CAHeader CA={CA} title="Bluetooth" right={
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8, fontFamily: CA.mono, fontSize: 10, letterSpacing: '.12em', color: CA.acc }}>
          ON
          <span style={{ width: 34, height: 18, border: `1px solid ${CA.acc}`, position: 'relative', display: 'inline-block' }}>
            <span style={{ position: 'absolute', top: 2, right: 2, width: 12, height: 12, background: CA.acc }}></span>
          </span>
        </span>
      } />
      <div style={{ margin: '0 22px', border: `1px solid ${CA.line}`, background: CA.panel, padding: '18px 18px 16px' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <span style={{ fontFamily: CA.mono, fontSize: 9, letterSpacing: '.18em', color: CA.acc }}>CONNECTED</span>
          <span style={{ fontFamily: CA.mono, fontSize: 9, letterSpacing: '.1em', color: CA.dim }}>HP BATT 60%</span>
        </div>
        <div style={{ fontSize: 23, fontWeight: 700, marginTop: 8 }}>WH-1000XM5</div>
        <div style={{ fontFamily: CA.mono, fontSize: 10, color: CA.dim, marginTop: 4, letterSpacing: '.04em' }}>LDAC · 96 kHz · Sound quality preferred</div>
        <div style={{ display: 'flex', gap: 10, marginTop: 16 }}>
          <span style={{ flex: 1, height: 44, display: 'flex', alignItems: 'center', justifyContent: 'center', border: `1px solid ${CA.line}`, fontSize: 13, fontWeight: 600, color: CA.dim }}>Disconnect</span>
          <span style={{ flex: 1, height: 44, display: 'flex', alignItems: 'center', justifyContent: 'center', border: `1px solid ${CA.line}`, fontSize: 13, fontWeight: 600, color: CA.dim }}>Quality · LDAC</span>
        </div>
      </div>
      <div style={{ padding: '22px 22px 8px', fontFamily: CA.mono, fontSize: 9, letterSpacing: '.18em', color: CA.faint }}>PAIRED DEVICES</div>
      <div>
        {FPAIRED.map((d) => (
          <div key={d.name} style={{ display: 'flex', alignItems: 'center', gap: 14, height: 58, padding: '0 22px', borderBottom: `1px solid ${CA.line}` }}>
            <span style={{ color: CA.dim }}><FIBt size={16} /></span>
            <span style={{ flex: 1 }}>
              <span style={{ fontSize: 15, fontWeight: 600, display: 'block' }}>{d.name}</span>
              <span style={{ fontFamily: CA.mono, fontSize: 9, color: CA.faint, letterSpacing: '.06em' }}>{d.kind}</span>
            </span>
            <span style={{ fontFamily: CA.mono, fontSize: 10, letterSpacing: '.1em', color: CA.acc, border: `1px solid ${CA.acc}`, padding: '6px 12px' }}>CONNECT</span>
          </div>
        ))}
      </div>
      <div style={{ marginTop: 'auto', padding: '0 22px 18px' }}>
        <div style={{ height: 52, background: CA.acc, color: CA.accInk, display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 10, fontSize: 15, fontWeight: 700 }}>
          <FIBt size={17} /> Pair new device
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: 12, color: CA.faint, fontFamily: CA.mono, fontSize: 9, letterSpacing: '.08em' }}>
          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 7 }}><FINfc size={14} /> NFC · TOUCH DEVICE TO REAR PANEL</span>
          <span style={{ color: CA.dim }}>RECEIVER MODE ›</span>
        </div>
      </div>
    </CAScr>
  );
}

// ─── A · Equalizer ─────────────────────────────────────────────
function CAEq({ night }) {
  const CA = caPal(night);
  const H = 330, range = 10;
  return (
    <CAScr CA={CA} label={`A · Equalizer${night ? ' · Night' : ''}`}>
      <CAStatus CA={CA} night={night} />
      <CAHeader CA={CA} title="Equalizer" right={<span style={{ fontFamily: CA.mono, fontSize: 10, letterSpacing: '.12em', color: CA.acc, border: `1px solid ${CA.acc}`, padding: '4px 9px' }}>CUSTOM A1</span>} />
      <div style={{ display: 'flex', gap: 8, padding: '2px 22px 20px', fontFamily: CA.mono, fontSize: 10, letterSpacing: '.08em' }}>
        {['FLAT', 'ROCK', 'JAZZ', 'A1', 'A2'].map((p) => (
          <span key={p} style={{
            padding: '7px 13px', border: `1px solid ${p === 'A1' ? CA.acc : CA.line}`,
            color: p === 'A1' ? CA.accInk : CA.dim, background: p === 'A1' ? CA.acc : 'transparent', fontWeight: p === 'A1' ? 700 : 400,
          }}>{p}</span>
        ))}
      </div>
      <div style={{ position: 'relative', margin: '6px 26px 0', height: H + 46 }}>
        <div style={{ position: 'absolute', left: 0, right: 0, top: H / 2 + 10, borderTop: `1px dashed ${CA.line}` }}></div>
        <div style={{ display: 'flex', height: '100%' }}>
          {FBANDS.map((b) => {
            const knobY = H / 2 - (b.db / range) * (H / 2 - 14);
            return (
              <div key={b.hz} style={{ flex: 1, position: 'relative' }}>
                <div style={{ position: 'absolute', top: -6, left: 0, right: 0, textAlign: 'center', fontFamily: CA.mono, fontSize: 9, color: b.db !== 0 ? CA.acc : CA.faint }}>
                  {b.db > 0 ? `+${b.db}` : b.db}
                </div>
                <div style={{ position: 'absolute', top: 10, bottom: 36, left: '50%', width: 2, marginLeft: -1, background: CA.line }}></div>
                <div style={{
                  position: 'absolute', left: '50%', width: 2, marginLeft: -1, background: CA.acc,
                  top: 10 + Math.min(knobY, H / 2), height: Math.abs((b.db / range) * (H / 2 - 14)),
                }}></div>
                <div style={{
                  position: 'absolute', top: 10 + knobY - 8, left: '50%', marginLeft: -8,
                  width: 16, height: 16, borderRadius: '50%', background: CA.acc, border: `3px solid ${CA.bg}`,
                }}></div>
                <div style={{ position: 'absolute', bottom: 12, left: 0, right: 0, textAlign: 'center', fontFamily: CA.mono, fontSize: 9, color: CA.dim }}>{b.hz}</div>
              </div>
            );
          })}
        </div>
      </div>
      <div style={{ marginTop: 'auto', borderTop: `1px solid ${CA.line}`, height: 60, display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '0 22px', flexShrink: 0 }}>
        <span style={{ fontSize: 14, fontWeight: 600, color: CA.dim }}>Reset</span>
        <span style={{ fontSize: 14, fontWeight: 700, color: CA.acc }}>Save Sound Preset</span>
      </div>
    </CAScr>
  );
}

// ─── A · Shelf sheet (pin places + undo/redo) ──────────────────
function CAShelf({ night }) {
  const CA = caPal(night);
  const slots = [
    { n: 1, title: 'Album · Last Smoke Before…', sub: 'Track 4 · saved 12 min ago' },
    { n: 2, title: 'Library · Artists · B', sub: 'Saved 1 hr ago' },
    { n: 3, title: null },
  ];
  const cap = { fontFamily: CA.mono, fontSize: 9, letterSpacing: '.18em', color: CA.acc, marginBottom: 8 };
  return (
    <CAScr CA={CA} label={`A · Shelf${night ? ' · Night' : ''}`}>
      <CAStatus CA={CA} night={night} />
      <div style={{ flex: 1, position: 'relative', overflow: 'hidden' }}>
        <div className="art" data-art={FTRX.art} style={{ width: 480, height: 480, opacity: 0.16 }}></div>
        <div style={{
          position: 'absolute', left: 0, right: 0, bottom: 0,
          background: CA.panel, borderTop: `1px solid ${CA.acc}`, padding: '16px 22px 20px',
        }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
            <span style={{ fontSize: 20, fontWeight: 700, display: 'flex', alignItems: 'center', gap: 9 }}><FIBookmark size={16} /> Shelf</span>
            <span style={{ fontFamily: CA.mono, fontSize: 10, color: CA.faint }}>CLOSE ×</span>
          </div>

          <div style={cap}>HISTORY</div>
          <div style={{ display: 'flex', gap: 10, marginBottom: 18 }}>
            <span style={{ flex: 1, border: `1px solid ${CA.line}`, padding: '10px 13px' }}>
              <span style={{ display: 'block', fontSize: 13, fontWeight: 600 }}>‹ Undo</span>
              <span style={{ display: 'block', fontFamily: CA.mono, fontSize: 9, color: CA.dim, marginTop: 4 }}>Library · Albums</span>
            </span>
            <span style={{ flex: 1, border: `1px solid ${CA.line}`, padding: '10px 13px', color: CA.faint }}>
              <span style={{ display: 'block', fontSize: 13, fontWeight: 600 }}>Redo ›</span>
              <span style={{ display: 'block', fontFamily: CA.mono, fontSize: 9, marginTop: 4 }}>—</span>
            </span>
          </div>

          <div style={cap}>THIS PLACE</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 12, border: `1px solid ${CA.line}`, padding: '11px 13px', marginBottom: 18 }}>
            <span style={{ flex: 1 }}>
              <span style={{ display: 'block', fontSize: 14, fontWeight: 600 }}>Now Playing · Atlas Hands</span>
              <span style={{ display: 'block', fontFamily: CA.mono, fontSize: 9, color: CA.dim, marginTop: 4 }}>1:47 / 4:32</span>
            </span>
            <span style={{ background: CA.acc, color: CA.accInk, fontSize: 12, fontWeight: 700, padding: '9px 14px' }}>Pin</span>
          </div>

          <div style={cap}>PINNED · 2/3</div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 9 }}>
            {slots.map((s) => (
              <div key={s.n} style={{
                display: 'flex', alignItems: 'center', gap: 13, padding: '10px 13px',
                border: `1px ${s.title ? 'solid' : 'dashed'} ${CA.line}`,
              }}>
                <span style={{ fontFamily: CA.mono, fontSize: 11, color: s.title ? CA.acc : CA.faint }}>{s.n}</span>
                {s.title ? (
                  <span style={{ flex: 1 }}>
                    <span style={{ display: 'block', fontSize: 13, fontWeight: 600 }}>{s.title}</span>
                    <span style={{ display: 'block', fontFamily: CA.mono, fontSize: 9, color: CA.dim, marginTop: 3 }}>{s.sub}</span>
                  </span>
                ) : (
                  <span style={{ flex: 1, fontSize: 12, color: CA.faint }}>Empty slot — pin here</span>
                )}
                {s.title && <span style={{ fontFamily: CA.mono, fontSize: 10, color: CA.acc }}>GO ›</span>}
              </div>
            ))}
          </div>
        </div>
      </div>
    </CAScr>
  );
}

Object.assign(window, { caPal, CAScr, CAStatus, CAHeader, CANowPlaying, CALock, CAMenu, CABluetooth, CAEq, CAShelf });
