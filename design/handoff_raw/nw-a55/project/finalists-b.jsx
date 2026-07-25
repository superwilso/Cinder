// ────────────────────────────────────────────────────────────────
// finalists-b.jsx — Candidate B · "NOCTURNE"
// Dark editorial: Instrument Serif display type, lavender accent,
// generous whitespace, hairline rules. `night` prop = darker
// palette, dimmed art, muted accent. Lock screen separate.
// ────────────────────────────────────────────────────────────────

const CB_FONTS = { serif: "'Instrument Serif', serif", mono: "'JetBrains Mono', monospace", sans: "'Hanken Grotesk', sans-serif" };

const CB_DAY = {
  bg: '#08080b', line: '#1c1c22', panel: '#0e0e13',
  ink: '#ece4d4', dim: '#97907f', faint: '#5c574c',
  acc: '#c4b6ff', accInk: '#13101f', artDim: 1, ...CB_FONTS,
};
const CB_NIGHT = {
  bg: '#000000', line: '#141414', panel: '#0a0a0c',
  ink: '#857c69', dim: '#544e40', faint: '#36322a',
  acc: '#6f6594', accInk: '#000000', artDim: 0.25, ...CB_FONTS,
};
const cbPal = (night) => (night ? CB_NIGHT : CB_DAY);

function CBScr({ CB, children, label }) {
  return (
    <div data-screen-label={label} style={{
      width: 480, height: 800, background: CB.bg, color: CB.ink, fontFamily: CB.sans,
      display: 'flex', flexDirection: 'column', overflow: 'hidden', userSelect: 'none',
    }}>{children}</div>
  );
}

function CBStatus({ CB, night }) {
  return (
    <div style={{
      height: 34, display: 'flex', alignItems: 'center', justifyContent: 'space-between',
      padding: '0 24px', fontFamily: CB.mono, fontSize: 10, letterSpacing: '.1em',
      color: CB.dim, flexShrink: 0,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
        <span>14:32</span>
        <span style={{ color: CB.acc, letterSpacing: '.16em', fontSize: 9 }}>FLAC 24/96</span>
        {night && <span style={{ color: CB.faint, letterSpacing: '.2em', fontSize: 9 }}>NIGHT</span>}
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
        <FIBookmark size={13} />
        <FIBt size={13} />
        <FBatt pct={78} />
      </div>
    </div>
  );
}

function CBHeader({ CB, title, right }) {
  return (
    <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', padding: '18px 24px 14px', flexShrink: 0 }}>
      <span style={{ display: 'flex', alignItems: 'center', gap: 14 }}>
        <span style={{ color: CB.dim }}><FIBack /></span>
        <span style={{ fontFamily: CB.serif, fontSize: 34, letterSpacing: '-.01em' }}>{title}</span>
      </span>
      {right}
    </div>
  );
}

// ─── B · Now Playing ───────────────────────────────────────────
function CBNowPlaying({ night }) {
  const CB = cbPal(night);
  return (
    <CBScr CB={CB} label={`B · Now Playing${night ? ' · Night' : ''}`}>
      <CBStatus CB={CB} night={night} />
      <div style={{ padding: '10px 32px 0' }}>
        <div className="art" data-art={FTRX.art} style={{ width: 416, height: 416, border: `1px solid ${CB.line}`, opacity: CB.artDim }}></div>
      </div>
      <div style={{ textAlign: 'center', padding: '24px 32px 0' }}>
        <div style={{ fontFamily: CB.serif, fontSize: 33, letterSpacing: '-.01em', lineHeight: 1.05 }}>{FTRX.title}</div>
        <div style={{ fontFamily: CB.serif, fontStyle: 'italic', fontSize: 17, color: CB.dim, marginTop: 7 }}>{FTRX.artist}</div>
        <div style={{ fontFamily: CB.mono, fontSize: 9, letterSpacing: '.22em', color: CB.acc, marginTop: 12 }}>FLAC · 24BIT / 96.0 KHZ</div>
      </div>
      <div style={{ padding: '22px 32px 0' }}>
        <FBars n={52} seed={4} h={22} gap={2} color={CB.acc} dimColor={CB.line} />
        <FProg pct={FTRX.pct} h={2} track={CB.line} fill={CB.acc} style={{ marginTop: 10 }} />
        <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 8, fontFamily: CB.mono, fontSize: 10, color: CB.dim }}>
          <span>{FTRX.cur}</span><span style={{ color: CB.faint }}>{FTRX.dur}</span>
        </div>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 38, flex: 1 }}>
        <span style={{ color: CB.faint }}><FIShuffle size={17} /></span>
        <span style={{ color: CB.ink }}><FIPrev size={26} /></span>
        <span style={{
          width: 72, height: 72, borderRadius: '50%', border: `1px solid ${CB.ink}`,
          display: 'flex', alignItems: 'center', justifyContent: 'center', color: CB.ink,
        }}><FIPause size={26} /></span>
        <span style={{ color: CB.ink }}><FINext size={26} /></span>
        <span style={{ color: CB.acc }}><FIRepeat size={17} /></span>
      </div>
      <div style={{
        borderTop: `1px solid ${CB.line}`, height: 58, display: 'flex', alignItems: 'center',
        justifyContent: 'space-around', color: CB.dim, flexShrink: 0,
      }}>
        <span style={{ color: CB.acc }}><FIHeart fill size={17} /></span>
        <FIQueue size={17} /><FIEq size={17} /><FIBt size={16} /><FISound size={17} />
      </div>
    </CBScr>
  );
}

