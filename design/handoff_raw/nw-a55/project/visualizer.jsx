// ────────────────────────────────────────────────────────────────
// visualizer.jsx
// A synthetic, music-like spectrum engine + a library of live
// canvas visualizers. No real audio — we synthesize believable
// FFT data (bass-heavy 1/f falloff + beat kick + per-bin LFOs).
//
//   VizEngine            — single RAF loop, broadcasts frames.
//   <VizCanvas kind … /> — themeable live canvas for any style.
//   VIZ_KINDS            — list of available visualizer styles.
//   <LiveProgress …/>    — a progress bar that advances in real time.
// ────────────────────────────────────────────────────────────────

const VizEngine = (() => {
  const N = 64;
  const bins   = new Float32Array(N);   // smoothed display values 0..1
  const targets= new Float32Array(N);
  let energy = 0;                        // overall loudness 0..1
  let gate = 1;                          // playing gate (0 paused → 1 playing)
  let gateTarget = 1;
  let beatEnv = 0, beatPhase = 0;
  const BPM = 122;
  const subs = new Set();
  let raf = null, last = 0, t0 = 0;

  function frame(now) {
    try {
      if (!t0) { t0 = now; last = now; }
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      const t = (now - t0) / 1000;

      // gate easing (pause → silence)
      gate += (gateTarget - gate) * Math.min(1, dt * 6);

      // beat kick
      beatPhase += dt * (BPM / 60);
      if (beatPhase >= 1) { beatPhase -= 1; beatEnv = 1; }
      beatEnv *= Math.pow(0.0008, dt); // fast decay

      let sum = 0;
      for (let i = 0; i < N; i++) {
        const fr = i / (N - 1);
        // bass-heavy base envelope
        let base = Math.pow(1 - fr, 1.7) * 0.62 + 0.06;
        // moving spectral content
        const o1 = 0.5 + 0.5 * Math.sin(t * (1.4 + fr * 7.0) + i * 0.55);
        const o2 = 0.5 + 0.5 * Math.sin(t * (0.6 + fr * 3.0) - i * 0.31 + 1.3);
        let v = base * (0.38 + 0.62 * o1 * o2);
        // beat kick adds energy to lows + a little broadband
        v += beatEnv * (Math.max(0, 1 - fr * 2.6) * 0.55 + 0.05);
        // sparkle in highs
        if (fr > 0.6) v += Math.random() * 0.05 * o1;
        v *= gate;
        v = Math.max(0, Math.min(1, v));
        targets[i] = v;
        // attack fast, decay slow
        const k = v > bins[i] ? 0.55 : 0.10;
        bins[i] += (v - bins[i]) * k;
        sum += bins[i];
      }
      energy += ((sum / N) - energy) * 0.2;

      const state = { t, dt, bins, energy, gate, beat: beatEnv };
      subs.forEach(fn => { try { fn(state); } catch (e) {} });
    } catch (e) {
      /* never let the loop die */
    } finally {
      raf = requestAnimationFrame(frame);
    }
  }

  function ensure() { if (raf == null) raf = requestAnimationFrame(frame); }

  // restart promptly when the tab/iframe becomes visible again (rAF is
  // throttled/paused while hidden, which can leave canvases blank)
  if (typeof document !== 'undefined') {
    document.addEventListener('visibilitychange', () => { if (!document.hidden) { last = performance.now(); ensure(); } });
  }

  return {
    subscribe(fn) { subs.add(fn); ensure(); return () => subs.delete(fn); },
    setPlaying(p) { gateTarget = p ? 1 : 0; ensure(); },
    get bins() { return bins; },
    get energy() { return energy; },
  };
})();

// downsample N engine bins → M display bars
function sample(bins, m) {
  const out = new Array(m);
  const step = bins.length / m;
  for (let i = 0; i < m; i++) {
    let s = 0, c = 0;
    for (let j = Math.floor(i * step); j < Math.floor((i + 1) * step); j++) { s += bins[j]; c++; }
    out[i] = c ? s / c : 0;
  }
  return out;
}

// ─── per-kind draw functions ────────────────────────────────────
// Each: (ctx, w, h, state, P, store) — P = palette, store = per-instance scratch.

function hexA(hex, a) {
  // hex (#rrggbb) → rgba string
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
}

