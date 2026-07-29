// ────────────────────────────────────────────────────────────────
// Direction 2 — NOCTURNE (portrait 480×800)
//
// Dark editorial. Pure-near-black field, generous breathing room,
// serif display titles, soft-violet accent. Signature visualization
// is a single thin waveform line — not bars. Big album art (like
// Hi-Res) but no frame, no chrome — the art just sits in space.
// ────────────────────────────────────────────────────────────────

const NcColors = {
  bg:       '#060608',
  text:     '#ede5d2',
  dim:      'rgba(237,229,210,.45)',
  faint:    'rgba(237,229,210,.18)',
  rule:     'rgba(237,229,210,.10)',
  accent:   '#c4b6ff',
  surface:  'rgba(255,255,255,.025)',
};

// Reusable: thin continuous waveform line drawn as SVG.
// Used as the signature visual element in NowPlaying / Lock / Lyrics.
function NcWaveform({ height = 28, playedPct = 0.38, color, dim }) {
  const pts = [
    0.20, 0.45, 0.30, 0.60, 0.85, 0.70, 0.50, 0.78, 0.32, 0.55,
    0.40, 0.65, 0.50, 0.72, 0.45, 0.30, 0.58, 0.80, 0.62, 0.42,
    0.55, 0.74, 0.36, 0.50, 0.68, 0.82, 0.45, 0.30, 0.42, 0.55,
    0.60, 0.45, 0.30, 0.50, 0.65, 0.55, 0.40, 0.28, 0.20, 0.15,
  ];
  const W = 100; // viewBox width (percent-like)
  const stepX = W / (pts.length - 1);
  const path = pts.map((v, i) => `${i === 0 ? 'M' : 'L'}${(i * stepX).toFixed(2)} ${((1 - v) * 50 + 25).toFixed(2)}`).join(' ');
  return (
    <svg className="wave" viewBox={`0 0 ${W} 100`} preserveAspectRatio="none" style={{ height }}>
      <defs>
        <linearGradient id="nc-wave-grad" x1="0" x2="1" y1="0" y2="0">
          <stop offset={`${playedPct * 100}%`} stopColor={color || NcColors.accent} />
          <stop offset={`${playedPct * 100}%`} stopColor={dim || NcColors.faint} />
        </linearGradient>
      </defs>
      <path d={path} stroke="url(#nc-wave-grad)" strokeWidth="0.8" fill="none" vectorEffect="non-scaling-stroke" />
      <line x1={playedPct * W} x2={playedPct * W} y1="20" y2="80" stroke={color || NcColors.accent} strokeWidth="0.5" vectorEffect="non-scaling-stroke" opacity=".5" />
    </svg>
  );
}

// Small section header — tracked caps in mono.
function NcKicker({ children, accent = false, style }) {
  return (
    <div className="mono" style={{ fontSize: 9, letterSpacing: '.22em', color: accent ? NcColors.accent : NcColors.dim, textTransform: 'uppercase', ...style }}>
      {children}
    </div>
  );
}

