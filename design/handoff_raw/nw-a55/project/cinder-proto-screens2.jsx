// ────────────────────────────────────────────────────────────────
// cinder-proto-screens2.jsx — Library (Songs / Albums / Artists /
// Playlists tabs) + Artist page. Interactive: tab switching, tap
// to play, artist drill-in. Registers: library, artist.
// ────────────────────────────────────────────────────────────────

function CTabs({ active, onTab }) {
  const c = useC(); const P = c.P;
  return (
    <div style={{ display: 'flex', gap: 22, padding: '0 22px', borderBottom: `1px solid ${P.line}`, flexShrink: 0 }}>
      {['SONGS', 'ALBUMS', 'ARTISTS', 'PLAYLISTS'].map((t) => (
        <span key={t} onClick={() => onTab(t)} style={{
          fontFamily: P.mono, fontSize: 11, letterSpacing: '.12em', paddingBottom: 11, cursor: 'pointer',
          color: t === active ? P.acc : P.faint,
          borderBottom: t === active ? `2px solid ${P.acc}` : '2px solid transparent', marginBottom: -1,
        }}>{t}</span>
      ))}
    </div>
  );
}

function CShuffleRow({ label, sub, onTap }) {
  const c = useC(); const P = c.P;
  return (
    <div onClick={onTap} style={{ display: 'flex', alignItems: 'center', gap: 14, margin: '16px 22px 4px', background: P.acc, color: P.accInk, padding: '0 16px', height: 56, flexShrink: 0, cursor: 'pointer' }}>
      <FIShuffle size={20} />
      <span style={{ flex: 1 }}>
        <span style={{ display: 'block', fontSize: 15, fontWeight: 700 }}>{label}</span>
        <span style={{ display: 'block', fontFamily: P.mono, fontSize: 9, letterSpacing: '.06em', marginTop: 2, opacity: 0.75 }}>{sub}</span>
      </span>
      <FIPlay size={18} />
    </div>
  );
}

function CArtStack({ arts }) {
  const c = useC(); const P = c.P;
  const op = P.artDim;
  if (arts.length === 1) return <div className="art" data-art={arts[0]} style={{ width: 44, height: 44, opacity: op, flexShrink: 0 }}></div>;
  return (
    <div style={{ position: 'relative', width: 54, height: 48, flexShrink: 0 }}>
      <div className="art" data-art={arts[1]} style={{ position: 'absolute', top: 0, right: 0, width: 36, height: 36, opacity: 0.55 * op }}></div>
      <div className="art" data-art={arts[0]} style={{ position: 'absolute', bottom: 0, left: 0, width: 40, height: 40, opacity: op, boxShadow: `2px -2px 0 ${P.bg}` }}></div>
    </div>
  );
}

const CAL_PLAYLISTS = [
  { n: 'Liked Songs', k: 214, art: 'bloom' },
  { n: 'Night Drives', k: 32, art: 'midnight' },
  { n: 'Acoustic Mornings', k: 48, art: 'ferns' },
  { n: 'Hi-Res Showcase', k: 26, art: 'prism' },
];