const VIZ_DRAW = {
  // classic bottom-anchored frequency bars
  bars(ctx, w, h, st, P, store) {
    const M = 40, bw = w / M, gap = bw * 0.32;
    const data = sample(st.bins, M);
    for (let i = 0; i < M; i++) {
      const bh = Math.max(2, data[i] * h);
      const x = i * bw, hot = data[i] > 0.82;
      const g = ctx.createLinearGradient(0, h, 0, h - bh);
      g.addColorStop(0, hexA(P.accent, 0.35));
      g.addColorStop(1, hot ? P.hot : P.accent);
      ctx.fillStyle = g;
      ctx.fillRect(x + gap / 2, h - bh, bw - gap, bh);
    }
  },

  // mirrored spectrum (center line)
  mirror(ctx, w, h, st, P, store) {
    const M = 48, bw = w / M, gap = bw * 0.3, mid = h / 2;
    const data = sample(st.bins, M);
    for (let i = 0; i < M; i++) {
      const bh = Math.max(1, data[i] * (h / 2) * 0.95);
      const x = i * bw + gap / 2, ww = bw - gap;
      ctx.fillStyle = data[i] > 0.82 ? P.hot : P.accent;
      ctx.globalAlpha = 0.95;
      ctx.fillRect(x, mid - bh, ww, bh);
      ctx.globalAlpha = 0.35;
      ctx.fillRect(x, mid, ww, bh);
    }
    ctx.globalAlpha = 1;
  },

  // oscilloscope — sum of harmonics weighted by low bins
  scope(ctx, w, h, st, P, store) {
    const mid = h / 2, amp = h * 0.42;
    const b = st.bins;
    ctx.beginPath();
    for (let px = 0; px <= w; px += 2) {
      const u = px / w;
      const ang = u * Math.PI * 2;
      let y = 0;
      y += b[2] * Math.sin(ang * 1 + st.t * 2.0);
      y += b[6] * Math.sin(ang * 2 + st.t * 1.3) * 0.7;
      y += b[12] * Math.sin(ang * 3 - st.t * 1.7) * 0.5;
      y += b[20] * Math.sin(ang * 5 + st.t * 2.4) * 0.3;
      const yy = mid - y * amp;
      px === 0 ? ctx.moveTo(px, yy) : ctx.lineTo(px, yy);
    }
    ctx.strokeStyle = P.accent;
    ctx.lineWidth = 1.6;
    ctx.shadowColor = hexA(P.accent, 0.7);
    ctx.shadowBlur = 6;
    ctx.stroke();
    ctx.shadowBlur = 0;
  },

  // filled smooth spectrum area
  area(ctx, w, h, st, P, store) {
    const M = 28, data = sample(st.bins, M);
    const xs = i => (i / (M - 1)) * w;
    const ys = i => h - Math.max(2, data[i] * h * 0.96);
    ctx.beginPath();
    ctx.moveTo(0, h);
    ctx.lineTo(xs(0), ys(0));
    for (let i = 0; i < M - 1; i++) {
      const xc = (xs(i) + xs(i + 1)) / 2, yc = (ys(i) + ys(i + 1)) / 2;
      ctx.quadraticCurveTo(xs(i), ys(i), xc, yc);
    }
    ctx.lineTo(xs(M - 1), ys(M - 1));
    ctx.lineTo(w, h);
    ctx.closePath();
    const g = ctx.createLinearGradient(0, 0, 0, h);
    g.addColorStop(0, hexA(P.accent, 0.85));
    g.addColorStop(1, hexA(P.accent, 0.05));
    ctx.fillStyle = g;
    ctx.fill();
    // top stroke
    ctx.beginPath();
    ctx.moveTo(xs(0), ys(0));
    for (let i = 0; i < M - 1; i++) {
      const xc = (xs(i) + xs(i + 1)) / 2, yc = (ys(i) + ys(i + 1)) / 2;
      ctx.quadraticCurveTo(xs(i), ys(i), xc, yc);
    }
    ctx.strokeStyle = P.accent; ctx.lineWidth = 1.4; ctx.stroke();
  },

  // radial bars around a center
  radial(ctx, w, h, st, P, store) {
    const cx = w / 2, cy = h / 2;
    const r0 = Math.min(w, h) * 0.20;
    const r1 = Math.min(w, h) * 0.46;
    const M = 56, data = sample(st.bins, M);
    ctx.lineWidth = Math.max(2, (Math.PI * 2 * r0) / M * 0.5);
    ctx.lineCap = 'round';
    for (let i = 0; i < M; i++) {
      const a = (i / M) * Math.PI * 2 - Math.PI / 2;
      const len = r0 + data[i] * (r1 - r0);
      ctx.beginPath();
      ctx.moveTo(cx + Math.cos(a) * r0, cy + Math.sin(a) * r0);
      ctx.lineTo(cx + Math.cos(a) * len, cy + Math.sin(a) * len);
      ctx.strokeStyle = data[i] > 0.8 ? P.hot : hexA(P.accent, 0.5 + data[i] * 0.5);
      ctx.stroke();
    }
    // inner ring
    ctx.beginPath();
    ctx.arc(cx, cy, r0 - 4, 0, Math.PI * 2);
    ctx.strokeStyle = hexA(P.accent, 0.25);
    ctx.lineWidth = 1;
    ctx.stroke();
  },

  // LED dot matrix
  dots(ctx, w, h, st, P, store) {
    const cols = 28, rows = 12;
    const data = sample(st.bins, cols);
    const cw = w / cols, ch = h / rows;
    const r = Math.min(cw, ch) * 0.3;
    for (let c = 0; c < cols; c++) {
      const lit = Math.round(data[c] * rows);
      for (let row = 0; row < rows; row++) {
        const on = row < lit;
        const fromTop = rows - 1 - row;
        const x = c * cw + cw / 2, y = fromTop * ch + ch / 2;
        ctx.beginPath();
        ctx.arc(x, y, r, 0, Math.PI * 2);
        if (on) {
          ctx.fillStyle = row >= rows - 2 ? P.hot : P.accent;
          ctx.globalAlpha = 1;
        } else {
          ctx.fillStyle = P.accent; ctx.globalAlpha = 0.10;
        }
        ctx.fill();
      }
    }
    ctx.globalAlpha = 1;
  },

  // two analog VU needles
  vu(ctx, w, h, st, P, store) {
    if (store.l == null) { store.l = 0; store.r = 0; }
    const tl = Math.min(1, st.energy * 1.5 + st.beat * 0.3);
    const tr = Math.min(1, st.energy * 1.4 + st.beat * 0.25 + 0.03);
    store.l += (tl - store.l) * 0.25;
    store.r += (tr - store.r) * 0.22;
    const draw1 = (val, x0, ww) => {
      const cx = x0 + ww / 2, cy = h * 0.92, R = Math.min(ww, h) * 0.78;
      // arc
      ctx.beginPath(); ctx.arc(cx, cy, R, Math.PI * 1.20, Math.PI * 1.80);
      ctx.strokeStyle = hexA(P.accent, 0.3); ctx.lineWidth = 1.5; ctx.stroke();
      // red zone
      ctx.beginPath(); ctx.arc(cx, cy, R, Math.PI * 1.66, Math.PI * 1.80);
      ctx.strokeStyle = P.hot; ctx.lineWidth = 2.5; ctx.stroke();
      // needle
      const ang = Math.PI * 1.20 + val * (Math.PI * 0.60);
      ctx.beginPath(); ctx.moveTo(cx, cy);
      ctx.lineTo(cx + Math.cos(ang) * R * 0.95, cy + Math.sin(ang) * R * 0.95);
      ctx.strokeStyle = P.accent; ctx.lineWidth = 2;
      ctx.shadowColor = hexA(P.accent, 0.6); ctx.shadowBlur = 5; ctx.stroke(); ctx.shadowBlur = 0;
      ctx.beginPath(); ctx.arc(cx, cy, 3, 0, Math.PI * 2); ctx.fillStyle = P.accent; ctx.fill();
    };
    draw1(store.l, 0, w / 2);
    draw1(store.r, w / 2, w / 2);
  },

  // scrolling waveform history (thin signature line)
  line(ctx, w, h, st, P, store) {
    if (!store.hist) store.hist = new Array(120).fill(0.5);
    const v = 0.5 + (st.energy * 1.4 + st.beat * 0.4 - 0.4) * (0.9);
    store.hist.push(Math.max(0.04, Math.min(0.96, v)));
    if (store.hist.length > 120) store.hist.shift();
    const H = store.hist, mid = h / 2;
    ctx.beginPath();
    for (let i = 0; i < H.length; i++) {
      const x = (i / (H.length - 1)) * w;
      const y = mid - (H[i] - 0.5) * h * 0.9;
      i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
    }
    ctx.strokeStyle = P.accent; ctx.lineWidth = 1.5;
    ctx.shadowColor = hexA(P.accent, 0.5); ctx.shadowBlur = 5; ctx.stroke(); ctx.shadowBlur = 0;
    // mirror faint
    ctx.beginPath();
    for (let i = 0; i < H.length; i++) {
      const x = (i / (H.length - 1)) * w;
      const y = mid + (H[i] - 0.5) * h * 0.9;
      i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
    }
    ctx.strokeStyle = hexA(P.accent, 0.25); ctx.lineWidth = 1; ctx.stroke();
  },
};

