// ────────────────────────────────────────────────────────────────
// Direction 1 — HI-RES (portrait 480×800)
// Faithful Sony Walkman: deep near-black, gold hi-res accent,
// monospaced technical metadata.
// ────────────────────────────────────────────────────────────────

const HiResColors = {
  gold: '#d4a955',
  textDim: 'rgba(230,227,220,.55)',
  surface: 'rgba(255,255,255,.05)',
  border: 'rgba(255,255,255,.08)',
};

// ─── Now Playing · Hero ─────────────────────────────────────────
function HiRes_NowPlayingHero({ track = TRACKS[0], theme = 'dark' }) {
  const t = track;
  return (
    <div className={`scr hires ${theme === 'light' ? 'light' : ''}`}>
      <StatusBar
        badge={<span className="hires-badge">Hi-Res Audio</span>}
        right={<span className="mono" style={{ opacity: .55, fontSize: 9 }}>FLAC&nbsp;24/96</span>}
      />

      {/* Album art — full width square */}
      <div style={{ padding: '20px 28px 0' }}>
        <Art kind={t.art} size={424} label={{ l: `${t.album.toUpperCase()} · ${t.artist.toUpperCase()}`, r: 'A·1' }} />
      </div>

      {/* Track info */}
      <div style={{ padding: '24px 28px 0' }}>
        <div className="mono" style={{ fontSize: 10, letterSpacing: '.18em', color: HiResColors.gold }}>
          TRACK 03 / 12
        </div>
        <div style={{ fontSize: 28, fontWeight: 600, letterSpacing: '-.01em', lineHeight: 1.1, marginTop: 8 }}>
          {t.title}
        </div>
        <div style={{ fontSize: 15, marginTop: 6, opacity: .82 }}>{t.artist}</div>
        <div className="mono" style={{ fontSize: 10, letterSpacing: '.12em', marginTop: 4, opacity: .5 }}>
          {t.album.toUpperCase()} · 2021
        </div>

        {/* Progress */}
        <div style={{ marginTop: 24 }}>
          <div className="prog" style={{ marginBottom: 8 }}>
            <i style={{ '--p': '38%', background: HiResColors.gold, opacity: 1 }} />
          </div>
          <div className="mono" style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, opacity: .65 }}>
            <span>{t.cur}</span>
            <span>−2:45</span>
          </div>
        </div>

        {/* Controls */}
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginTop: 24 }}>
          <IconShuffle size={20} style={{ opacity: .55 }} />
          <IconPrev size={32} />
          <div style={{
            width: 72, height: 72, borderRadius: '50%',
            background: HiResColors.gold, color: '#1a1612',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
          }}>
            <IconPause size={28} />
          </div>
          <IconNext size={32} />
          <IconRepeat size={20} style={{ opacity: .55 }} />
        </div>

        {/* Aux row */}
        <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 22, opacity: .55, padding: '0 6px' }}>
          <IconHeart size={18} />
          <IconQueue size={18} />
          <IconMore size={18} />
        </div>
      </div>
    </div>
  );
}