// ─── Now Playing · Hero ─────────────────────────────────────────
function Nc_NowPlayingHero({ track = TRACKS[0] }) {
  const t = track;
  return (
    <div className="scr nocturne">
      <StatusBar
        badge={<span className="nb-badge">Hi-Res</span>}
        right={<span className="mono" style={{ opacity: .55, fontSize: 9 }}>{t.codec} {t.bits}/{t.rate.replace(' kHz','k').replace(' MHz','M')}</span>}
      />

      {/* Album art — generous, no border */}
      <div style={{ padding: '24px 30px 0' }}>
        <Art kind={t.art} size={420} label={false} />
      </div>

      {/* Track meta + waveform sit in the bottom half. */}
      <div style={{ padding: '22px 32px 0' }}>
        <NcKicker accent>Track 03 / 12 · Side A</NcKicker>

        <div className="display" style={{
          fontSize: 36, lineHeight: 1.0, marginTop: 8,
        }}>
          {t.title}
        </div>
        <div style={{ fontSize: 14, marginTop: 8, opacity: .8 }}>
          <span className="italic" style={{ fontSize: 18 }}>{t.artist}</span>
          <span className="mono" style={{ marginLeft: 8, fontSize: 10, opacity: .5 }}>· {t.album}, 2021</span>
        </div>

        {/* Waveform progress */}
        <div style={{ marginTop: 22 }}>
          <NcWaveform height={32} playedPct={0.38} />
          <div className="mono" style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, marginTop: 4, opacity: .55 }}>
            <span>{t.cur}</span>
            <span>−2:45</span>
          </div>
        </div>

        {/* Controls — minimal, no buttons; just icons in a row */}
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginTop: 24 }}>
          <IconShuffle size={18} style={{ opacity: .5 }} />
          <IconPrev size={28} />
          <div style={{
            width: 64, height: 64, borderRadius: '50%',
            border: `1px solid ${NcColors.accent}`,
            color: NcColors.accent,
            display: 'flex', alignItems: 'center', justifyContent: 'center',
          }}>
            <IconPause size={24} />
          </div>
          <IconNext size={28} />
          <IconRepeat size={18} style={{ opacity: .5 }} />
        </div>
      </div>
    </div>
  );
}

// ─── Now Playing · Dense (codec-first, technical) ──────────────
function Nc_NowPlayingDense({ track = TRACKS[3] }) {
  const t = track;
  return (
    <div className="scr nocturne">
      <StatusBar
        badge={<span className="nb-badge">Hi-Res</span>}
        right={<span className="mono" style={{ opacity: .55, fontSize: 9 }}>DSEE HX · LDAC</span>}
      />

      {/* Two-up: art + meta */}
      <div style={{ padding: '20px 30px 0', display: 'grid', gridTemplateColumns: '170px 1fr', gap: 22 }}>
        <Art kind={t.art} size={170} label={false} />
        <div>
          <NcKicker accent style={{ marginBottom: 8 }}>Track 04 / 12</NcKicker>
          <div className="display" style={{ fontSize: 26, lineHeight: 1.0 }}>{t.title}</div>
          <div className="italic" style={{ fontSize: 16, marginTop: 6, opacity: .8 }}>{t.artist}</div>
          <div className="mono" style={{ fontSize: 9, opacity: .45, marginTop: 6, letterSpacing: '.12em', textTransform: 'uppercase' }}>{t.album}</div>
        </div>
      </div>

      {/* Spec table — restrained, hairlines */}
      <div style={{ padding: '24px 30px 0' }}>
        <NcKicker accent>Signal Chain</NcKicker>
        <div className="mono" style={{ marginTop: 10, fontSize: 11 }}>
          {[
            ['Codec',            'DSD · DSF'],
            ['Sample rate',      '5.6 MHz'],
            ['Bit depth',        '1-bit'],
            ['DSD playback',     'DoP'],
            ['DSEE HX',          'Standard'],
            ['DC Phase Linearizer', 'Type A · Low'],
            ['Equalizer',        'Custom A1'],
            ['Output',           '3.5 mm Stereo'],
          ].map(([k, v]) => (
            <div key={k} style={{
              display: 'flex', justifyContent: 'space-between',
              padding: '8px 0',
              borderTop: `1px solid ${NcColors.rule}`,
            }}>
              <span style={{ opacity: .55, fontSize: 10 }}>{k}</span>
              <span style={{ fontSize: 11 }}>{v}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Waveform + controls */}
      <div style={{ position: 'absolute', left: 30, right: 30, bottom: 28 }}>
        <NcWaveform height={24} playedPct={0.56} />
        <div className="mono" style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, marginTop: 4, opacity: .55 }}>
          <span>{t.cur}</span><span>−3:12</span>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginTop: 14 }}>
          <IconShuffle size={16} style={{ opacity: .45 }} />
          <IconPrev size={24} />
          <div style={{ width: 50, height: 50, borderRadius: '50%', border: `1px solid ${NcColors.accent}`, color: NcColors.accent, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <IconPause size={20} />
          </div>
          <IconNext size={24} />
          <IconRepeat size={16} style={{ opacity: .45 }} />
        </div>
      </div>
    </div>
  );
}