// ─── B · Lock screen ───────────────────────────────────────────
function CBLock({ night }) {
  const CB = cbPal(night);
  return (
    <CBScr CB={CB} label={`B · Lock Screen${night ? ' · Night' : ''}`}>
      <CBStatus CB={CB} night={night} />
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center' }}>
        <div style={{ fontFamily: CB.serif, fontSize: 96, color: CB.ink, letterSpacing: '-.02em' }}>23:41</div>
        <div style={{ fontFamily: CB.serif, fontStyle: 'italic', fontSize: 16, color: CB.ink, marginTop: 18 }}>{FTRX.title}</div>
        <div style={{ fontSize: 11, color: CB.dim, marginTop: 6 }}>{FTRX.artist}</div>
        <div style={{ width: 220, marginTop: 26 }}>
          <FProg pct={FTRX.pct} h={1.5} track={CB.line} fill={CB.dim} />
        </div>
      </div>
      <div style={{ height: 56, display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 8, fontFamily: CB.mono, fontSize: 9, letterSpacing: '.2em', color: CB.faint, flexShrink: 0 }}>
        <FILock size={11} /> LOCKED · KEYS ACTIVE · TAP TWICE TO WAKE
      </div>
    </CBScr>
  );
}

// ─── B · Menu ──────────────────────────────────────────────────
function CBMenu({ night }) {
  const CB = cbPal(night);
  return (
    <CBScr CB={CB} label={`B · Menu${night ? ' · Night' : ''}`}>
      <CBStatus CB={CB} night={night} />
      <CBHeader CB={CB} title="Menu" right={<span style={{ fontFamily: CB.mono, fontSize: 9, letterSpacing: '.18em', color: CB.faint }}>NW-A55</span>} />
      <div style={{ flex: 1, padding: '0 24px' }}>
        {FMENU.map((m, i) => (
          <div key={m.label} style={{
            display: 'flex', alignItems: 'center', gap: 16, height: 63,
            borderBottom: `1px solid ${CB.line}`,
          }}>
            <span style={{ fontFamily: CB.serif, fontStyle: 'italic', fontSize: 15, color: i === 0 ? CB.acc : CB.faint, width: 22 }}>{String(i + 1).padStart(2, '0')}</span>
            <span style={{ fontSize: 17, fontWeight: 500, flex: 1, letterSpacing: '.01em' }}>{m.label}</span>
            <span style={{ fontFamily: CB.mono, fontSize: 9, color: CB.faint, letterSpacing: '.06em' }}>{m.value}</span>
          </div>
        ))}
      </div>
    </CBScr>
  );
}

