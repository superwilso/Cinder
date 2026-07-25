// ────────────────────────────────────────────────────────────────
// Direction 3 — TERMINAL (portrait 480×800)
// Retro/brutalist: pixel mono, hard rules, ASCII meters,
// amber-phosphor accent. Re-flowed for portrait — vertical-first.
// ────────────────────────────────────────────────────────────────

const TmColors = {
  bg:       '#0d0d0d',
  bgLight:  '#e8e4d5',
  phosphor: '#f0a420',
  text:     '#e8e6dc',
  textL:    '#1a1a16',
  rule:     'rgba(232,230,220,.25)',
  ruleL:    'rgba(26,26,22,.25)',
};

// Reusable header
function TmHeader({ title = '', sub = '', light = false }) {
  return (
    <div className="status" style={{ borderBottom: `1px solid ${light ? TmColors.ruleL : TmColors.rule}`, fontFamily: 'JetBrains Mono, monospace' }}>
      <div className="l" style={{ gap: 12 }}>
        <span style={{ color: light ? '#a85a08' : TmColors.phosphor }}>● NW-A55</span>
        <span style={{ opacity: .6 }}>{title}</span>
      </div>
      <div className="r" style={{ gap: 8 }}>
        <span style={{ opacity: .6 }}>{sub}</span>
        <span>14:32</span>
        <span>78%</span>
        <span className="batt"><i style={{ '--p': '78%' }}/></span>
      </div>
    </div>
  );
}

// Horizontal slider row (used by EQ + meters)
function TmRow({ label, value, max = 12, min = -12, color = TmColors.phosphor, cells = 30, valueDisplay }) {
  const pct = (value - min) / (max - min);
  const filled = Math.round(pct * cells);
  return (
    <div style={{ display: 'grid', gridTemplateColumns: '46px 1fr 46px', gap: 10, alignItems: 'center', padding: '4px 0' }}>
      <span style={{ fontSize: 11, opacity: .8, textAlign: 'right' }}>{label}</span>
      <span style={{ display: 'flex', gap: 1 }}>
        {Array.from({ length: cells }).map((_, i) => (
          <span key={i} style={{
            flex: 1,
            height: 12,
            background: i < filled ? color : 'rgba(232,230,220,.16)',
          }} />
        ))}
      </span>
      <span style={{ fontSize: 11, color, fontFamily: 'JetBrains Mono, monospace', textAlign: 'right' }}>
        {valueDisplay}
      </span>
    </div>
  );
}

// ─── Now Playing · Hero ─────────────────────────────────────────
function Tm_NowPlayingHero({ track = TRACKS[0], theme = 'dark' }) {
  const t = track;
  const light = theme === 'light';
  const spec = [78, 62, 91, 45, 70, 88, 55, 30, 65, 72, 40, 25, 55, 38, 18, 28, 45, 30, 22, 18];

  return (
    <div className={`scr terminal ${light ? 'light' : ''}`}>
      <TmHeader title="// NOW_PLAYING" sub="FLAC·24/96" light={light} />

      {/* Big art with frame */}
      <div style={{ padding: '20px 32px 0' }}>
        <div style={{ border: `1px solid ${light ? TmColors.textL : TmColors.text}`, padding: 5 }}>
          <Art kind={t.art} size={406} label={false} style={{ display: 'block' }} />
        </div>
      </div>

      {/* Bottom data band */}
      <div style={{
        position: 'absolute', left: 0, right: 0, bottom: 0,
        padding: '0 24px 22px',
      }}>
        {/* readout row */}
        <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, padding: '8px 0' }}>
          <span>[A·01]</span>
          <span className="phosphor">[ HI-RES AUDIO ]</span>
          <span>[03/12]</span>
        </div>

        {/* Spectrum */}
        <div style={{ display: 'flex', gap: 2, alignItems: 'end', height: 36, marginTop: 4 }}>
          {spec.map((v, i) => (
            <div key={i} style={{
              flex: 1, height: `${v}%`,
              background: light ? TmColors.textL : TmColors.text,
              opacity: i < 8 ? 1 : .5,
            }} />
          ))}
        </div>

        {/* Track */}
        <div style={{ marginTop: 12, fontSize: 10, opacity: .6 }}>&gt; {t.album.toUpperCase()}</div>
        <div style={{ fontSize: 22, textTransform: 'uppercase', marginTop: 2, lineHeight: 1.05 }}>
          {t.title}
        </div>
        <div style={{ fontSize: 11, opacity: .8, marginTop: 4 }}>BY {t.artist.toUpperCase()}</div>

        {/* Progress */}
        <div style={{ marginTop: 10, fontSize: 11 }}>
          <span>{t.cur} </span>
          <span className="phosphor">[</span>
          <span className="phosphor">██████████</span>
          <span style={{ opacity: .4 }}>░░░░░░░░░░░░░░░░░</span>
          <span className="phosphor">]</span>
          <span> -2:45</span>
        </div>

        {/* Controls */}
        <div style={{ marginTop: 14, display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 6 }}>
          <span style={{ fontSize: 11 }}>[SHUF]</span>
          <span style={{ fontSize: 11 }}>[‹‹]</span>
          <span style={{
            padding: '8px 30px',
            border: `1px solid ${light ? TmColors.textL : TmColors.phosphor}`,
            color: light ? TmColors.textL : TmColors.phosphor,
            fontSize: 13, letterSpacing: '.14em',
          }}>► PLAY</span>
          <span style={{ fontSize: 11 }}>[››]</span>
          <span style={{ fontSize: 11 }}>[REPT]</span>
        </div>
      </div>
    </div>
  );
}

