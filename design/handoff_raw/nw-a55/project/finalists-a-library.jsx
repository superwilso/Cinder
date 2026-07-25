// ────────────────────────────────────────────────────────────────
// finalists-a-library.jsx — Cinder Library screens.
// One Library with tabs (Songs / Albums / Artists / Playlists).
// Every tab gets a scope-aware shuffle row at the top:
//   Songs   → shuffle all 1,842 songs
//   Albums  → album shuffle: random album order, tracks in order
//   Artists → artist shuffle: random artist, then shuffle within
// Per-row shuffle glyphs let you shuffle one album / one artist.
// ────────────────────────────────────────────────────────────────

const CAL_SONGS = [
  { t: 'Atlas Hands', a: 'Benjamin Francis Leftwich', d: '4:32', art: 'kind', now: true },
  { t: 'Box of Stones', a: 'Benjamin Francis Leftwich', d: '3:58', art: 'kind' },
  { t: 'Harvest Moon', a: 'Cold Stone & Sea', d: '5:03', art: 'harvest' },
  { t: 'Midnight Arcade', a: 'Neon Cartography', d: '4:11', art: 'midnight' },
  { t: 'Ferns', a: 'Hollow Pines', d: '3:24', art: 'ferns' },
  { t: 'Halcyon Days', a: 'Vesper Lane', d: '4:47', art: 'halcyon' },
  { t: 'Bloom', a: 'Petal & Wire', d: '3:36', art: 'bloom' },
  { t: 'Prism Break', a: 'Glass Atlas', d: '4:02', art: 'prism' },
];

// Albums grouped by artist (Apple Music-style sections).
const CAL_ALBUM_GROUPS = [
  { artist: 'Benjamin Francis Leftwich', albums: [
    { n: 'Last Smoke Before the Snowstorm', k: 12, y: '2011', art: 'kind' },
    { n: 'After the Rain', k: 10, y: '2016', art: 'atlas' },
  ]},
  { artist: 'Cold Stone & Sea', albums: [
    { n: 'Harvest Moon', k: 10, y: '2019', art: 'harvest' },
    { n: 'Static Lines', k: 9, y: '2022', art: 'static' },
  ]},
  { artist: 'Glass Atlas', albums: [
    { n: 'Prism Break', k: 10, y: '2021', art: 'prism' },
  ]},
  { artist: 'Neon Cartography', albums: [
    { n: 'Midnight Arcade', k: 11, y: '2020', art: 'midnight' },
  ]},
];

const CAL_ARTISTS = [
  { n: 'Benjamin Francis Leftwich', al: 3, tr: 34, arts: ['kind', 'atlas'] },
  { n: 'Cold Stone & Sea', al: 2, tr: 21, arts: ['harvest', 'static'] },
  { n: 'Glass Atlas', al: 1, tr: 10, arts: ['prism'] },
  { n: 'Hollow Pines', al: 2, tr: 19, arts: ['ferns', 'cassette'] },
  { n: 'Neon Cartography', al: 4, tr: 46, arts: ['midnight', 'prism'] },
  { n: 'Petal & Wire', al: 1, tr: 8, arts: ['bloom'] },
  { n: 'Vesper Lane', al: 2, tr: 26, arts: ['halcyon', 'bloom'] },
];

// Overlapping stack of an artist's album covers (no artist photos
// on-device — the artist's own art is the identity).
function CALArtStack({ arts, night }) {
  const op = night ? 0.3 : 1;
  if (arts.length === 1) {
    return <div className="art" data-art={arts[0]} style={{ width: 44, height: 44, opacity: op, flexShrink: 0 }}></div>;
  }
  return (
    <div style={{ position: 'relative', width: 54, height: 48, flexShrink: 0 }}>
      <div className="art" data-art={arts[1]} style={{ position: 'absolute', top: 0, right: 0, width: 36, height: 36, opacity: 0.55 * op }}></div>
      <div className="art" data-art={arts[0]} style={{ position: 'absolute', bottom: 0, left: 0, width: 40, height: 40, opacity: op, boxShadow: '2px -2px 0 #0d0c0b' }}></div>
    </div>
  );
}