// ─── Now Playing · Dense ────────────────────────────────────────
function HiRes_NowPlayingDense({ track = TRACKS[1] }) {
  const t = track;
  const bars = [40, 60, 35, 80, 90, 55, 70, 95, 50, 30, 65, 80, 40, 70, 55, 35, 60, 45, 75, 50];
  return (
    <div className="scr hires">
      <StatusBar
        badge={<span className="hires-badge">Hi-Res Audio</span>}
        right={<span className="mono" style={{ opacity: .55, fontSize: 9 }}>FLAC 24/96 · LDAC</span>}
      />

      {/* Top row: thumb + meta */}
      <div style={{ padding: '18px 24px 0', display: 'grid', gridTemplateColumns: '160px 1fr', gap: 18 }}>
        <Art kind={t.art} size={160} label={false} />
        <div>
          <div className="mono" style={{ fontSize: 9, letterSpacing: '.14em', color: HiResColors.gold }}>
            TRACK 02 / 12
          </div>
          <div style={{ fontSize: 20, fontWeight: 600, lineHeight: 1.1, marginTop: 6 }}>{t.title}</div>
          <div style={{ fontSize: 12, opacity: .75, marginTop: 4 }}>{t.artist}</div>
          <div className="mono" style={{ fontSize: 9, opacity: .45, marginTop: 4 }}>{t.album.toUpperCase()}</div>
          <div style={{ display: 'flex', gap: 14, marginTop: 12, opacity: .6 }}>
            <IconHeart size={16} />
            <IconQueue size={16} />
            <IconMore size={16} />
          </div>
        </div>
      </div>

      {/* Visualizer */}
      <div style={{ padding: '18px 24px 0' }}>
        <div style={{ display: 'flex', alignItems: 'end', height: 70, gap: 4 }}>
          {bars.map((h, i) => (
            <div key={i} style={{
              flex: 1, height: `${h}%`,
              background: i < bars.length * 0.4 ? HiResColors.gold : 'rgba(255,255,255,.18)',
            }} />
          ))}
        </div>
      </div>

      {/* Tech spec grid */}
      <div style={{ padding: '18px 24px 0' }}>
        <div className="mono" style={{ fontSize: 9, letterSpacing: '.16em', color: HiResColors.gold, marginBottom: 8 }}>
          SIGNAL CHAIN
        </div>
        <div className="mono" style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 0, fontSize: 10, letterSpacing: '.08em' }}>
          {[
            ['FORMAT', 'FLAC'],
            ['SAMPLE', '96.0 kHz'],
            ['DEPTH',  '24 bit'],
            ['BITRATE','2304 kbps'],
            ['DSEE',   'ULTIMATE'],
            ['DC LIN.','TYPE A · LO'],
            ['EQ',     'CUSTOM · A1'],
            ['BAL.',   '0.0 dB'],
            ['OUTPUT', '3.5 mm'],
          ].map(([k, v]) => (
            <div key={k} style={{
              padding: '7px 0',
              borderTop: `1px solid ${HiResColors.border}`,
            }}>
              <div style={{ opacity: .5, fontSize: 9 }}>{k}</div>
              <div style={{ marginTop: 2 }}>{v}</div>
            </div>
          ))}
        </div>
      </div>

      {/* Progress + controls */}
      <div style={{ padding: '20px 24px 0' }}>
        <div className="prog">
          <i style={{ '--p': '38%', background: HiResColors.gold }} />
        </div>
        <div className="mono" style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, marginTop: 8, opacity: .65 }}>
          <span>{t.cur}</span><span>−2:49</span>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginTop: 14 }}>
          <IconShuffle size={18} style={{ opacity: .5 }} />
          <IconPrev size={28} />
          <div style={{
            width: 60, height: 60, borderRadius: '50%', background: HiResColors.gold, color: '#1a1612',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
          }}><IconPause size={24} /></div>
          <IconNext size={28} />
          <IconRepeat size={18} style={{ opacity: .5 }} />
        </div>
      </div>
    </div>
  );
}