// ─── Now Playing · Dense (data-heavy, portrait-native) ──────────
function Tm_NowPlayingDense({ track = TRACKS[3] }) {
  const t = track;
  const spec = [88, 72, 55, 45, 60, 70, 50, 35, 50, 65, 40, 28, 45, 30, 20, 18, 40, 25, 18, 12];
  return (
    <div className="scr terminal">
      <TmHeader title="// NP.DENSE" sub="DSEE:ULT · EQ:A1" />

      {/* Header band */}
      <div style={{ padding: '14px 24px 0' }}>
        <div style={{ display: 'grid', gridTemplateColumns: '110px 1fr', gap: 14 }}>
          <Art kind={t.art} size={110} label={false} />
          <div>
            <div style={{ fontSize: 9, opacity: .55 }}>&gt; TRACK 04 / 12</div>
            <div style={{ fontSize: 20, textTransform: 'uppercase', marginTop: 4, lineHeight: 1.05 }}>{t.title}</div>
            <div style={{ fontSize: 11, opacity: .75, marginTop: 4 }}>BY {t.artist.toUpperCase()}</div>
            <div style={{ fontSize: 10, opacity: .5, marginTop: 2 }}>{t.album.toUpperCase()}</div>
          </div>
        </div>
      </div>

      {/* Spectrum + dB readout */}
      <div style={{ padding: '14px 24px 0' }}>
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em', marginBottom: 4 }}>[ SPECTRUM · L+R ]</div>
        <div style={{ display: 'flex', gap: 2, alignItems: 'end', height: 60, borderBottom: `1px solid ${TmColors.rule}`, paddingBottom: 4 }}>
          {spec.map((v, i) => (
            <div key={i} style={{
              flex: 1, height: `${v}%`,
              background: i < 8 ? TmColors.phosphor : TmColors.text,
              opacity: i < 8 ? 1 : .6,
            }} />
          ))}
        </div>
        {/* L/R bars */}
        <div style={{ marginTop: 8 }}>
          <TmRow label="L" value={72} max={100} min={0} cells={32} valueDirection valueDisplay="-5.6 dB" />
          <TmRow label="R" value={78} max={100} min={0} cells={32} valueDisplay="-4.4 dB" />
        </div>
      </div>

      {/* Tech grid */}
      <div style={{ padding: '14px 24px 0' }}>
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em', marginBottom: 6 }}>[ SIGNAL CHAIN ]</div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 0 }}>
          {[
            ['FMT',     'FLAC'],
            ['RATE',    '96.0K'],
            ['BITS',    '24'],
            ['BR',      '2304K'],
            ['DSEE',    'ULT'],
            ['DC.LIN',  'A·LO'],
            ['EQ',      'A1'],
            ['BAL',     '0.0'],
            ['OUT',     '3.5'],
            ['VOL',     '24/30'],
            ['NORM',    'OFF'],
            ['GAIN',    'STD'],
          ].map(([k, v]) => (
            <div key={k} style={{ border: `1px solid ${TmColors.rule}`, padding: '6px 8px', marginRight: -1, marginBottom: -1 }}>
              <div style={{ fontSize: 8, opacity: .5, letterSpacing: '.1em' }}>{k}</div>
              <div className="phosphor" style={{ fontSize: 11 }}>{v}</div>
            </div>
          ))}
        </div>
      </div>

      {/* Progress + controls */}
      <div style={{ padding: '16px 24px 0' }}>
        <div style={{ fontSize: 11 }}>
          <span>{t.cur} </span>
          <span className="phosphor">[█████████░░░░░░░░░░░░░░░░░░]</span>
          <span> {t.dur}</span>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginTop: 12, fontSize: 11, gap: 4 }}>
          <span>[H]</span>
          <span>[‹‹]</span>
          <span className="phosphor" style={{ border: `1px solid ${TmColors.phosphor}`, padding: '6px 22px' }}>► PLAY</span>
          <span>[››]</span>
          <span>[♡]</span>
          <span>[≡]</span>
          <span>[⋯]</span>
        </div>
      </div>
    </div>
  );
}