function CALTabs({ CA, active }) {
  return (
    <div style={{ display: 'flex', gap: 22, padding: '0 22px', borderBottom: `1px solid ${CA.line}`, flexShrink: 0 }}>
      {['SONGS', 'ALBUMS', 'ARTISTS', 'PLAYLISTS'].map((t) => (
        <span key={t} style={{
          fontFamily: CA.mono, fontSize: 11, letterSpacing: '.12em', paddingBottom: 11,
          color: t === active ? CA.acc : CA.faint,
          borderBottom: t === active ? `2px solid ${CA.acc}` : '2px solid transparent',
          marginBottom: -1,
        }}>{t}</span>
      ))}
    </div>
  );
}

function CALShuffleRow({ CA, label, sub }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 14, margin: '16px 22px 4px', background: CA.acc, color: CA.accInk, padding: '0 16px', height: 56, flexShrink: 0 }}>
      <FIShuffle size={20} />
      <span style={{ flex: 1 }}>
        <span style={{ display: 'block', fontSize: 15, fontWeight: 700 }}>{label}</span>
        <span style={{ display: 'block', fontFamily: CA.mono, fontSize: 9, letterSpacing: '.06em', marginTop: 2, opacity: 0.75 }}>{sub}</span>
      </span>
      <FIPlay size={18} />
    </div>
  );
}