// ─── Library ────────────────────────────────────────────────────
function HiRes_Library() {
  const tabs = ['Albums', 'Artists', 'Songs', 'Folders'];
  return (
    <div className="scr hires">
      <StatusBar badge={<span className="hires-badge">Hi-Res Audio</span>} />
      <div style={{ padding: '14px 24px 0' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
          <div style={{ fontSize: 24, fontWeight: 600 }}>Library</div>
          <div style={{ display: 'flex', gap: 14, opacity: .65 }}>
            <IconSearch size={18} />
            <IconGrid size={18} />
          </div>
        </div>

        <div className="mono" style={{ display: 'flex', gap: 18, marginTop: 14, fontSize: 10, letterSpacing: '.14em' }}>
          {tabs.map((t, i) => (
            <div key={t} style={{
              paddingBottom: 6,
              borderBottom: i === 0 ? `2px solid ${HiResColors.gold}` : '2px solid transparent',
              color: i === 0 ? HiResColors.gold : 'rgba(255,255,255,.45)',
            }}>{t.toUpperCase()}</div>
          ))}
        </div>

        {/* 2-col album grid */}
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 14, marginTop: 16 }}>
          {ALBUMS.slice(0, 4).map((a, i) => (
            <div key={i}>
              <div style={{ width: '100%', aspectRatio: '1', position: 'relative' }}>
                <Art kind={a.art} size={210} label={false} style={{ width: '100%', height: '100%' }} />
              </div>
              <div style={{ fontSize: 13, fontWeight: 600, marginTop: 8, lineHeight: 1.2, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                {a.title}
              </div>
              <div className="mono" style={{ fontSize: 9, opacity: .5, letterSpacing: '.06em', marginTop: 2, display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', maxWidth: 120 }}>{a.artist}</span>
                <span style={{ color: a.fmt === 'DSD' ? HiResColors.gold : 'inherit' }}>{a.fmt}</span>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// ─── Queue ──────────────────────────────────────────────────────
function HiRes_Queue() {
  return (
    <div className="scr hires">
      <StatusBar badge={<span className="hires-badge">Hi-Res Audio</span>} />
      <div style={{ padding: '14px 24px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 14 }}>
          <IconBack size={16} style={{ opacity: .65 }} />
          <div style={{ fontSize: 22, fontWeight: 600 }}>Up Next</div>
          <div className="mono" style={{ marginLeft: 'auto', fontSize: 9, opacity: .55, letterSpacing: '.12em' }}>
            9 TRACKS · 41:24
          </div>
        </div>

        {/* Now-playing card */}
        <div style={{
          display: 'grid', gridTemplateColumns: '64px 1fr',
          gap: 14, alignItems: 'center', padding: '12px',
          background: HiResColors.surface,
          border: `1px solid ${HiResColors.gold}`,
          borderRadius: 2, marginBottom: 14,
        }}>
          <Art kind={TRACKS[0].art} size={64} label={false} />
          <div>
            <div className="mono" style={{ fontSize: 9, color: HiResColors.gold, letterSpacing: '.14em' }}>NOW PLAYING</div>
            <div style={{ fontSize: 14, fontWeight: 600, marginTop: 2 }}>{TRACKS[0].title}</div>
            <div style={{ fontSize: 11, opacity: .65 }}>{TRACKS[0].artist}</div>
          </div>
        </div>

        {/* Up next list */}
        {TRACKS.slice(1, 9).map((t, i) => (
          <div key={t.id} style={{
            display: 'grid',
            gridTemplateColumns: '20px 36px 1fr 50px 14px',
            gap: 12, alignItems: 'center',
            padding: '9px 0',
            borderBottom: `1px solid ${HiResColors.border}`,
          }}>
            <span className="mono" style={{ fontSize: 9, opacity: .45 }}>{String(i + 1).padStart(2, '0')}</span>
            <Art kind={t.art} size={36} label={false} />
            <div style={{ minWidth: 0 }}>
              <div style={{ fontSize: 13, fontWeight: 500, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{t.title}</div>
              <div className="mono" style={{ fontSize: 9, opacity: .5, letterSpacing: '.06em' }}>{t.artist}</div>
            </div>
            <span className="mono" style={{ fontSize: 9, opacity: .55, textAlign: 'right' }}>{t.dur}</span>
            <IconMore size={12} style={{ opacity: .4 }} />
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Lock ───────────────────────────────────────────────────────
function HiRes_Lock() {
  return (
    <div className="scr hires" style={{ background: '#050608' }}>
      <div style={{ position: 'absolute', inset: 0, display: 'flex', flexDirection: 'column', justifyContent: 'space-between', padding: '32px 28px' }}>
        {/* Top: HOLD indicator */}
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div className="mono" style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 10, letterSpacing: '.18em', color: HiResColors.gold }}>
            <IconLock size={12} /> HOLD
          </div>
          <div className="mono" style={{ fontSize: 10, opacity: .55, letterSpacing: '.14em' }}>
            78% · FLAC 24/96
          </div>
        </div>

        {/* Center: large time + small thumb */}
        <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 24 }}>
          <div className="mono" style={{ fontSize: 96, fontWeight: 200, letterSpacing: '-.02em', lineHeight: 1 }}>14:32</div>
          <div className="mono" style={{ fontSize: 11, opacity: .55, letterSpacing: '.2em' }}>
            THU · 27 MAY
          </div>

          <div style={{
            marginTop: 32,
            display: 'flex', alignItems: 'center', gap: 18,
            padding: '14px',
            background: 'rgba(255,255,255,.04)',
            border: `1px solid ${HiResColors.border}`,
            width: '100%', boxSizing: 'border-box',
          }}>
            <Art kind={TRACKS[0].art} size={80} label={false} style={{ borderRadius: 2 }} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div className="mono" style={{ fontSize: 9, color: HiResColors.gold, letterSpacing: '.14em' }}>NOW PLAYING</div>
              <div style={{ fontSize: 16, fontWeight: 600, lineHeight: 1.1, marginTop: 4 }}>{TRACKS[0].title}</div>
              <div style={{ fontSize: 12, opacity: .75, marginTop: 2 }}>{TRACKS[0].artist}</div>
              <div className="prog" style={{ marginTop: 10 }}>
                <i style={{ '--p': '38%', background: HiResColors.gold }} />
              </div>
            </div>
          </div>
        </div>

        {/* Bottom: instructions */}
        <div className="mono" style={{ textAlign: 'center', fontSize: 10, opacity: .4, letterSpacing: '.2em' }}>
          SLIDE HOLD ▼ TO UNLOCK
        </div>
      </div>
    </div>
  );
}

// ─── Settings ───────────────────────────────────────────────────
function HiRes_Settings() {
  return (
    <div className="scr hires">
      <StatusBar badge={<span className="hires-badge">Hi-Res Audio</span>} />
      <div style={{ padding: '14px 24px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 18 }}>
          <IconBack size={18} style={{ opacity: .65 }} />
          <div style={{ fontSize: 22, fontWeight: 600 }}>Settings</div>
        </div>

        {SETTINGS.map(s => (
          <div key={s.group} style={{ marginBottom: 18 }}>
            <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HiResColors.gold, marginBottom: 8 }}>
              {s.group.toUpperCase()}
            </div>
            {s.items.map((it, ii) => (
              <div key={ii} style={{
                display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                padding: '11px 0',
                borderTop: ii === 0 ? `1px solid ${HiResColors.border}` : 'none',
                borderBottom: `1px solid ${HiResColors.border}`,
              }}>
                <span style={{ fontSize: 13, fontWeight: 500 }}>{it.label}</span>
                <span style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  {it.type === 'toggle' ? (
                    <span style={{
                      width: 32, height: 18, borderRadius: 10,
                      background: it.on ? HiResColors.gold : 'rgba(255,255,255,.12)',
                      position: 'relative',
                    }}>
                      <span style={{
                        position: 'absolute', top: 2, left: it.on ? 16 : 2,
                        width: 14, height: 14, borderRadius: '50%', background: it.on ? '#1a1612' : '#fff',
                      }} />
                    </span>
                  ) : (
                    <>
                      <span className="mono" style={{ fontSize: 11, opacity: .6 }}>{it.value}</span>
                      <IconChevron size={14} style={{ opacity: .4 }} />
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

// ─── Browse (no keyboard — hierarchical menu) ─────────────────
function HiRes_Search() {
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
    { name: 'Neil Young',         meta: '6 albums · 84 tracks',  art: 'harvest',  fmt: 'FLAC' },
    { name: 'Nicolas Jaar',       meta: '2 albums · 24 tracks',  art: 'midnight', fmt: 'FLAC' },
    { name: 'Nils Frahm',         meta: '4 albums · 38 tracks',  art: 'ferns',    fmt: 'DSD'  },
  ];
  return (
    <div className="scr hires">
      <StatusBar badge={<span className="hires-badge">Hi-Res Audio</span>} />
      <div style={{ padding: '14px 24px 0' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 14 }}>
          <IconBack size={18} style={{ opacity: .65 }} />
          <div style={{ fontSize: 22, fontWeight: 600 }}>Browse</div>
          <div className="mono" style={{ marginLeft: 'auto', fontSize: 9, opacity: .55, letterSpacing: '.14em' }}>SELECT TO DRILL DOWN</div>
        </div>

        <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HiResColors.gold, marginBottom: 8 }}>BROWSE BY</div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 6 }}>
          {cats.map(c => (
            <div key={c.name} style={{
              padding: '10px 12px',
              background: c.active ? HiResColors.gold : HiResColors.surface,
              color: c.active ? '#1a1612' : 'inherit',
              border: `1px solid ${c.active ? HiResColors.gold : HiResColors.border}`,
              borderRadius: 3,
            }}>
              <div style={{ fontSize: 13, fontWeight: 600 }}>{c.name}</div>
              <div className="mono" style={{ fontSize: 9, opacity: c.active ? .7 : .55, letterSpacing: '.06em', marginTop: 2 }}>{c.count}</div>
            </div>
          ))}
        </div>

        <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HiResColors.gold, marginTop: 18, marginBottom: 6 }}>JUMP TO</div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(9, 1fr)', gap: 3 }}>
          {letters.map(l => {
            const has = available.has(l);
            const sel = l === 'N';
            return (
              <div key={l} className="mono" style={{
                aspectRatio: '1',
                background: sel ? HiResColors.gold : (has ? HiResColors.surface : 'transparent'),
                color: sel ? '#1a1612' : (has ? 'inherit' : 'rgba(230,227,220,.25)'),
                border: `1px solid ${sel ? HiResColors.gold : HiResColors.border}`,
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                fontSize: 12, fontWeight: sel ? 700 : 500,
                borderRadius: 2,
              }}>{l}</div>
            );
          })}
        </div>

        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginTop: 18, marginBottom: 6 }}>
          <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HiResColors.gold }}>N · 3 ARTISTS</div>
          <div className="mono" style={{ fontSize: 9, opacity: .5 }}>HI-RES ONLY ON</div>
        </div>
        {artists.map((a, i) => (
          <div key={a.name} style={{
            display: 'grid', gridTemplateColumns: '44px 1fr 50px 14px',
            gap: 12, alignItems: 'center', padding: '10px 0',
            borderTop: i === 0 ? `1px solid ${HiResColors.border}` : 'none',
            borderBottom: `1px solid ${HiResColors.border}`,
          }}>
            <Art kind={a.art} size={44} label={false} style={{ borderRadius: 2 }} />
            <div>
              <div style={{ fontSize: 14, fontWeight: 500 }}>{a.name}</div>
              <div className="mono" style={{ fontSize: 9, opacity: .55, letterSpacing: '.06em' }}>{a.meta}</div>
            </div>
            <span className="mono" style={{ fontSize: 9, opacity: .55, color: a.fmt === 'DSD' ? HiResColors.gold : 'inherit', textAlign: 'right' }}>{a.fmt}</span>
            <IconChevron size={12} style={{ opacity: .4 }} />
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Equalizer ──────────────────────────────────────────────────
function HiRes_EQ() {
  return (
    <div className="scr hires">
      <StatusBar badge={<span className="hires-badge">Hi-Res Audio</span>} />
      <div style={{ padding: '14px 24px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <IconBack size={18} style={{ opacity: .65 }} />
          <div style={{ fontSize: 22, fontWeight: 600 }}>Equalizer</div>
          <div className="mono" style={{ marginLeft: 'auto', fontSize: 10, letterSpacing: '.14em', color: HiResColors.gold }}>CUSTOM · A1</div>
        </div>

        {/* Presets */}
        <div className="mono" style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 14, fontSize: 10, letterSpacing: '.1em' }}>
          {['Off', 'A1', 'A2', 'A3', 'Heavy', 'Pop', 'Jazz', 'Vocal', 'Custom'].map((p, i) => (
            <div key={p} style={{
              padding: '6px 10px',
              border: `1px solid ${i === 1 ? HiResColors.gold : HiResColors.border}`,
              color: i === 1 ? HiResColors.gold : 'inherit',
              opacity: i === 1 ? 1 : .7,
            }}>{p.toUpperCase()}</div>
          ))}
        </div>

        {/* Band sliders */}
        <div style={{ marginTop: 26, height: 420, display: 'flex', position: 'relative', paddingLeft: 30, paddingBottom: 28 }}>
          {/* dB grid */}
          <div style={{ position: 'absolute', inset: '0 0 28px 0', pointerEvents: 'none' }}>
            {[+10, +5, 0, -5, -10].map((db, i) => (
              <div key={db} style={{ position: 'absolute', left: 0, right: 0, top: `${i * 25}%`, display: 'flex', alignItems: 'center' }}>
                <span className="mono" style={{ fontSize: 9, opacity: .45, width: 26, letterSpacing: '.08em' }}>
                  {db > 0 ? '+' : ''}{db}
                </span>
                <div style={{ flex: 1, height: 1, background: HiResColors.border, opacity: db === 0 ? 1 : .5 }} />
              </div>
            ))}
          </div>
          {EQ_BANDS.map(b => {
            const pct = (b.db + 12) / 24 * 100;
            return (
              <div key={b.hz} style={{ flex: 1, height: '100%', position: 'relative' }}>
                <div style={{ position: 'absolute', top: 0, bottom: 28, width: 2, background: HiResColors.border, left: '50%', transform: 'translateX(-50%)' }} />
                <div style={{
                  position: 'absolute',
                  bottom: `calc(28px + ${pct}% - 8px)`,
                  width: 16, height: 16, borderRadius: '50%',
                  background: HiResColors.gold,
                  left: '50%', transform: 'translateX(-50%)',
                  boxShadow: '0 0 10px rgba(212,169,85,.5)',
                }} />
                <div className="mono" style={{ position: 'absolute', bottom: 8, left: '50%', transform: 'translateX(-50%)', fontSize: 9, opacity: .65 }}>
                  {b.hz}
                </div>
                <div className="mono" style={{ position: 'absolute', bottom: `calc(28px + ${pct}% + 10px)`, left: '50%', transform: 'translateX(-50%)', fontSize: 9, color: HiResColors.gold }}>
                  {b.db > 0 ? '+' : ''}{b.db}
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

Object.assign(window, {
  HiRes_NowPlayingHero, HiRes_NowPlayingDense, HiRes_Library,
  HiRes_Queue, HiRes_Lock, HiRes_Settings, HiRes_Search, HiRes_EQ,
});