// ─── B · Bluetooth ─────────────────────────────────────────────
function CBBluetooth({ night }) {
  const CB = cbPal(night);
  return (
    <CBScr CB={CB} label={`B · Bluetooth${night ? ' · Night' : ''}`}>
      <CBStatus CB={CB} night={night} />
      <CBHeader CB={CB} title="Bluetooth" right={
        <span style={{ fontFamily: CB.mono, fontSize: 9, letterSpacing: '.16em', color: CB.acc, display: 'inline-flex', alignItems: 'center', gap: 8 }}>
          ON
          <span style={{ width: 32, height: 17, borderRadius: 999, border: `1px solid ${CB.acc}`, position: 'relative', display: 'inline-block' }}>
            <span style={{ position: 'absolute', top: 2, right: 2, width: 11, height: 11, borderRadius: '50%', background: CB.acc }}></span>
          </span>
        </span>
      } />
      <div style={{ margin: '4px 24px 0', borderTop: `1px solid ${CB.acc}`, paddingTop: 16 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between' }}>
          <span style={{ fontFamily: CB.mono, fontSize: 9, letterSpacing: '.22em', color: CB.acc }}>CONNECTED</span>
          <span style={{ fontFamily: CB.mono, fontSize: 9, letterSpacing: '.1em', color: CB.dim }}>HP BATT 60%</span>
        </div>
        <div style={{ fontFamily: CB.serif, fontSize: 30, marginTop: 10 }}>WH-1000XM5</div>
        <div style={{ fontFamily: CB.mono, fontSize: 10, color: CB.dim, marginTop: 5, letterSpacing: '.05em' }}>LDAC · 96 kHz · Sound quality preferred</div>
        <div style={{ display: 'flex', gap: 26, marginTop: 14, fontSize: 13, fontWeight: 600 }}>
          <span style={{ color: CB.dim, borderBottom: `1px solid ${CB.line}`, paddingBottom: 3 }}>Disconnect</span>
          <span style={{ color: CB.dim, borderBottom: `1px solid ${CB.line}`, paddingBottom: 3 }}>Quality · LDAC</span>
        </div>
      </div>
      <div style={{ padding: '30px 24px 6px', fontFamily: CB.mono, fontSize: 9, letterSpacing: '.22em', color: CB.faint }}>PAIRED DEVICES</div>
      <div style={{ padding: '0 24px' }}>
        {FPAIRED.map((d) => (
          <div key={d.name} style={{ display: 'flex', alignItems: 'center', height: 58, borderBottom: `1px solid ${CB.line}`, gap: 14 }}>
            <span style={{ flex: 1 }}>
              <span style={{ fontSize: 16, fontWeight: 500, display: 'block' }}>{d.name}</span>
              <span style={{ fontFamily: CB.mono, fontSize: 9, color: CB.faint, letterSpacing: '.06em' }}>{d.kind}</span>
            </span>
            <span style={{ fontFamily: CB.serif, fontStyle: 'italic', fontSize: 15, color: CB.acc }}>connect</span>
          </div>
        ))}
      </div>
      <div style={{ marginTop: 'auto', padding: '0 24px 20px' }}>
        <div style={{ height: 52, borderRadius: 999, background: CB.acc, color: CB.accInk, display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 10, fontSize: 15, fontWeight: 700 }}>
          <FIBt size={16} /> Pair new device
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 13, color: CB.faint, fontFamily: CB.mono, fontSize: 9, letterSpacing: '.1em' }}>
          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 7 }}><FINfc size={13} /> NFC · TOUCH TO REAR</span>
          <span style={{ color: CB.dim }}>RECEIVER MODE ›</span>
        </div>
      </div>
    </CBScr>
  );
}

// ─── B · Equalizer ─────────────────────────────────────────────
function CBEq({ night }) {
  const CB = cbPal(night);
  const H = 330, range = 10;
  return (
    <CBScr CB={CB} label={`B · Equalizer${night ? ' · Night' : ''}`}>
      <CBStatus CB={CB} night={night} />
      <CBHeader CB={CB} title="Equalizer" right={<span style={{ fontFamily: CB.serif, fontStyle: 'italic', fontSize: 17, color: CB.acc }}>Custom A1</span>} />
      <div style={{ display: 'flex', gap: 22, padding: '0 24px 22px', fontFamily: CB.mono, fontSize: 10, letterSpacing: '.14em' }}>
        {['FLAT', 'ROCK', 'JAZZ', 'A1', 'A2'].map((p) => (
          <span key={p} style={{
            color: p === 'A1' ? CB.acc : CB.faint, paddingBottom: 4,
            borderBottom: p === 'A1' ? `1px solid ${CB.acc}` : '1px solid transparent',
          }}>{p}</span>
        ))}
      </div>
      <div style={{ position: 'relative', margin: '4px 28px 0', height: H + 46 }}>
        <div style={{ position: 'absolute', left: 0, right: 0, top: H / 2 + 10, borderTop: `1px solid ${CB.line}` }}></div>
        <div style={{ display: 'flex', height: '100%' }}>
          {FBANDS.map((b) => {
            const knobY = H / 2 - (b.db / range) * (H / 2 - 14);
            return (
              <div key={b.hz} style={{ flex: 1, position: 'relative' }}>
                <div style={{ position: 'absolute', top: -6, left: 0, right: 0, textAlign: 'center', fontFamily: CB.mono, fontSize: 9, color: b.db !== 0 ? CB.acc : CB.faint }}>
                  {b.db > 0 ? `+${b.db}` : b.db}
                </div>
                <div style={{ position: 'absolute', top: 10, bottom: 36, left: '50%', width: 1, background: CB.line }}></div>
                <div style={{
                  position: 'absolute', top: 10 + knobY - 6, left: '50%', marginLeft: -6,
                  width: 12, height: 12, borderRadius: '50%', background: CB.acc,
                }}></div>
                <div style={{ position: 'absolute', bottom: 12, left: 0, right: 0, textAlign: 'center', fontFamily: CB.mono, fontSize: 9, color: CB.dim }}>{b.hz}</div>
              </div>
            );
          })}
        </div>
      </div>
      <div style={{ marginTop: 'auto', borderTop: `1px solid ${CB.line}`, height: 58, display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '0 24px', flexShrink: 0 }}>
        <span style={{ fontSize: 13, fontWeight: 600, color: CB.dim }}>Reset</span>
        <span style={{ fontFamily: CB.serif, fontStyle: 'italic', fontSize: 17, color: CB.acc }}>Save preset</span>
      </div>
    </CBScr>
  );
}

