// Shared data + small building-blocks reused across all three directions.
// Settings, devices, codecs, sample lyrics — pulled from Sony NW-A55
// firmware terminology (lightly rewritten for clarity).

const TRACKS = [
  { id: 'k', title: 'Atlas Hands',         artist: 'Benjamin Francis Leftwich',  album: 'Last Smoke Before the Snowstorm', art: 'kind',     dur: '4:32', cur: '1:47', codec: 'FLAC', rate: '96.0 kHz', bits: 24, bitrate: 2304 },
  { id: 'h', title: 'Harvest Moon',        artist: 'Neil Young',                 album: 'Harvest Moon',                   art: 'harvest',  dur: '5:03', cur: '2:14', codec: 'FLAC', rate: '96.0 kHz', bits: 24, bitrate: 2189 },
  { id: 'm', title: 'Lighter than Air',    artist: 'Hania Rani',                 album: 'On Giacometti',                  art: 'midnight', dur: '3:48', cur: '0:51', codec: 'FLAC', rate: '96.0 kHz', bits: 24, bitrate: 2412 },
  { id: 'f', title: 'Ferns',               artist: 'Nils Frahm',                 album: 'All Encores',                    art: 'ferns',    dur: '7:21', cur: '4:09', codec: 'DSD',  rate: '5.6 MHz',  bits: 1,  bitrate: 5645 },
  { id: 'a', title: 'Atlas, Vol. II',      artist: 'Sleep Token',                album: 'Atlas',                          art: 'atlas',    dur: '6:14', cur: '3:30', codec: 'MP3',  rate: '44.1 kHz', bits: 16, bitrate: 256 },
  { id: 'b', title: 'Bloom & Decay',       artist: 'Phoebe Bridgers',            album: 'Punisher',                       art: 'bloom',    dur: '4:08', cur: '0:00', codec: 'FLAC', rate: '96.0 kHz', bits: 24, bitrate: 2204 },
  { id: 'p', title: 'Prism',               artist: 'Cosmic Analog Ensemble',     album: 'Mirrors',                        art: 'prism',    dur: '3:12', cur: '0:00', codec: 'DSD',  rate: '5.6 MHz',  bits: 1,  bitrate: 5645 },
  { id: 'c', title: 'Cassette Romance',    artist: 'Sun June',                   album: 'Bad Dream Jaguar',               art: 'cassette', dur: '3:42', cur: '0:00', codec: 'FLAC', rate: '44.1 kHz', bits: 16, bitrate: 941  },
  { id: 'd', title: 'Static Light',        artist: 'Boards of Canada',           album: 'Tomorrow\u2019s Harvest',        art: 'static',   dur: '4:48', cur: '0:00', codec: 'FLAC', rate: '96.0 kHz', bits: 24, bitrate: 2362 },
  { id: 'l', title: 'Halcyon Days',        artist: 'Khruangbin',                 album: 'A LA SALA',                      art: 'halcyon',  dur: '3:56', cur: '0:00', codec: 'FLAC', rate: '96.0 kHz', bits: 24, bitrate: 2189 },
];
// Convenience legacy shape used by earlier code paths.
TRACKS.forEach(t => { t.n = t.codec === 'DSD' ? `DSD · ${t.rate}` : t.codec === 'MP3' ? `${t.bitrate}kbps · MP3` : `${t.codec} · ${t.bits}bit/${t.rate}`; });

