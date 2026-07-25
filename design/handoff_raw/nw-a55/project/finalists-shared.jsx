// ────────────────────────────────────────────────────────────────
// finalists-shared.jsx — primitives shared by all three finalist
// directions. Everything is flat-fill / hairline only: each element
// maps to a cheap rect, line, or glyph draw in a lightweight Rust
// renderer (Slint / embedded-graphics). No blur, no shadows.
// ────────────────────────────────────────────────────────────────

const FTRX = {
  title: 'Atlas Hands', artist: 'Benjamin Francis Leftwich',
  album: 'Last Smoke Before the Snowstorm', art: 'kind',
  codec: 'FLAC', spec: '24bit / 96.0 kHz', kbps: '2,304 kbps',
  cur: '1:47', dur: '4:32', rem: '-2:45', pct: 39,
};

const FBANDS = [
  { hz: '32', db: 2 }, { hz: '64', db: 3 }, { hz: '125', db: 1 },
  { hz: '250', db: 0 }, { hz: '500', db: -1 }, { hz: '1k', db: 0 },
  { hz: '2k', db: 2 }, { hz: '4k', db: 3 }, { hz: '8k', db: 2 }, { hz: '16k', db: 1 },
];

const FMENU = [
  { icon: 'note', label: 'Now Playing', value: 'Atlas Hands · 1:47' },
  { icon: 'library', label: 'Library', value: '124 albums · 1,842 tracks' },
  { icon: 'queue', label: 'Up Next', value: '9 tracks · 41:24' },
  { icon: 'radio', label: 'FM Radio', value: '88.6 MHz' },
  { icon: 'eq', label: 'Equalizer', value: 'Custom A1' },
  { icon: 'sound', label: 'Sound Settings', value: 'DSEE HX · VPT · Vinyl' },
  { icon: 'bt', label: 'Bluetooth', value: 'WH-1000XM5 · LDAC' },
  { icon: 'usb', label: 'USB-DAC', value: 'Off' },
  { icon: 'rx', label: 'BT Receiver', value: 'Off' },
  { icon: 'settings', label: 'Settings', value: 'System · Storage · About' },
];

const FPAIRED = [
  { name: 'WF-1000XM4', kind: 'Earbuds · LDAC' },
  { name: 'SRS-XB23', kind: 'Speaker · AAC' },
  { name: 'Car · CX-30', kind: 'Car unit · SBC' },
];

// deterministic pseudo-random bar heights for the static viz strips
function fbarHeights(n, seed) {
  return Array.from({ length: n }, (_, i) =>
    0.18 + 0.82 * Math.abs(Math.sin(i * 1.93 + seed * 2.7)) );
}

function FBars({ n = 28, seed = 1, h = 34, gap = 3, color, dimColor, style }) {
  const hs = fbarHeights(n, seed);
  return (
    <div style={{ display: 'flex', alignItems: 'flex-end', gap, height: h, ...style }}>
      {hs.map((v, i) => (
        <div key={i} style={{
          flex: 1, height: Math.max(2, Math.round(v * h)),
          background: i % 4 === 0 ? color : (dimColor || color),
        }}></div>
      ))}
    </div>
  );
}

function FProg({ pct = 39, h = 3, track, fill, style }) {
  return (
    <div style={{ height: h, background: track, position: 'relative', ...style }}>
      <div style={{ position: 'absolute', top: 0, left: 0, bottom: 0, width: `${pct}%`, background: fill }}></div>
    </div>
  );
}

function FBatt({ pct = 78, style }) {
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 5, ...style }}>
      <span style={{ width: 19, height: 9, border: '1px solid currentColor', position: 'relative', display: 'inline-block' }}>
        <span style={{ display: 'block', height: '100%', width: `${pct}%`, background: 'currentColor' }}></span>
        <span style={{ position: 'absolute', right: -3, top: 2, width: 2, height: 3, background: 'currentColor' }}></span>
      </span>
    </span>
  );
}