// ─── B · Shelf sheet (pin places + undo/redo) ──────────────────
function CBShelf({ night }) {
  const CB = cbPal(night);
  const slots = [
    { n: '01', title: 'Album · Last Smoke Before…', sub: 'Track 4 · saved 12 min ago' },
    { n: '02', title: 'Library · Artists · B', sub: 'Saved 1 hr ago' },
    { n: '03', title: null },
  ];
  const cap = { fontFamily: CB.mono, fontSize: 9, letterSpacing: '.22em', color: CB.acc, marginBottom: 9 };
  return (
    <CBScr CB={CB} label={`B · Shelf${night ? ' · Night' : ''}`}>
      <CBStatus CB={CB} night={night} />
      <div style={{ flex: 1, position: 'relative', overflow: 'hidden' }}>
        <div style={{ padding: '10px 32px 0', opacity: 0.16 }}>
          <div className="art" data-art={FTRX.art} style={{ width: 416, height: 416, border: `1px solid ${CB.line}` }}></div>
        </div>
        <div style={{
          position: 'absolute', left: 0, right: 0, bottom: 0,
          background: CB.panel, borderTop: `1px solid ${CB.acc}`, padding: '18px 24px 22px',
        }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 16 }}>
            <span style={{ fontFamily: CB.serif, fontSize: 27 }}>Shelf</span>
            <span style={{ fontFamily: CB.mono, fontSize: 9, letterSpacing: '.14em', color: CB.faint }}>CLOSE ×</span>
          </div>

          <div style={cap}>HISTORY</div>
          <div style={{ display: 'flex', gap: 10, marginBottom: 20 }}>
            <span style={{ flex: 1, borderBottom: `1px solid ${CB.line}`, paddingBottom: 9 }}>
              <span style={{ display: 'block', fontSize: 14, fontWeight: 600 }}>‹ Undo</span>
              <span style={{ display: 'block', fontFamily: CB.mono, fontSize: 9, color: CB.dim, marginTop: 4 }}>Library · Albums</span>
            </span>
            <span style={{ flex: 1, borderBottom: `1px solid ${CB.line}`, paddingBottom: 9, color: CB.faint }}>
              <span style={{ display: 'block', fontSize: 14, fontWeight: 600 }}>Redo ›</span>
              <span style={{ display: 'block', fontFamily: CB.mono, fontSize: 9, marginTop: 4 }}>—</span>
            </span>
          </div>

          <div style={cap}>THIS PLACE</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 20 }}>
            <span style={{ flex: 1 }}>
              <span style={{ display: 'block', fontSize: 15, fontWeight: 600 }}>Now Playing · Atlas Hands</span>
              <span style={{ display: 'block', fontFamily: CB.mono, fontSize: 9, color: CB.dim, marginTop: 4 }}>1:47 / 4:32</span>
            </span>
            <span style={{ background: CB.acc, color: CB.accInk, fontSize: 13, fontWeight: 700, padding: '9px 18px', borderRadius: 999 }}>Pin</span>
          </div>

          <div style={cap}>PINNED · 2 OF 3</div>
          <div style={{ display: 'flex', flexDirection: 'column' }}>
            {slots.map((s) => (
              <div key={s.n} style={{ display: 'flex', alignItems: 'center', gap: 14, padding: '11px 0', borderBottom: `1px solid ${CB.line}` }}>
                <span style={{ fontFamily: CB.serif, fontStyle: 'italic', fontSize: 14, color: s.title ? CB.acc : CB.faint }}>{s.n}</span>
                {s.title ? (
                  <span style={{ flex: 1 }}>
                    <span style={{ display: 'block', fontSize: 14, fontWeight: 500 }}>{s.title}</span>
                    <span style={{ display: 'block', fontFamily: CB.mono, fontSize: 9, color: CB.dim, marginTop: 3 }}>{s.sub}</span>
                  </span>
                ) : (
                  <span style={{ flex: 1, fontSize: 13, color: CB.faint }}>Empty slot — pin here</span>
                )}
                {s.title && <span style={{ fontFamily: CB.serif, fontStyle: 'italic', fontSize: 14, color: CB.acc }}>go ›</span>}
              </div>
            ))}
          </div>
        </div>
      </div>
    </CBScr>
  );
}

Object.assign(window, { CBNowPlaying, CBLock, CBMenu, CBBluetooth, CBEq, CBShelf });