const ALBUMS = [
  { title: 'Ignorance',                 artist: 'The Weather Station',         art: 'midnight', yr: '2021', fmt: 'FLAC' },
  { title: 'Harvest Moon',              artist: 'Neil Young',                  art: 'harvest',  yr: '1992', fmt: 'FLAC' },
  { title: 'Last Smoke Before…',        artist: 'B. F. Leftwich',              art: 'kind',     yr: '2011', fmt: 'FLAC' },
  { title: 'All Encores',               artist: 'Nils Frahm',                  art: 'ferns',    yr: '2019', fmt: 'DSD'  },
  { title: 'Punisher',                  artist: 'Phoebe Bridgers',             art: 'bloom',    yr: '2020', fmt: 'FLAC' },
  { title: 'Tomorrow\u2019s Harvest',   artist: 'Boards of Canada',            art: 'static',   yr: '2013', fmt: 'FLAC' },
  { title: 'A LA SALA',                 artist: 'Khruangbin',                  art: 'halcyon',  yr: '2024', fmt: 'FLAC' },
  { title: 'Mirrors',                   artist: 'Cosmic Analog Ens.',          art: 'prism',    yr: '2023', fmt: 'DSD'  },
  { title: 'Atlas',                     artist: 'Sleep Token',                 art: 'atlas',    yr: '2023', fmt: 'MP3'  },
  { title: 'Bad Dream Jaguar',          artist: 'Sun June',                    art: 'cassette', yr: '2023', fmt: 'FLAC' },
];

const EQ_BANDS = [
  { hz: '32',  db:  +2 },
  { hz: '64',  db:  +3 },
  { hz: '125', db:  +1 },
  { hz: '250', db:   0 },
  { hz: '500', db:  -1 },
  { hz: '1k',  db:   0 },
  { hz: '2k',  db:  +2 },
  { hz: '4k',  db:  +3 },
  { hz: '8k',  db:  +2 },
  { hz: '16k', db:  +1 },
];

// Sony firmware copy — Sound Settings (the "Sound Settings" root menu).
// Source: HgrmMediaPlayerApp v1.02 string table, lightly rewritten.
const SOUND_SETTINGS = [
  { group: 'EQ / Tone', items: [
    { label: 'Equalizer',          value: 'Custom A1',       type: 'nav' },
    { label: 'Tone Control',       value: 'Off',             type: 'nav' },
    { label: 'DSEE HX',            value: 'Standard',        type: 'nav' },
  ]},
  { group: 'Surround / Source', items: [
    { label: 'VPT (Surround)',     value: 'Studio',          type: 'nav' },
    { label: 'Dynamic Normalizer', value: 'Off',             type: 'toggle', on: false },
    { label: 'Vinyl Processor',    value: 'Standard',        type: 'nav' },
  ]},
  { group: 'Analog', items: [
    { label: 'DC Phase Linearizer',value: 'Type A · Low',    type: 'nav' },
    { label: 'L/R Balance',        value: 'Center',          type: 'nav' },
  ]},
  { group: 'Preset', items: [
    { label: 'Save Sound Preset',  value: '',                type: 'action' },
  ]},
];

// Sony firmware copy — Settings root.
const SETTINGS = [
  { group: 'Sound',     items: [
    { label: 'Sound Settings',     value: 'Custom A1 · DSEE HX', type: 'nav' },
    { label: 'High Gain Output',   value: 'Off',                  type: 'toggle', on: false },
    { label: 'DSD Playback',       value: 'DoP',                  type: 'nav' },
  ]},
  { group: 'Output',    items: [
    { label: 'Headphone Output',   value: '3.5 mm Stereo',        type: 'nav' },
    { label: 'Bluetooth',          value: 'WH-1000XM5',           type: 'nav' },
    { label: 'BT Receiver',        value: 'Off',                  type: 'toggle', on: false },
    { label: 'USB Connection',     value: 'Auto · USB-DAC',       type: 'nav' },
  ]},
  { group: 'System',    items: [
    { label: 'Screen Brightness',  value: '7 / 10',               type: 'nav' },
    { label: 'Screen Off Timer',   value: '30 sec',               type: 'nav' },
    { label: 'Auto Power Off',     value: '3 min',                type: 'nav' },
    { label: 'Hold Switch',        value: 'Screen + Keys',        type: 'nav' },
    { label: 'Date & Time',        value: '24-Hour · YYYY-MM-DD', type: 'nav' },
    { label: 'Language',           value: 'English',              type: 'nav' },
  ]},
  { group: 'Storage',   items: [
    { label: 'SD Card',            value: 'Mounted · 122 GB',     type: 'nav' },
    { label: 'Rebuild Database',   value: '',                     type: 'nav' },
    { label: 'Reset / Format',     value: '',                     type: 'nav' },
  ]},
  { group: 'About',     items: [
    { label: 'Device Information', value: 'NW-A55 · v1.02',       type: 'nav' },
  ]},
];