// ───── icon set — single-color strokes, ports to glyph/path draws ─────
function fsvgWrap(size, sw, kids, fill) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24"
      fill={fill ? 'currentColor' : 'none'} stroke={fill ? 'none' : 'currentColor'}
      strokeWidth={sw} strokeLinecap="round" strokeLinejoin="round"
      style={{ display: 'block', flexShrink: 0 }}>{kids}</svg>
  );
}
function FIPlay({ size = 20 }) { return fsvgWrap(size, 0, <path d="M8 5.2 18.5 12 8 18.8Z" />, true); }
function FIPause({ size = 20 }) { return fsvgWrap(size, 0, <g><rect x="6.5" y="5" width="3.4" height="14" /><rect x="14.1" y="5" width="3.4" height="14" /></g>, true); }
function FIPrev({ size = 20, sw = 1.8 }) {
  return fsvgWrap(size, sw, <g><path d="M7 5v14" /><path d="M18 6.2 9.8 12l8.2 5.8Z" fill="currentColor" stroke="none" /></g>);
}
function FINext({ size = 20, sw = 1.8 }) {
  return fsvgWrap(size, sw, <g><path d="M17 5v14" /><path d="M6 6.2 14.2 12 6 17.8Z" fill="currentColor" stroke="none" /></g>);
}
function FIShuffle({ size = 18, sw = 1.7 }) {
  return fsvgWrap(size, sw, <g><path d="M3 6.5h3.6L17 17.5h4" /><path d="M3 17.5h3.6l2.9-3.3" /><path d="M13.8 9.6 17 6.5h4" /><path d="m18.6 4 2.6 2.5-2.6 2.5" /><path d="m18.6 15 2.6 2.5-2.6 2.5" /></g>);
}
function FIRepeat({ size = 18, sw = 1.7 }) {
  return fsvgWrap(size, sw, <g><path d="M4 13V9.8A3.3 3.3 0 0 1 7.3 6.5H20" /><path d="m17.4 3.9 2.6 2.6-2.6 2.6" /><path d="M20 11v3.2a3.3 3.3 0 0 1-3.3 3.3H4" /><path d="m6.6 14.9-2.6 2.6 2.6 2.6" /></g>);
}
function FIHeart({ size = 19, sw = 1.7, fill = false }) {
  return fsvgWrap(size, sw, <path d="M12 20.3C5.4 16 3 12.8 3 9.4 3 7 4.9 5 7.4 5c1.8 0 3.4 1 4.6 2.7C13.2 6 14.8 5 16.6 5 19.1 5 21 7 21 9.4c0 3.4-2.4 6.6-9 10.9Z" />, fill);
}
function FIQueue({ size = 19, sw = 1.7 }) {
  return fsvgWrap(size, sw, <g><path d="M4 6h16" /><path d="M4 12h10" /><path d="M4 18h7" /><path d="M16.5 14.8l4.5 2.7-4.5 2.7Z" fill="currentColor" stroke="none" /></g>);
}
function FIEq({ size = 19, sw = 1.7 }) {
  return fsvgWrap(size, sw, <g><path d="M6 4v16" /><path d="M12 4v16" /><path d="M18 4v16" /><path d="M3.6 14.2h4.8" /><path d="M9.6 8.2h4.8" /><path d="M15.6 16.2h4.8" /></g>);
}
function FIBt({ size = 18, sw = 1.7 }) {
  return fsvgWrap(size, sw, <path d="M6 7.2 17 16.8 11.5 21.5V2.5L17 7.2 6 16.8" />);
}
function FIRx({ size = 18, sw = 1.7 }) {
  return fsvgWrap(size, sw, <g><path d="M9 7.6 18 15.4 13.5 19.2V4.8L18 8.6 9 16.4" /><path d="M3 9v6" /><path d="M5.6 7v10" /></g>);
}
function FIUsb({ size = 19, sw = 1.6 }) {
  return fsvgWrap(size, sw, <g><path d="M12 21V4.6" /><path d="M9.8 6.8 12 4l2.2 2.8" /><path d="M12 14.5 7.5 12V9.4" /><path d="M12 12l4.5-2V7.2" /><circle cx="7.5" cy="8" r="1.3" /><rect x="15.4" y="5" width="2.4" height="2.4" /><circle cx="12" cy="19" r="1.6" fill="currentColor" stroke="none" /></g>);
}
function FIRadio({ size = 19, sw = 1.6 }) {
  return fsvgWrap(size, sw, <g><rect x="3" y="8.5" width="18" height="11" rx="1" /><circle cx="8.2" cy="14" r="2.4" /><path d="M14 12.2h4" /><path d="M14 15.8h4" /><path d="M7 8.5 17.5 3.4" /></g>);
}
function FISettings({ size = 19, sw = 1.6 }) {
  return fsvgWrap(size, sw, <g><circle cx="12" cy="12" r="3.1" /><path d="M12 2.5v3" /><path d="M12 18.5v3" /><path d="M2.5 12h3" /><path d="M18.5 12h3" /><path d="m5.3 5.3 2.1 2.1" /><path d="m16.6 16.6 2.1 2.1" /><path d="M18.7 5.3 16.6 7.4" /><path d="m7.4 16.6-2.1 2.1" /></g>);
}
function FILibrary({ size = 19, sw = 1.6 }) {
  return fsvgWrap(size, sw, <g><rect x="4" y="4" width="6.6" height="6.6" /><rect x="13.4" y="4" width="6.6" height="6.6" /><rect x="4" y="13.4" width="6.6" height="6.6" /><rect x="13.4" y="13.4" width="6.6" height="6.6" /></g>);
}
function FINote({ size = 19, sw = 1.7 }) {
  return fsvgWrap(size, sw, <g><path d="M9 17.5V5l11-2.2V15" /><circle cx="6.5" cy="17.5" r="2.6" /><circle cx="17.5" cy="15" r="2.6" /></g>);
}
function FISound({ size = 19, sw = 1.6 }) {
  return fsvgWrap(size, sw, <g><path d="M4 9.5v5h3.4L13 19.2V4.8L7.4 9.5H4Z" /><path d="M16 9.2a4.2 4.2 0 0 1 0 5.6" /><path d="M18.6 6.6a8 8 0 0 1 0 10.8" /></g>);
}
function FIChev({ size = 16, sw = 1.8 }) { return fsvgWrap(size, sw, <path d="m9 5 7 7-7 7" />); }
function FIBack({ size = 20, sw = 1.8 }) { return fsvgWrap(size, sw, <path d="m15 5-7 7 7 7" />); }
function FILock({ size = 15, sw = 1.7 }) {
  return fsvgWrap(size, sw, <g><rect x="5" y="10.5" width="14" height="9.5" rx="1" /><path d="M8 10.5V7.5a4 4 0 0 1 8 0v3" /></g>);
}
function FIBookmark({ size = 16, sw = 1.7 }) {
  return fsvgWrap(size, sw, <path d="M6.5 3.5h11V21L12 16.8 6.5 21Z" />);
}
function FINfc({ size = 17, sw = 1.7 }) {
  return fsvgWrap(size, sw, <g><path d="M8.6 8.6a4.8 4.8 0 0 1 6.8 6.8" /><path d="M5.8 5.8a8.8 8.8 0 0 1 12.4 12.4" /><circle cx="7" cy="17" r="1.5" fill="currentColor" stroke="none" /></g>);
}

const FICONS = {
  note: FINote, library: FILibrary, queue: FIQueue, radio: FIRadio,
  eq: FIEq, sound: FISound, bt: FIBt, usb: FIUsb, rx: FIRx, settings: FISettings,
};

Object.assign(window, {
  FTRX, FBANDS, FMENU, FPAIRED, FICONS, fbarHeights,
  FBars, FProg, FBatt,
  FIPlay, FIPause, FIPrev, FINext, FIShuffle, FIRepeat, FIHeart, FIQueue,
  FIEq, FIBt, FIRx, FIUsb, FIRadio, FISettings, FILibrary, FINote, FISound,
  FIChev, FIBack, FILock, FIBookmark, FINfc,
});