function CLibrary({ params }) {
  const c = useC(); const P = c.P;
  const [tab, setTab] = React.useState((params && params.tab) || 'SONGS');
  const playAndGo = (i) => { c.setTrackIdx(i); c.setPlaying(true); c.go('nowplaying'); };
  const shuffleAll = () => { c.setTrackIdx(Math.floor(Math.random() * CAL_SONGS.length)); c.setPlaying(true); c.go('nowplaying'); };
  const counts = { SONGS: '1,842 TRACKS', ALBUMS: '124 ALBUMS', ARTISTS: '96 ARTISTS', PLAYLISTS: '12 PLAYLISTS' };
  return (
    <React.Fragment>
      <CStatus />
      <CHeader title="Library" right={<span style={{ fontFamily: P.mono, fontSize: 10, color: P.faint, letterSpacing: '.08em' }}>{counts[tab]}</span>} />
      <CTabs active={tab} onTab={setTab} />

      {tab === 'SONGS' && (
        <React.Fragment>
          <CShuffleRow label="Shuffle all songs" sub="1,842 TRACKS · RANDOM ORDER" onTap={shuffleAll} />
          <div style={{ flex: 1, overflow: 'hidden', padding: '8px 0 0' }}>
            {CAL_SONGS.map((s, i) => {
              const now = i === c.trackIdx;
              return (
                <div key={s.t} onClick={() => playAndGo(i)} style={{ display: 'flex', alignItems: 'center', gap: 13, height: 62, padding: '0 22px', borderBottom: `1px solid ${P.line}`, cursor: 'pointer' }}>
                  <div className="art" data-art={s.art} style={{ width: 42, height: 42, opacity: P.artDim }}></div>
                  <span style={{ flex: 1, minWidth: 0 }}>
                    <span style={{ display: 'block', fontSize: 15, fontWeight: 600, color: now ? P.acc : P.ink, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{s.t}</span>
                    <span style={{ display: 'block', fontSize: 11, color: P.dim, marginTop: 2 }}>{s.a}</span>
                  </span>
                  {now && <FBars n={4} seed={3} h={14} gap={2} color={P.acc} style={{ width: 18 }} />}
                  <span style={{ fontFamily: P.mono, fontSize: 10, color: P.faint }}>{s.d}</span>
                </div>
              );
            })}
          </div>
        </React.Fragment>
      )}

      {tab === 'ALBUMS' && (
        <React.Fragment>
          <CShuffleRow label="Shuffle by album" sub="RANDOM ALBUM ORDER · TRACKS IN SEQUENCE" onTap={shuffleAll} />
          <div style={{ flex: 1, overflow: 'hidden', padding: '4px 0 0' }}>
            {CAL_ALBUM_GROUPS.map((g) => (
              <div key={g.artist}>
                <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', padding: '14px 22px 8px' }}>
                  <span onClick={() => c.go('artist')} style={{ fontFamily: P.mono, fontSize: 10, letterSpacing: '.16em', color: P.dim, cursor: 'pointer' }}>{g.artist.toUpperCase()}</span>
                  <span style={{ fontFamily: P.mono, fontSize: 9, letterSpacing: '.08em', color: P.faint }}>{g.albums.length} ALBUM{g.albums.length > 1 ? 'S' : ''}</span>
                </div>
                {g.albums.map((al) => (
                  <div key={al.n} onClick={shuffleAll} style={{ display: 'flex', alignItems: 'center', gap: 13, height: 60, padding: '0 22px', borderBottom: `1px solid ${P.line}`, cursor: 'pointer' }}>
                    <div className="art" data-art={al.art} style={{ width: 44, height: 44, opacity: P.artDim }}></div>
                    <span style={{ flex: 1, minWidth: 0 }}>
                      <span style={{ display: 'block', fontSize: 15, fontWeight: 600, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{al.n}</span>
                      <span style={{ display: 'block', fontSize: 11, color: P.dim, marginTop: 2 }}>{al.y} · {al.k} tracks</span>
                    </span>
                    <span style={{ width: 38, height: 38, display: 'flex', alignItems: 'center', justifyContent: 'center', border: `1px solid ${P.line}`, color: P.dim }}><FIShuffle size={14} /></span>
                  </div>
                ))}
              </div>
            ))}
          </div>
        </React.Fragment>
      )}

      {tab === 'ARTISTS' && (
        <React.Fragment>
          <CShuffleRow label="Shuffle by artist" sub="RANDOM ARTIST · SHUFFLED WITHIN ARTIST" onTap={shuffleAll} />
          <div style={{ flex: 1, overflow: 'hidden', padding: '8px 0 0' }}>
            {CAL_ARTISTS.map((ar) => (
              <div key={ar.n} onClick={() => c.go('artist')} style={{ display: 'flex', alignItems: 'center', gap: 13, height: 64, padding: '0 22px', borderBottom: `1px solid ${P.line}`, cursor: 'pointer' }}>
                <CArtStack arts={ar.arts} />
                <span style={{ flex: 1, minWidth: 0 }}>
                  <span style={{ display: 'block', fontSize: 15, fontWeight: 600, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{ar.n}</span>
                  <span style={{ display: 'block', fontSize: 11, color: P.dim, marginTop: 2 }}>{ar.al} albums · {ar.tr} tracks</span>
                </span>
                <span style={{ width: 40, height: 40, display: 'flex', alignItems: 'center', justifyContent: 'center', border: `1px solid ${P.line}`, color: P.dim }}><FIShuffle size={15} /></span>
              </div>
            ))}
          </div>
        </React.Fragment>
      )}

      {tab === 'PLAYLISTS' && (
        <React.Fragment>
          <CShuffleRow label="Shuffle a playlist" sub="RANDOM PLAYLIST · SHUFFLED" onTap={shuffleAll} />
          <div style={{ flex: 1, overflow: 'hidden', padding: '8px 0 0' }}>
            {CAL_PLAYLISTS.map((pl) => (
              <div key={pl.n} onClick={shuffleAll} style={{ display: 'flex', alignItems: 'center', gap: 13, height: 64, padding: '0 22px', borderBottom: `1px solid ${P.line}`, cursor: 'pointer' }}>
                <div className="art" data-art={pl.art} style={{ width: 44, height: 44, opacity: P.artDim }}></div>
                <span style={{ flex: 1, minWidth: 0 }}>
                  <span style={{ display: 'block', fontSize: 15, fontWeight: 600 }}>{pl.n}</span>
                  <span style={{ display: 'block', fontSize: 11, color: P.dim, marginTop: 2 }}>{pl.k} tracks</span>
                </span>
                <span style={{ color: P.faint }}><FIChev /></span>
              </div>
            ))}
          </div>
        </React.Fragment>
      )}
    </React.Fragment>
  );
}

function CArtist() {
  const c = useC(); const P = c.P;
  const playAndGo = () => { c.setPlaying(true); c.go('nowplaying'); };
  return (
    <React.Fragment>
      <CStatus />
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '14px 22px 0' }}>
        <span onClick={c.back} style={{ color: P.dim, cursor: 'pointer' }}><FIBack /></span>
        <span style={{ fontFamily: P.mono, fontSize: 9, letterSpacing: '.2em', color: P.faint }}>ARTIST</span>
      </div>
      <div style={{ padding: '8px 22px 0' }}>
        <div style={{ fontSize: 28, fontWeight: 800, lineHeight: 1.1 }}>{CAL_BFL.name}</div>
        <div style={{ fontFamily: P.mono, fontSize: 10, letterSpacing: '.1em', color: P.dim, marginTop: 7 }}>{CAL_BFL.stats}</div>
      </div>
      <CShuffleRow label="Shuffle artist" sub="ALL 34 TRACKS · RANDOM ORDER" onTap={playAndGo} />
      <div style={{ padding: '20px 22px 9px', fontFamily: P.mono, fontSize: 9, letterSpacing: '.18em', color: P.faint }}>ALBUMS · 3</div>
      <div style={{ display: 'flex', gap: 12, padding: '0 22px' }}>
        {CAL_BFL.albums.map((al) => (
          <div key={al.n} onClick={playAndGo} style={{ flex: 1, minWidth: 0, cursor: 'pointer' }}>
            <div className="art" data-art={al.art} style={{ width: '100%', aspectRatio: '1', opacity: P.artDim }}></div>
            <div style={{ fontSize: 12, fontWeight: 600, marginTop: 7, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{al.n}</div>
            <div style={{ fontFamily: P.mono, fontSize: 9, color: P.faint, marginTop: 3 }}>{al.y}</div>
          </div>
        ))}
      </div>
      <div style={{ padding: '22px 22px 6px', fontFamily: P.mono, fontSize: 9, letterSpacing: '.18em', color: P.faint }}>TOP SONGS</div>
      <div style={{ flex: 1, overflow: 'hidden' }}>
        {CAL_BFL.top.map((s, i) => {
          const now = s.now && c.trackIdx === 0;
          return (
            <div key={s.t} onClick={() => { c.setTrackIdx(0); c.setPlaying(true); c.go('nowplaying'); }} style={{ display: 'flex', alignItems: 'center', gap: 13, height: 54, padding: '0 22px', borderBottom: `1px solid ${P.line}`, cursor: 'pointer' }}>
              <span style={{ fontFamily: P.mono, fontSize: 11, color: now ? P.acc : P.faint, width: 16 }}>{i + 1}</span>
              <span style={{ flex: 1, minWidth: 0 }}>
                <span style={{ display: 'block', fontSize: 14, fontWeight: 600, color: now ? P.acc : P.ink, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{s.t}</span>
                <span style={{ display: 'block', fontSize: 10, color: P.dim, marginTop: 2 }}>{s.al}</span>
              </span>
              {now && <FBars n={4} seed={3} h={13} gap={2} color={P.acc} style={{ width: 17 }} />}
              <span style={{ fontFamily: P.mono, fontSize: 10, color: P.faint }}>{s.d}</span>
            </div>
          );
        })}
      </div>
    </React.Fragment>
  );
}

Object.assign(window.CSCREENS, { library: CLibrary, artist: CArtist });
Object.assign(window, { CTabs, CShuffleRow });