// Reset/Format hub items.
const RESET_ITEMS = [
  { label: 'Reset All Settings',           desc: 'Restore every setting to its default. Library and content are preserved.' },
  { label: 'Format System Storage',        desc: 'Erase all music and data on internal storage. This cannot be undone.', destructive: true },
  { label: 'Format SD Card',               desc: 'Erase the inserted SD card. This cannot be undone.', destructive: true },
  { label: 'Rebuild Database',             desc: 'Scan all media and rebuild the library index. Playback unavailable while running.' },
  { label: 'Restore to Factory Config',    desc: 'Return the player to its as-shipped state. Resets settings and reinstalls the original layout.', destructive: true },
];

// Bluetooth — known devices for the pair list.
const BT_DEVICES = [
  { name: 'WH-1000XM5',          kind: 'Headphones',    codec: 'LDAC',    rssi: 4, paired: true,  connected: true  },
  { name: 'WF-1000XM4',          kind: 'Earbuds',       codec: 'LDAC',    rssi: 3, paired: true,  connected: false },
  { name: 'SRS-XB23',            kind: 'Speaker',       codec: 'SBC',     rssi: 2, paired: true,  connected: false },
  { name: 'Audio-Technica M50x', kind: 'Headphones',    codec: 'AAC',     rssi: 4, paired: false, connected: false },
  { name: 'Kitchen Sonos',       kind: 'Speaker',       codec: 'aptX HD', rssi: 1, paired: false, connected: false },
];

// LDAC quality options (Sony's "Wireless Playback Quality" picker).
const LDAC_QUALITY = [
  { label: 'Auto',           sub: 'Adapts bitrate to connection strength.',     bitrate: 'auto'    },
  { label: 'Sound Quality',  sub: '990 kbps · prioritizes audio fidelity.',     bitrate: '990'    , selected: true },
  { label: 'Standard',       sub: '660 kbps · balanced.',                       bitrate: '660'    },
  { label: 'Connection',     sub: '330 kbps · stable on busy 2.4 GHz bands.',   bitrate: '330'    },
];

// Setup wizard steps.
const WIZARD_STEPS = [
  { n: 1, key: 'language', label: 'Language',          done: true  },
  { n: 2, key: 'date',     label: 'Date & Time',       done: true  },
  { n: 3, key: 'sound',    label: 'High-Quality Sound',done: false },
  { n: 4, key: 'finish',   label: 'Finish',            done: false },
];

// Sync-lyric data (timed lyrics, like Hagoromo's SyncLyricDisp).
// Original lines — written for this mock.
const LYRICS = [
  { t: '0:32', line: 'I waited for the harbour lights to fade' },
  { t: '0:41', line: 'and the radio to settle into static' },
  { t: '0:52', line: 'before I said the thing I came to say' },
  { t: '1:04', line: 'A signal in the noise, a hand on the dial' },
  { t: '1:18', line: 'a slow turn through the night' },
  { t: '1:32', line: 'the room a quiet room', current: true },
  { t: '1:46', line: 'and the song almost over' },
  { t: '1:58', line: 'almost a memory' },
  { t: '2:12', line: 'before the silence' },
];

// Quick-access tiles for Night Mode.
const NIGHT_TILES = [
  { key: 'bt',     label: 'Bluetooth',  sub: 'WH-1000XM5'   },
  { key: 'lib',    label: 'Albums',     sub: '124 · FLAC'    },
  { key: 'queue',  label: 'Up Next',    sub: '9 tracks'      },
  { key: 'eq',     label: 'Equalizer',  sub: 'Custom A1'     },
  { key: 'vol',    label: 'Volume',     sub: '21 / 120'      },
  { key: 'bright', label: 'Brightness', sub: '1 / 10 · Low'  },
];