// ─── Library ────────────────────────────────────────────────────
function Tm_Library() {
  return (
    <div className="scr terminal">
      <TmHeader title="// LIBRARY" sub="184 TRK · 12 ALB" />
      <div style={{ padding: '14px 24px 0', fontSize: 11 }}>
        <div style={{ display: 'flex', gap: 0, marginBottom: 12 }}>
          {['ALBUMS', 'ARTISTS', 'SONGS', 'FOLDERS'].map((t, i) => (
            <div key={t} style={{
              padding: '6px 10px',
              background: i === 0 ? TmColors.phosphor : 'transparent',
              color: i === 0 ? '#0d0d0d' : 'inherit',
              border: `1px solid ${i === 0 ? TmColors.phosphor : TmColors.rule}`,
              marginRight: -1,
              fontSize: 10,
            }}>{t}</div>
          ))}
        </div>

        <div style={{ display: 'grid', gridTemplateColumns: '24px 36px 1fr 50px 40px', gap: 10, padding: '5px 0', borderTop: `1px solid ${TmColors.rule}`, borderBottom: `1px solid ${TmColors.rule}`, fontSize: 9, opacity: .55, letterSpacing: '.1em' }}>
          <span>#</span><span></span><span>TITLE / ARTIST</span><span>YEAR</span><span>FMT</span>
        </div>

        {ALBUMS.slice(0, 10).map((a, i) => (
          <div key={i} style={{
            display: 'grid', gridTemplateColumns: '24px 36px 1fr 50px 40px',
            gap: 10, padding: '7px 0',
            borderBottom: `1px solid ${TmColors.rule}`,
            background: i === 0 ? 'rgba(240,164,32,.08)' : 'transparent',
            fontSize: 11, alignItems: 'center',
          }}>
            <span className="phosphor" style={{ fontSize: 10 }}>
              {i === 0 ? '►' : String(i + 1).padStart(2, '0')}
            </span>
            <Art kind={a.art} size={32} label={false} />
            <div style={{ minWidth: 0 }}>
              <div style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{a.title}</div>
              <div style={{ opacity: .65, fontSize: 9, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{a.artist}</div>
            </div>
            <span style={{ opacity: .6, fontSize: 10 }}>{a.yr}</span>
            <span style={{ color: a.fmt === 'DSD' ? TmColors.phosphor : 'inherit', fontSize: 10 }}>{a.fmt}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Queue ──────────────────────────────────────────────────────
function Tm_Queue() {
  return (
    <div className="scr terminal">
      <TmHeader title="// QUEUE" sub="9 UP · 41:24" />
      <div style={{ padding: '14px 24px 0' }}>
        <div style={{ fontSize: 9, opacity: .55, marginBottom: 6, letterSpacing: '.06em' }}>
          [NOW_PLAYING] ────────────────────────────────────
        </div>
        <div style={{
          display: 'grid', gridTemplateColumns: '60px 1fr',
          gap: 14, alignItems: 'center', padding: '10px',
          background: 'rgba(240,164,32,.1)',
          border: `1px solid ${TmColors.phosphor}`,
          fontSize: 11,
        }}>
          <Art kind={TRACKS[0].art} size={60} label={false} />
          <div>
            <div className="phosphor" style={{ fontSize: 13 }}>► {TRACKS[0].title.toUpperCase()}</div>
            <div style={{ opacity: .7, fontSize: 10, marginTop: 2 }}>{TRACKS[0].artist}</div>
            <div style={{ marginTop: 8, fontSize: 10 }}>
              <span className="phosphor">[████░░░░░░]</span>
              <span style={{ opacity: .7 }}> 1:47/4:32 · FLAC 24/96</span>
            </div>
          </div>
        </div>

        <div style={{ fontSize: 9, opacity: .55, margin: '14px 0 6px', letterSpacing: '.06em' }}>
          [UP_NEXT] ────────────────────────────────────────
        </div>

        {TRACKS.slice(1, 9).map((t, i) => (
          <div key={t.id} style={{
            display: 'grid', gridTemplateColumns: '24px 34px 1fr 56px',
            gap: 10, alignItems: 'center', padding: '7px 0',
            fontSize: 11,
            borderBottom: `1px solid ${TmColors.rule}`,
          }}>
            <span style={{ fontSize: 10, opacity: .55 }}>{String(i + 2).padStart(2, '0')}</span>
            <Art kind={t.art} size={30} label={false} />
            <div style={{ minWidth: 0 }}>
              <div style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', fontSize: 11 }}>{t.title}</div>
              <div style={{ opacity: .55, fontSize: 9, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{t.artist}</div>
            </div>
            <span style={{ opacity: .7, fontSize: 10, textAlign: 'right' }}>{t.dur}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Lock ───────────────────────────────────────────────────────
function Tm_Lock() {
  return (
    <div className="scr terminal" style={{ background: '#050505' }}>
      <div style={{ position: 'absolute', inset: 0, padding: '28px 32px 36px', display: 'flex', flexDirection: 'column', justifyContent: 'space-between' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', fontSize: 10 }}>
          <span className="phosphor">[LOCK] HOLD ENGAGED</span>
          <span style={{ opacity: .55 }}>BATT 78%</span>
        </div>

        {/* Massive time */}
        <div style={{ textAlign: 'center' }}>
          <div className="phosphor" style={{ fontSize: 124, letterSpacing: '-.04em', lineHeight: .9, fontWeight: 400 }}>14:32</div>
          <div style={{ fontSize: 12, opacity: .55, marginTop: 14, letterSpacing: '.22em' }}>THU · 27 MAY 2026</div>
        </div>

        {/* Now playing */}
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 14, padding: 14, border: `1px solid ${TmColors.phosphor}` }}>
            <Art kind={TRACKS[0].art} size={68} label={false} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.12em' }}>► NOW PLAYING</div>
              <div style={{ fontSize: 18, textTransform: 'uppercase', lineHeight: 1.05, marginTop: 4 }}>{TRACKS[0].title}</div>
              <div style={{ fontSize: 11, opacity: .75, marginTop: 2 }}>BY {TRACKS[0].artist.toUpperCase()}</div>
              <div style={{ marginTop: 8, fontSize: 10 }}>
                <span className="phosphor">[██████░░░░░░░░]</span>
                <span style={{ opacity: .7 }}> 1:47/4:32</span>
              </div>
            </div>
          </div>
          <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, opacity: .45, marginTop: 16 }}>
            <span>VOL ‹‹/›› OK</span>
            <span>SLIDE HOLD ▼ TO WAKE</span>
            <span>PWR · WAKE</span>
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── Settings ───────────────────────────────────────────────────
function Tm_Settings() {
  return (
    <div className="scr terminal">
      <TmHeader title="// SETTINGS" sub="ROOT@NW-A55:~$" />
      <div style={{ padding: '14px 24px 0', fontSize: 11 }}>
        {SETTINGS.map(s => (
          <div key={s.group} style={{ border: `1px solid ${TmColors.rule}`, marginBottom: 12 }}>
            <div className="phosphor" style={{
              padding: '6px 12px',
              borderBottom: `1px solid ${TmColors.rule}`,
              fontSize: 9, letterSpacing: '.18em',
            }}>[ {s.group.toUpperCase()} ]</div>
            {s.items.map((it, ii) => (
              <div key={ii} style={{
                display: 'flex', justifyContent: 'space-between', alignItems: 'center',
                padding: '10px 12px',
                borderBottom: ii === s.items.length - 1 ? 'none' : `1px solid ${TmColors.rule}`,
                fontSize: 12,
              }}>
                <span>{it.label}</span>
                {it.type === 'toggle' ? (
                  <span className="phosphor" style={{ fontSize: 10 }}>
                    [{it.on ? 'X' : ' '}] {it.on ? 'ON' : 'OFF'}
                  </span>
                ) : (
                  <span style={{ opacity: .75 }}>{it.value} <span style={{ opacity: .4 }}>›</span></span>
                )}
              </div>
            ))}
          </div>
        ))}
        <div style={{ marginTop: 4, fontSize: 9, opacity: .55, textAlign: 'center' }}>
          <span className="phosphor">$</span> D-PAD · ENTER · BACK
        </div>
      </div>
    </div>
  );
}

// ─── Browse (no keyboard — category menu) ───────────────────────
function Tm_Search() {
  return (
    <div className="scr terminal">
      <TmHeader title="// BROWSE" sub="SELECT TO DRILL" />
      <div style={{ padding: '14px 24px 0', fontSize: 11 }}>
        {/* Category picker */}
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.18em', marginBottom: 6 }}>[ BROWSE BY ]</div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 0 }}>
          {[
            { name: 'ARTISTS',   count: 64,  active: false },
            { name: 'ALBUMS',    count: 124, active: true  },
            { name: 'GENRES',    count: 11,  active: false },
            { name: 'COMPOSERS', count: 18,  active: false },
            { name: 'YEARS',     count: 22,  active: false },
            { name: 'FORMATS',   count: 4,   active: false },
          ].map(c => (
            <div key={c.name} style={{
              padding: '10px 14px',
              border: `1px solid ${c.active ? TmColors.phosphor : TmColors.rule}`,
              background: c.active ? 'rgba(240,164,32,.1)' : 'transparent',
              marginRight: -1, marginBottom: -1,
              display: 'flex', justifyContent: 'space-between', alignItems: 'center',
            }}>
              <span style={{ color: c.active ? TmColors.phosphor : 'inherit', fontSize: 12 }}>
                {c.active ? '► ' : '  '}{c.name}
              </span>
              <span style={{ fontSize: 10, opacity: .65 }}>{c.count}</span>
            </div>
          ))}
        </div>

        {/* A-Z jump */}
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.18em', marginTop: 18, marginBottom: 6 }}>[ JUMP TO ]</div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(9, 1fr)', gap: 1, fontSize: 11 }}>
          {'ABCDEFGHIJKLMNOPQRSTUVWXYZ#'.split('').map(l => {
            const has = ['A','B','F','H','I','K','L','M','N','P','S','T'].includes(l);
            const sel = l === 'I';
            return (
              <div key={l} style={{
                aspectRatio: '1.1',
                border: `1px solid ${sel ? TmColors.phosphor : TmColors.rule}`,
                background: sel ? TmColors.phosphor : 'transparent',
                color: sel ? '#0d0d0d' : (has ? TmColors.text : 'rgba(232,230,220,.3)'),
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                fontWeight: sel ? 700 : 400,
                marginRight: -1, marginBottom: -1,
              }}>{l}</div>
            );
          })}
        </div>

        {/* Filter toggles */}
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.18em', marginTop: 18, marginBottom: 6 }}>[ FILTER ]</div>
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4, fontSize: 10 }}>
          {[
            ['HI-RES ONLY', true],
            ['DSD',         false],
            ['FLAC',        false],
            ['FAVORITES',   false],
            ['RECENTLY ADDED', false],
          ].map(([f, on]) => (
            <span key={f} style={{
              padding: '5px 10px',
              border: `1px solid ${on ? TmColors.phosphor : TmColors.rule}`,
              background: on ? TmColors.phosphor : 'transparent',
              color: on ? '#0d0d0d' : 'inherit',
            }}>{on ? '[X] ' : '[ ] '}{f}</span>
          ))}
        </div>

        {/* Results */}
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.18em', marginTop: 16, marginBottom: 4 }}>[ I · 2 ALBUMS ]</div>
        {[
          { name: 'IGNORANCE',           meta: 'THE WEATHER STATION · 2021', art: 'midnight' },
          { name: 'INSIDE WAVES',        meta: 'CRUMB · 2024',              art: 'halcyon'  },
        ].map((r, i) => (
          <div key={r.name} style={{
            display: 'grid', gridTemplateColumns: '36px 1fr 14px',
            gap: 10, alignItems: 'center', padding: '6px 0',
            borderTop: `1px solid ${TmColors.rule}`,
            borderBottom: i === 1 ? `1px solid ${TmColors.rule}` : 'none',
          }}>
            <Art kind={r.art} size={32} label={false} />
            <div>
              <div style={{ fontSize: 12 }}>{r.name}</div>
              <div style={{ opacity: .6, fontSize: 9 }}>{r.meta}</div>
            </div>
            <span style={{ opacity: .4 }}>›</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Equalizer — horizontal sliders stacked vertically ──────────
function Tm_EQ() {
  return (
    <div className="scr terminal">
      <TmHeader title="// EQUALIZER" sub="PRESET: CUSTOM_A1" />
      <div style={{ padding: '14px 24px 0' }}>
        {/* Presets */}
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.18em', marginBottom: 6 }}>[ PRESET ]</div>
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4, fontSize: 10, marginBottom: 14 }}>
          {['OFF', 'A1*', 'A2', 'A3', 'HEAVY', 'POP', 'JAZZ', 'VOCAL', 'CUSTOM'].map((p, i) => (
            <span key={p} style={{
              padding: '5px 9px',
              border: `1px solid ${i === 1 ? TmColors.phosphor : TmColors.rule}`,
              background: i === 1 ? TmColors.phosphor : 'transparent',
              color: i === 1 ? '#0d0d0d' : 'inherit',
            }}>{p}</span>
          ))}
        </div>

        {/* dB scale header */}
        <div style={{ display: 'grid', gridTemplateColumns: '46px 1fr 46px', gap: 10, marginBottom: 6, fontSize: 9, opacity: .55 }}>
          <span style={{ textAlign: 'right' }}>BAND</span>
          <span style={{ display: 'flex', justifyContent: 'space-between' }}>
            <span>−12</span><span>−6</span><span>0</span><span>+6</span><span>+12</span>
          </span>
          <span style={{ textAlign: 'right' }}>GAIN</span>
        </div>

        {/* Sliders */}
        {EQ_BANDS.map(b => (
          <TmRow
            key={b.hz}
            label={b.hz + ' Hz'}
            value={b.db}
            valueDisplay={(b.db > 0 ? '+' : '') + b.db + 'dB'}
            cells={30}
          />
        ))}

        {/* Zero line indicator below sliders */}
        <div style={{ marginTop: 14, padding: '8px 12px', border: `1px solid ${TmColors.rule}`, fontSize: 10, display: 'flex', justifyContent: 'space-between' }}>
          <span style={{ opacity: .7 }}>DC PHASE LINEARIZER</span>
          <span className="phosphor">TYPE A · LOW</span>
        </div>
        <div style={{ padding: '8px 12px', border: `1px solid ${TmColors.rule}`, borderTop: 'none', fontSize: 10, display: 'flex', justifyContent: 'space-between' }}>
          <span style={{ opacity: .7 }}>DSEE ULTIMATE</span>
          <span className="phosphor">[X] ON</span>
        </div>

        <div style={{ marginTop: 12, fontSize: 9, opacity: .55, textAlign: 'center', letterSpacing: '.1em' }}>
          [↑/↓] ADJUST · [‹/›] BAND · [ENTER] SAVE
        </div>
      </div>
    </div>
  );
}

Object.assign(window, {
  Tm_NowPlayingHero, Tm_NowPlayingDense, Tm_Library,
  Tm_Queue, Tm_Lock, Tm_Settings, Tm_Search, Tm_EQ,
});