// ─── Library ────────────────────────────────────────────────────
function Nc_Library() {
  const tabs = ['Albums', 'Artists', 'Songs', 'Folders'];
  return (
    <div className="scr nocturne">
      <StatusBar badge={<span className="nb-badge">Hi-Res</span>} />
      <div style={{ padding: '16px 30px 0' }}>
        <div className="display" style={{ fontSize: 38, lineHeight: 1.0 }}>
          The <span className="italic">Library</span>
        </div>
        <NcKicker style={{ marginTop: 6 }}>124 albums · 1,840 tracks</NcKicker>

        {/* Tabs as dotted nav */}
        <div className="mono" style={{ display: 'flex', gap: 16, marginTop: 18, fontSize: 10, letterSpacing: '.16em', textTransform: 'uppercase' }}>
          {tabs.map((t, i) => (
            <div key={t} style={{
              color: i === 0 ? NcColors.accent : NcColors.dim,
              borderBottom: i === 0 ? `1px solid ${NcColors.accent}` : '1px solid transparent',
              paddingBottom: 4,
            }}>{t}</div>
          ))}
        </div>

        {/* Asymmetric 2-up grid — first album hero-sized */}
        <div style={{ marginTop: 16 }}>
          <div>
            <Art kind={ALBUMS[0].art} size={400} label={false} style={{ width: '100%', aspectRatio: '1', height: 'auto' }} />
            <div style={{ marginTop: 10, display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', gap: 12 }}>
              <div>
                <div className="serif italic" style={{ fontSize: 22 }}>{ALBUMS[0].title}</div>
                <div className="mono" style={{ fontSize: 10, opacity: .55, marginTop: 2 }}>{ALBUMS[0].artist} · {ALBUMS[0].yr}</div>
              </div>
              <span className="nb-badge">{ALBUMS[0].fmt}</span>
            </div>
          </div>

          {/* Row of small thumbs */}
          <div style={{ marginTop: 18, display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 10 }}>
            {ALBUMS.slice(1, 4).map((a, i) => (
              <div key={i}>
                <Art kind={a.art} size={120} label={false} style={{ width: '100%', aspectRatio: '1', height: 'auto' }} />
                <div className="serif" style={{ fontSize: 12, marginTop: 6, lineHeight: 1.1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{a.title}</div>
                <div className="mono" style={{ fontSize: 8, opacity: .5, letterSpacing: '.08em' }}>{a.artist}</div>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── Queue ──────────────────────────────────────────────────────
function Nc_Queue() {
  return (
    <div className="scr nocturne">
      <StatusBar badge={<span className="nb-badge">Hi-Res</span>} />
      <div style={{ padding: '16px 30px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 18 }}>
          <IconBack size={18} style={{ opacity: .55 }} />
          <div className="display" style={{ fontSize: 30 }}>
            <span className="italic">Up</span> Next
          </div>
          <NcKicker style={{ marginLeft: 'auto' }}>9 · 41:24</NcKicker>
        </div>

        {/* Now playing — quiet card */}
        <div style={{
          padding: '14px',
          background: NcColors.surface,
          borderTop: `1px solid ${NcColors.rule}`,
          borderBottom: `1px solid ${NcColors.rule}`,
          marginBottom: 14,
        }}>
          <NcKicker accent>Now Playing</NcKicker>
          <div style={{ display: 'flex', gap: 14, alignItems: 'center', marginTop: 8 }}>
            <Art kind={TRACKS[0].art} size={56} label={false} />
            <div>
              <div className="serif" style={{ fontSize: 18, lineHeight: 1 }}>{TRACKS[0].title}</div>
              <div className="italic" style={{ fontSize: 12, opacity: .8, marginTop: 2 }}>{TRACKS[0].artist}</div>
            </div>
          </div>
        </div>

        {TRACKS.slice(1, 8).map((t, i) => (
          <div key={t.id} style={{
            display: 'grid',
            gridTemplateColumns: '24px 38px 1fr 50px',
            gap: 12, alignItems: 'center',
            padding: '11px 0',
            borderBottom: `1px solid ${NcColors.rule}`,
          }}>
            <span className="mono" style={{ fontSize: 9, opacity: .4 }}>{String(i + 1).padStart(2, '0')}</span>
            <Art kind={t.art} size={38} label={false} />
            <div style={{ minWidth: 0 }}>
              <div className="serif" style={{ fontSize: 15, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{t.title}</div>
              <div className="italic" style={{ fontSize: 11, opacity: .65, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{t.artist}</div>
            </div>
            <span className="mono" style={{ fontSize: 10, opacity: .55, textAlign: 'right' }}>{t.dur}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Lock ───────────────────────────────────────────────────────
function Nc_Lock() {
  return (
    <div className="scr nocturne amoled">
      <div style={{ position: 'absolute', inset: 0, display: 'flex', flexDirection: 'column', padding: '32px 30px' }}>
        {/* Top: HOLD pill */}
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <span className="nb-badge"><IconLock size={10} /> HOLD</span>
          <span className="mono" style={{ fontSize: 10, opacity: .55, letterSpacing: '.14em' }}>78%</span>
        </div>

        {/* Time — serif, huge, italic */}
        <div style={{ marginTop: 64, textAlign: 'left' }}>
          <div className="display" style={{ fontSize: 140, lineHeight: 1, letterSpacing: '-.04em' }}>14:32</div>
          <div className="italic" style={{ fontSize: 22, opacity: .65, marginTop: 4 }}>Thursday, 27 May</div>
        </div>

        {/* Now playing line — no art */}
        <div style={{ marginTop: 'auto' }}>
          <NcKicker accent>Now Playing</NcKicker>
          <div className="display" style={{ fontSize: 28, lineHeight: 1.05, marginTop: 6 }}>{TRACKS[0].title}</div>
          <div className="italic" style={{ fontSize: 14, opacity: .65, marginTop: 4 }}>{TRACKS[0].artist}</div>

          <div style={{ marginTop: 18 }}>
            <NcWaveform height={20} playedPct={0.38} />
          </div>
          <div className="mono" style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, marginTop: 4, opacity: .45 }}>
            <span>1:47</span><span>−2:45</span>
          </div>

          <div className="mono" style={{ textAlign: 'center', fontSize: 9, opacity: .35, letterSpacing: '.24em', marginTop: 22 }}>
            SLIDE HOLD ▼ TO UNLOCK
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── Settings ───────────────────────────────────────────────────
function Nc_Settings() {
  return (
    <div className="scr nocturne">
      <StatusBar badge={<span className="nb-badge">Hi-Res</span>} />
      <div style={{ padding: '16px 30px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 18 }}>
          <IconBack size={18} style={{ opacity: .55 }} />
          <div className="display" style={{ fontSize: 30 }}>Settings</div>
        </div>

        {SETTINGS.slice(0, 3).map(s => (
          <div key={s.group} style={{ marginBottom: 18 }}>
            <NcKicker accent style={{ marginBottom: 8 }}>{s.group}</NcKicker>
            {s.items.map((it, ii) => (
              <div key={ii} style={{
                display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                padding: '11px 0',
                borderTop: ii === 0 ? `1px solid ${NcColors.rule}` : 'none',
                borderBottom: `1px solid ${NcColors.rule}`,
              }}>
                <span className="serif" style={{ fontSize: 15 }}>{it.label}</span>
                <span style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  {it.type === 'toggle' ? (
                    <span style={{
                      width: 32, height: 18, borderRadius: 10,
                      background: it.on ? NcColors.accent : 'rgba(255,255,255,.10)',
                      position: 'relative',
                    }}>
                      <span style={{
                        position: 'absolute', top: 2, left: it.on ? 16 : 2,
                        width: 14, height: 14, borderRadius: '50%',
                        background: it.on ? '#0a0a14' : '#fff',
                      }} />
                    </span>
                  ) : (
                    <>
                      <span className="mono" style={{ fontSize: 10, opacity: .55 }}>{it.value}</span>
                      <IconChevron size={12} style={{ opacity: .35 }} />
                    </>
                  )}
                </span>
              </div>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Browse ─────────────────────────────────────────────────────
function Nc_Search() {
  const cats = [
    { name: 'Artists',    count: 64,  active: true  },
    { name: 'Albums',     count: 124, active: false },
    { name: 'Genres',     count: 11,  active: false },
    { name: 'Composers',  count: 18,  active: false },
    { name: 'Years',      count: 22,  active: false },
    { name: 'Folders',    count: 7,   active: false },
  ];
  const letters = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ#'.split('');
  const available = new Set(['A','B','F','H','K','M','N','P','S','T']);
  const artists = [
    { name: 'Neil Young',    meta: '6 albums · 84 tracks',  art: 'harvest',  fmt: 'FLAC' },
    { name: 'Nicolas Jaar',  meta: '2 albums · 24 tracks',  art: 'midnight', fmt: 'FLAC' },
    { name: 'Nils Frahm',    meta: '4 albums · 38 tracks',  art: 'ferns',    fmt: 'DSD'  },
  ];
  return (
    <div className="scr nocturne">
      <StatusBar badge={<span className="nb-badge">Hi-Res</span>} />
      <div style={{ padding: '16px 30px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 16 }}>
          <IconBack size={18} style={{ opacity: .55 }} />
          <div className="display" style={{ fontSize: 30 }}>Browse</div>
        </div>

        <NcKicker accent style={{ marginBottom: 8 }}>By</NcKicker>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 6 }}>
          {cats.map(c => (
            <div key={c.name} style={{
              padding: '12px 14px',
              background: c.active ? NcColors.accent : NcColors.surface,
              color: c.active ? '#0a0a14' : 'inherit',
              borderRadius: 2,
            }}>
              <div className="serif" style={{ fontSize: 15, lineHeight: 1 }}>{c.name}</div>
              <div className="mono" style={{ fontSize: 9, opacity: c.active ? .65 : .5, marginTop: 4 }}>{c.count}</div>
            </div>
          ))}
        </div>

        <NcKicker accent style={{ marginTop: 18, marginBottom: 6 }}>Jump to</NcKicker>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(9, 1fr)', gap: 3 }}>
          {letters.map(l => {
            const has = available.has(l);
            const sel = l === 'N';
            return (
              <div key={l} className="mono" style={{
                aspectRatio: '1',
                background: sel ? NcColors.accent : 'transparent',
                color: sel ? '#0a0a14' : (has ? NcColors.text : 'rgba(237,229,210,.18)'),
                border: `1px solid ${sel ? NcColors.accent : (has ? NcColors.rule : 'transparent')}`,
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                fontSize: 12, fontWeight: sel ? 600 : 400,
              }}>{l}</div>
            );
          })}
        </div>

        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginTop: 18, marginBottom: 6 }}>
          <NcKicker accent>N · 3 Artists</NcKicker>
          <NcKicker>Hi-Res only on</NcKicker>
        </div>
        {artists.map((a, i) => (
          <div key={a.name} style={{
            display: 'grid', gridTemplateColumns: '44px 1fr 50px 14px',
            gap: 12, alignItems: 'center', padding: '11px 0',
            borderTop: i === 0 ? `1px solid ${NcColors.rule}` : 'none',
            borderBottom: `1px solid ${NcColors.rule}`,
          }}>
            <Art kind={a.art} size={44} label={false} />
            <div>
              <div className="serif" style={{ fontSize: 15 }}>{a.name}</div>
              <div className="italic" style={{ fontSize: 11, opacity: .55 }}>{a.meta}</div>
            </div>
            <span className="mono" style={{ fontSize: 9, opacity: .55, color: a.fmt === 'DSD' ? NcColors.accent : 'inherit', textAlign: 'right' }}>{a.fmt}</span>
            <IconChevron size={12} style={{ opacity: .35 }} />
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Equalizer ──────────────────────────────────────────────────
function Nc_EQ() {
  return (
    <div className="scr nocturne">
      <StatusBar badge={<span className="nb-badge">Hi-Res</span>} />
      <div style={{ padding: '16px 30px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 4 }}>
          <IconBack size={18} style={{ opacity: .55 }} />
          <div className="display" style={{ fontSize: 30 }}>Equalizer</div>
          <NcKicker accent style={{ marginLeft: 'auto' }}>Custom A1</NcKicker>
        </div>

        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 16 }}>
          {['Off', 'A1', 'A2', 'A3', 'Heavy', 'Pop', 'Jazz', 'Vocal', 'Custom'].map((p, i) => (
            <div key={p} className="mono" style={{
              padding: '6px 12px',
              borderRadius: 999,
              border: `1px solid ${i === 1 ? NcColors.accent : NcColors.rule}`,
              color: i === 1 ? NcColors.accent : 'inherit',
              fontSize: 10, letterSpacing: '.1em',
            }}>{p}</div>
          ))}
        </div>

        {/* Sliders */}
        <div style={{ marginTop: 28, height: 420, display: 'flex', position: 'relative', paddingLeft: 30, paddingBottom: 28 }}>
          <div style={{ position: 'absolute', inset: '0 0 28px 0', pointerEvents: 'none' }}>
            {[+10, +5, 0, -5, -10].map((db, i) => (
              <div key={db} style={{ position: 'absolute', left: 0, right: 0, top: `${i * 25}%`, display: 'flex', alignItems: 'center' }}>
                <span className="mono" style={{ fontSize: 9, opacity: .4, width: 26 }}>{db > 0 ? '+' : ''}{db}</span>
                <div style={{ flex: 1, height: 1, background: NcColors.rule, opacity: db === 0 ? 1.6 : 1 }} />
              </div>
            ))}
          </div>
          {EQ_BANDS.map(b => {
            const pct = (b.db + 12) / 24 * 100;
            return (
              <div key={b.hz} style={{ flex: 1, height: '100%', position: 'relative' }}>
                <div style={{ position: 'absolute', top: 0, bottom: 28, width: 1, background: NcColors.rule, left: '50%', transform: 'translateX(-50%)' }} />
                <div style={{
                  position: 'absolute',
                  bottom: `calc(28px + ${pct}% - 7px)`,
                  width: 14, height: 14, borderRadius: '50%',
                  background: NcColors.bg, border: `1.5px solid ${NcColors.accent}`,
                  left: '50%', transform: 'translateX(-50%)',
                }} />
                <div className="mono" style={{ position: 'absolute', bottom: 8, left: '50%', transform: 'translateX(-50%)', fontSize: 9, opacity: .55 }}>{b.hz}</div>
                <div className="mono" style={{ position: 'absolute', bottom: `calc(28px + ${pct}% + 12px)`, left: '50%', transform: 'translateX(-50%)', fontSize: 9, color: NcColors.accent }}>{b.db > 0 ? '+' : ''}{b.db}</div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

Object.assign(window, {
  Nc_NowPlayingHero, Nc_NowPlayingDense, Nc_Library, Nc_Queue,
  Nc_Lock, Nc_Settings, Nc_Search, Nc_EQ,
  NcWaveform, NcKicker, NcColors,
});