// Stylized placeholder for album art. Uses [data-art] background from CSS.
function Art({ kind, size = 320, label, style }) {
  return (
    <div className="art" data-art={kind} style={{ width: size, height: size, ...style }}>
      {label !== false && (
        <div className="art-label">
          <span className="l">{label?.l || ''}</span>
          <span className="r">{label?.r || ''}</span>
        </div>
      )}
    </div>
  );
}

// Status bar — pass theme-agnostic color via parent.
function StatusBar({ time = '14:32', batt = 78, badge, right }) {
  return (
    <div className="status">
      <div className="l">
        <span>{time}</span>
        {badge}
      </div>
      <div className="r">
        {right}
        <span>{batt}%</span>
        <span className="batt"><i style={{ '--p': batt + '%' }}/></span>
      </div>
    </div>
  );
}

Object.assign(window, {
  TRACKS, ALBUMS, EQ_BANDS,
  SETTINGS, SOUND_SETTINGS, RESET_ITEMS,
  BT_DEVICES, LDAC_QUALITY, WIZARD_STEPS, LYRICS, NIGHT_TILES,
  Art, StatusBar,
});

// ─── Deterministic synthetic tracklist per album (for the album page) ──
const SONG_WORDS = ['Halcyon', 'Ferns', 'Static', 'Glasshouse', 'Harbour', 'Ember', 'Tide',
  'Cassette', 'Aurora', 'Pollen', 'Drift', 'Marrow', 'Velvet', 'Cinder', 'Lantern',
  'Meridian', 'Saffron', 'Willow', 'Cobalt', 'Reverie', 'Anchor', 'Petrichor', 'Hollow',
  'Slowburn', 'Northwind', 'Vellum', 'Quartz', 'Wren', 'Sable', 'Solstice', 'Margin', 'Echo'];
function seeded(n) { const x = Math.sin(n * 12.9898) * 43758.5453; return x - Math.floor(x); }
function tracksForAlbum(idx) {
  const a = ALBUMS[idx] || ALBUMS[0];
  const count = 8 + Math.floor(seeded(idx + 1) * 5);     // 8–12 tracks
  const out = [];
  for (let i = 0; i < count; i++) {
    const w1 = SONG_WORDS[Math.floor(seeded((idx + 1) * 7 + i * 3) * SONG_WORDS.length)];
    const w2 = SONG_WORDS[Math.floor(seeded((idx + 1) * 13 + i * 5 + 1) * SONG_WORDS.length)];
    const two = seeded((idx + 2) * (i + 3)) > 0.62 && w1 !== w2;
    const title = two ? `${w1} ${w2}` : w1;
    const mins = 2 + Math.floor(seeded((idx + 3) * (i + 2)) * 5);
    const secs = Math.floor(seeded((idx + 5) * (i + 7)) * 60);
    const plays = Math.floor(seeded((idx + 9) * (i + 4)) * 900 + 30);
    out.push({ n: i + 1, title, dur: `${mins}:${String(secs).padStart(2, '0')}`, fmt: a.fmt, plays });
  }
  return out;
}
function artistsFromAlbums() {
  const map = {};
  ALBUMS.forEach(a => { (map[a.artist] = map[a.artist] || { name: a.artist, art: a.art, count: 0 }).count++; });
  return Object.values(map);
}
const PLAYLISTS = [
  { name: 'Recently Added', sub: '24 songs · updated today', art: 'midnight', icon: 'clock' },
  { name: 'Favorites',      sub: 'Songs you love',          art: 'bloom',    icon: 'heart' },
  { name: 'Hi-Res Only',    sub: 'FLAC & DSD · 18 albums',  art: 'prism',    icon: 'badge' },
  { name: 'Late Night',     sub: 'Low & slow · 31 songs',   art: 'static',   icon: 'moon' },
  { name: 'On the Move',    sub: 'Commute mix · 40 songs',  art: 'halcyon',  icon: 'shuffle' },
  { name: 'Downloaded',     sub: 'On SD card · 122 GB',     art: 'harvest',  icon: 'check' },
];

Object.assign(window, { tracksForAlbum, artistsFromAlbums, PLAYLISTS });