const VIZ_KINDS = [
  { id: 'bars',   label: 'Frequency Bars' },
  { id: 'mirror', label: 'Mirror Spectrum' },
  { id: 'area',   label: 'Filled Spectrum' },
  { id: 'scope',  label: 'Oscilloscope' },
  { id: 'line',   label: 'Waveform Line' },
  { id: 'dots',   label: 'LED Matrix' },
  { id: 'radial', label: 'Radial' },
  { id: 'vu',     label: 'Analog VU' },
];

// ─── live canvas component ──────────────────────────────────────
function VizCanvas({ kind = 'bars', palette, width, height, style }) {
  const ref = React.useRef(null);
  const store = React.useRef({});
  React.useEffect(() => {
    const cv = ref.current;
    const ctx = cv.getContext('2d');
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    cv.width = Math.round(width * dpr);
    cv.height = Math.round(height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    store.current = {};
    const fn = VIZ_DRAW[kind] || VIZ_DRAW.bars;
    const unsub = VizEngine.subscribe((st) => {
      ctx.clearRect(0, 0, width, height);
      fn(ctx, width, height, st, palette, store.current);
    });
    return unsub;
  }, [kind, width, height, palette.accent, palette.hot]);
  return <canvas ref={ref} style={{ width, height, display: 'block', ...style }} />;
}

// ─── real-time progress bar ─────────────────────────────────────
// Reads posSec/durSec from props; render-only (player owns the clock).
function fmtTime(s) {
  s = Math.max(0, Math.floor(s));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

Object.assign(window, {
  VizEngine, VizCanvas, VIZ_KINDS, VIZ_DRAW, fmtTime, hexA,
});