// ─── Library · Songs ───────────────────────────────────────────
function CALibrarySongs({ night }) {
  const CA = caPal(night);
  return (
    <CAScr CA={CA} label={`A · Library · Songs${night ? ' · Night' : ''}`}>
      <CAStatus CA={CA} night={night} />
      <CAHeader CA={CA} title="Library" right={<span style={{ fontFamily: CA.mono, fontSize: 10, color: CA.faint, letterSpacing: '.08em' }}>1,842 TRACKS</span>} />
      <CALTabs CA={CA} active="SONGS" />
      <CALShuffleRow CA={CA} label="Shuffle all songs" sub="1,842 TRACKS · RANDOM ORDER" />
      <div style={{ flex: 1, overflow: 'hidden', padding: '8px 0 0' }}>
        {CAL_SONGS.map((s) => (
          <div key={s.t} style={{ display: 'flex', alignItems: 'center', gap: 13, height: 62, padding: '0 22px', borderBottom: `1px solid ${CA.line}` }}>
            <div className="art" data-art={s.art} style={{ width: 42, height: 42, opacity: night ? 0.3 : 1 }}></div>
            <span style={{ flex: 1, minWidth: 0 }}>
              <span style={{ display: 'block', fontSize: 15, fontWeight: 600, color: s.now ? CA.acc : CA.ink, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{s.t}</span>
              <span style={{ display: 'block', fontSize: 11, color: CA.dim, marginTop: 2, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{s.a}</span>
            </span>
            {s.now && <FBars n={4} seed={3} h={14} gap={2} color={CA.acc} style={{ width: 18 }} />}
            <span style={{ fontFamily: CA.mono, fontSize: 10, color: CA.faint }}>{s.d}</span>
          </div>
        ))}
      </div>
    </CAScr>
  );
}

// ─── Library · Albums ──────────────────────────────────────────
function CALibraryAlbums({ night }) {
  const CA = caPal(night);
  return (
    <CAScr CA={CA} label={`A · Library · Albums${night ? ' · Night' : ''}`}>
      <CAStatus CA={CA} night={night} />
      <CAHeader CA={CA} title="Library" right={<span style={{ fontFamily: CA.mono, fontSize: 10, color: CA.faint, letterSpacing: '.08em' }}>124 ALBUMS</span>} />
      <CALTabs CA={CA} active="ALBUMS" />
      <CALShuffleRow CA={CA} label="Shuffle by album" sub="RANDOM ALBUM ORDER · TRACKS IN SEQUENCE" />
      <div style={{ flex: 1, overflow: 'hidden', padding: '4px 0 0' }}>
        {CAL_ALBUM_GROUPS.map((g) => (
          <div key={g.artist}>
            <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', padding: '14px 22px 8px' }}>
              <span style={{ fontFamily: CA.mono, fontSize: 10, letterSpacing: '.16em', color: CA.dim }}>{g.artist.toUpperCase()}</span>
              <span style={{ fontFamily: CA.mono, fontSize: 9, letterSpacing: '.08em', color: CA.faint }}>{g.albums.length} ALBUM{g.albums.length > 1 ? 'S' : ''}</span>
            </div>
            {g.albums.map((al) => (
              <div key={al.n} style={{ display: 'flex', alignItems: 'center', gap: 13, height: 60, padding: '0 22px', borderBottom: `1px solid ${CA.line}` }}>
                <div className="art" data-art={al.art} style={{ width: 44, height: 44, opacity: night ? 0.3 : 1 }}></div>
                <span style={{ flex: 1, minWidth: 0 }}>
                  <span style={{ display: 'block', fontSize: 15, fontWeight: 600, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{al.n}</span>
                  <span style={{ display: 'block', fontSize: 11, color: CA.dim, marginTop: 2 }}>{al.y} · {al.k} tracks</span>
                </span>
                <span style={{ width: 38, height: 38, display: 'flex', alignItems: 'center', justifyContent: 'center', border: `1px solid ${CA.line}`, color: CA.dim }}><FIShuffle size={14} /></span>
              </div>
            ))}
          </div>
        ))}
      </div>
    </CAScr>
  );
}

// ─── Library · Artists ─────────────────────────────────────────
function CALibraryArtists({ night }) {
  const CA = caPal(night);
  return (
    <CAScr CA={CA} label={`A · Library · Artists${night ? ' · Night' : ''}`}>
      <CAStatus CA={CA} night={night} />
      <CAHeader CA={CA} title="Library" right={<span style={{ fontFamily: CA.mono, fontSize: 10, color: CA.faint, letterSpacing: '.08em' }}>96 ARTISTS</span>} />
      <CALTabs CA={CA} active="ARTISTS" />
      <CALShuffleRow CA={CA} label="Shuffle by artist" sub="RANDOM ARTIST · SHUFFLED WITHIN ARTIST" />
      <div style={{ flex: 1, overflow: 'hidden', padding: '8px 0 0' }}>
        {CAL_ARTISTS.map((ar) => (
          <div key={ar.n} style={{ display: 'flex', alignItems: 'center', gap: 13, height: 64, padding: '0 22px', borderBottom: `1px solid ${CA.line}` }}>
            <CALArtStack arts={ar.arts} night={night} />
            <span style={{ flex: 1, minWidth: 0 }}>
              <span style={{ display: 'block', fontSize: 15, fontWeight: 600, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{ar.n}</span>
              <span style={{ display: 'block', fontSize: 11, color: CA.dim, marginTop: 2 }}>{ar.al} albums · {ar.tr} tracks</span>
            </span>
            <span style={{ width: 40, height: 40, display: 'flex', alignItems: 'center', justifyContent: 'center', border: `1px solid ${CA.line}`, color: CA.dim }}><FIShuffle size={15} /></span>
          </div>
        ))}
      </div>
    </CAScr>
  );
}

// ─── Artist page (opened from Artists tab) ─────────────────────
const CAL_BFL = {
  name: 'Benjamin Francis Leftwich',
  stats: '3 ALBUMS · 34 TRACKS · 2 HR 14 MIN',
  albums: [
    { n: 'Last Smoke Before the Snowstorm', y: '2011', art: 'kind' },
    { n: 'After the Rain', y: '2016', art: 'atlas' },
    { n: 'Gratitude', y: '2019', art: 'cassette' },
  ],
  top: [
    { t: 'Atlas Hands', al: 'Last Smoke Before…', d: '4:32', now: true },
    { t: 'Box of Stones', al: 'Last Smoke Before…', d: '3:58' },
    { t: 'Tilikum', al: 'After the Rain', d: '4:14' },
    { t: 'Gratitude', al: 'Gratitude', d: '3:47' },
  ],
};

function CAArtistPage({ night }) {
  const CA = caPal(night);
  return (
    <CAScr CA={CA} label={`A · Artist Page${night ? ' · Night' : ''}`}>
      <CAStatus CA={CA} night={night} />
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '14px 22px 0' }}>
        <span style={{ color: CA.dim }}><FIBack /></span>
        <span style={{ fontFamily: CA.mono, fontSize: 9, letterSpacing: '.2em', color: CA.faint }}>ARTIST</span>
      </div>
      <div style={{ padding: '8px 22px 0' }}>
        <div style={{ fontSize: 28, fontWeight: 800, letterSpacing: '-.01em', lineHeight: 1.1 }}>{CAL_BFL.name}</div>
        <div style={{ fontFamily: CA.mono, fontSize: 10, letterSpacing: '.1em', color: CA.dim, marginTop: 7 }}>{CAL_BFL.stats}</div>
      </div>
      <CALShuffleRow CA={CA} label="Shuffle artist" sub="ALL 34 TRACKS · RANDOM ORDER" />
      <div style={{ padding: '20px 22px 9px', fontFamily: CA.mono, fontSize: 9, letterSpacing: '.18em', color: CA.faint }}>ALBUMS · 3</div>
      <div style={{ display: 'flex', gap: 12, padding: '0 22px' }}>
        {CAL_BFL.albums.map((al) => (
          <div key={al.n} style={{ flex: 1, minWidth: 0 }}>
            <div className="art" data-art={al.art} style={{ width: '100%', aspectRatio: '1', opacity: night ? 0.3 : 1 }}></div>
            <div style={{ fontSize: 12, fontWeight: 600, marginTop: 7, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{al.n}</div>
            <div style={{ fontFamily: CA.mono, fontSize: 9, color: CA.faint, marginTop: 3 }}>{al.y}</div>
          </div>
        ))}
      </div>
      <div style={{ padding: '22px 22px 6px', fontFamily: CA.mono, fontSize: 9, letterSpacing: '.18em', color: CA.faint }}>TOP SONGS</div>
      <div style={{ flex: 1, overflow: 'hidden' }}>
        {CAL_BFL.top.map((s, i) => (
          <div key={s.t} style={{ display: 'flex', alignItems: 'center', gap: 13, height: 54, padding: '0 22px', borderBottom: `1px solid ${CA.line}` }}>
            <span style={{ fontFamily: CA.mono, fontSize: 11, color: s.now ? CA.acc : CA.faint, width: 16 }}>{i + 1}</span>
            <span style={{ flex: 1, minWidth: 0 }}>
              <span style={{ display: 'block', fontSize: 14, fontWeight: 600, color: s.now ? CA.acc : CA.ink, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{s.t}</span>
              <span style={{ display: 'block', fontSize: 10, color: CA.dim, marginTop: 2 }}>{s.al}</span>
            </span>
            {s.now && <FBars n={4} seed={3} h={13} gap={2} color={CA.acc} style={{ width: 17 }} />}
            <span style={{ fontFamily: CA.mono, fontSize: 10, color: CA.faint }}>{s.d}</span>
          </div>
        ))}
      </div>
    </CAScr>
  );
}

Object.assign(window, { CALibrarySongs, CALibraryAlbums, CALibraryArtists, CAArtistPage, CAL_SONGS, CAL_ALBUM_GROUPS, CAL_ARTISTS, CAL_BFL });
