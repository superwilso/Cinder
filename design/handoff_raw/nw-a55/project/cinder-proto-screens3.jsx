// ────────────────────────────────────────────────────────────────
// cinder-proto-screens3.jsx — Menu, Equalizer (interactive 10-band),
// Sound Settings (Sony DSP suite), Settings.
// Registers: menu, eq, sound, settings.
// ────────────────────────────────────────────────────────────────

function CMenu() {
  const c = useC(); const P = c.P;
  const items = [
    { icon: 'note', label: 'Now Playing', id: 'nowplaying', value: () => `${CAL_SONGS[c.trackIdx].t} · 1:47` },
    { icon: 'library', label: 'Library', id: 'library', value: () => '124 albums · 1,842 tracks' },
    { icon: 'queue', label: 'Up Next', id: 'upnext', value: () => '8 tracks · 41:24' },
    { icon: 'radio', label: 'FM Radio', id: 'fm', value: () => `${c.fm.freq.toFixed(1)} MHz` },
    { icon: 'eq', label: 'Equalizer', id: 'eq', value: () => `Custom ${c.eqPreset}` },
    { icon: 'sound', label: 'Sound Settings', id: 'sound', value: () => [c.snd.dsee && 'DSEE HX', c.snd.vpt !== 'Off' && 'VPT', c.snd.vinyl && 'Vinyl'].filter(Boolean).join(' · ') || 'All off' },
    { icon: 'bt', label: 'Bluetooth', id: 'bluetooth', value: () => (c.bt.connected ? `${c.bt.connected} · ${c.bt.codec}` : 'Off') },
    { icon: 'usb', label: 'USB-DAC', id: 'usbdac', value: () => (c.usbDac ? 'On' : 'Off') },
    { icon: 'rx', label: 'BT Receiver', id: 'receiver', value: () => (c.rx ? 'On' : 'Off') },
    { icon: 'settings', label: 'Settings', id: 'settings', value: () => 'System · Storage · About' },
  ];
  return (
    <React.Fragment>
      <CStatus />
      <CHeader title="Menu" right={<span style={{ fontFamily: P.mono, fontSize: 10, color: P.faint, letterSpacing: '.1em' }}>NW-A55</span>} />
      <div style={{ flex: 1, overflow: 'hidden' }}>
        {items.map((m, i) => {
          const Ico = FICONS[m.icon];
          return (
            <div key={m.label} onClick={() => c.go(m.id)} style={{
              display: 'flex', alignItems: 'center', gap: 16, height: 63, padding: '0 22px',
              borderTop: i === 0 ? `1px solid ${P.line}` : 'none', borderBottom: `1px solid ${P.line}`, cursor: 'pointer',
            }}>
              <span style={{ color: m.id === 'nowplaying' ? P.acc : P.dim }}><Ico /></span>
              <span style={{ fontSize: 17, fontWeight: 600, flex: 1 }}>{m.label}</span>
              <span style={{ fontFamily: P.mono, fontSize: 10, color: P.faint, letterSpacing: '.04em' }}>{m.value()}</span>
              <span style={{ color: P.faint }}><FIChev /></span>
            </div>
          );
        })}
      </div>
    </React.Fragment>
  );
}

// ─── Equalizer — draggable bands ───────────────────────────────
const EQ_PRESETS = { FLAT: [0,0,0,0,0,0,0,0,0,0], ROCK: [4,3,1,0,-1,0,2,3,4,4], JAZZ: [2,1,0,1,2,1,0,1,2,3], A1: [2,3,1,0,-1,0,2,3,2,1], A2: [5,4,2,0,0,1,1,2,4,5] };

function CEq() {
  const c = useC(); const P = c.P;
  const H = 330, range = 10;
  const setBand = (i, clientY, el) => {
    const r = el.getBoundingClientRect();
    const rel = (clientY - r.top - 10) / (H - 20);
    const db = Math.max(-range, Math.min(range, Math.round((0.5 - rel) * 2 * range)));
    c.setEq((eq) => eq.map((v, j) => (j === i ? db : v)));
    c.setEqPreset('A1');
  };
  return (
    <React.Fragment>
      <CStatus />
      <CHeader title="Equalizer" right={<span style={{ fontFamily: P.mono, fontSize: 10, letterSpacing: '.12em', color: P.acc, border: `1px solid ${P.acc}`, padding: '4px 9px' }}>CUSTOM {c.eqPreset}</span>} />
      <div style={{ display: 'flex', gap: 8, padding: '2px 22px 20px', fontFamily: P.mono, fontSize: 10, letterSpacing: '.08em' }}>
        {Object.keys(EQ_PRESETS).map((p) => (
          <span key={p} onClick={() => { c.setEq([...EQ_PRESETS[p]]); c.setEqPreset(p); }} style={{
            padding: '7px 13px', border: `1px solid ${p === c.eqPreset ? P.acc : P.line}`, cursor: 'pointer',
            color: p === c.eqPreset ? P.accInk : P.dim, background: p === c.eqPreset ? P.acc : 'transparent', fontWeight: p === c.eqPreset ? 700 : 400,
          }}>{p}</span>
        ))}
      </div>
      <div style={{ position: 'relative', margin: '6px 26px 0', height: H + 46 }}>
        <div style={{ position: 'absolute', left: 0, right: 0, top: H / 2 + 10, borderTop: `1px dashed ${P.line}` }}></div>
        <div style={{ display: 'flex', height: '100%' }}>
          {FBANDS.map((b, i) => {
            const db = c.eq[i];
            const knobY = H / 2 - (db / range) * (H / 2 - 14);
            return (
              <div key={b.hz}
                onPointerDown={(e) => { e.currentTarget.setPointerCapture(e.pointerId); setBand(i, e.clientY, e.currentTarget); }}
                onPointerMove={(e) => { if (e.buttons) setBand(i, e.clientY, e.currentTarget); }}
                style={{ flex: 1, position: 'relative', cursor: 'ns-resize', touchAction: 'none' }}>
                <div style={{ position: 'absolute', top: -6, left: 0, right: 0, textAlign: 'center', fontFamily: P.mono, fontSize: 9, color: db !== 0 ? P.acc : P.faint }}>
                  {db > 0 ? `+${db}` : db}
                </div>
                <div style={{ position: 'absolute', top: 10, bottom: 36, left: '50%', width: 2, marginLeft: -1, background: P.line }}></div>
                <div style={{
                  position: 'absolute', left: '50%', width: 2, marginLeft: -1, background: P.acc,
                  top: 10 + Math.min(knobY, H / 2), height: Math.abs((db / range) * (H / 2 - 14)),
                }}></div>
                <div style={{
                  position: 'absolute', top: 10 + knobY - 8, left: '50%', marginLeft: -8,
                  width: 16, height: 16, borderRadius: '50%', background: P.acc, border: `3px solid ${P.bg}`,
                }}></div>
                <div style={{ position: 'absolute', bottom: 12, left: 0, right: 0, textAlign: 'center', fontFamily: P.mono, fontSize: 9, color: P.dim }}>{b.hz}</div>
              </div>
            );
          })}
        </div>
      </div>
      <div style={{ marginTop: 'auto', borderTop: `1px solid ${P.line}`, height: 60, display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '0 22px', flexShrink: 0 }}>
        <span onClick={() => { c.setEq([0,0,0,0,0,0,0,0,0,0]); c.setEqPreset('FLAT'); }} style={{ fontSize: 14, fontWeight: 600, color: P.dim, cursor: 'pointer' }}>Reset</span>
        <span style={{ fontSize: 14, fontWeight: 700, color: P.acc, cursor: 'pointer' }}>Save Sound Preset</span>
      </div>
    </React.Fragment>
  );
}

// ─── Sound Settings — Sony DSP suite ───────────────────────────
function CToggle({ on, onTap }) {
  const c = useC(); const P = c.P;
  return (
    <span onClick={onTap} style={{ width: 40, height: 22, border: `1px solid ${on ? P.acc : P.line}`, position: 'relative', display: 'inline-block', cursor: 'pointer', flexShrink: 0 }}>
      <span style={{ position: 'absolute', top: 3, width: 14, height: 14, background: on ? P.acc : P.faint, left: on ? 'auto' : 3, right: on ? 3 : 'auto' }}></span>
    </span>
  );
}

function CSound() {
  const c = useC(); const P = c.P;
  const snd = c.snd;
  const set = (k, v) => c.setSnd({ ...snd, [k]: v });
  const VPTS = ['Off', 'Studio', 'Club', 'Concert Hall'];
  const DCS = ['Off', 'Standard A', 'Standard B', 'Low A', 'Low B'];
  const row = (label, desc, kids) => (
    <div style={{ display: 'flex', alignItems: 'center', gap: 14, minHeight: 64, padding: '10px 22px', borderBottom: `1px solid ${P.line}` }}>
      <span style={{ flex: 1 }}>
        <span style={{ display: 'block', fontSize: 16, fontWeight: 600 }}>{label}</span>
        <span style={{ display: 'block', fontSize: 11, color: P.dim, marginTop: 3 }}>{desc}</span>
      </span>
      {kids}
    </div>
  );
  const cycle = (k, list) => set(k, list[(list.indexOf(snd[k]) + 1) % list.length]);
  const pill = (val, onTap) => (
    <span onClick={onTap} style={{ fontFamily: P.mono, fontSize: 10, letterSpacing: '.08em', color: val === 'Off' ? P.faint : P.acc, border: `1px solid ${val === 'Off' ? P.line : P.acc}`, padding: '7px 12px', cursor: 'pointer', whiteSpace: 'nowrap' }}>{val.toUpperCase()}</span>
  );
  const dseeOff = c.bt.connected && c.bt.codec === 'LDAC';
  return (
    <React.Fragment>
      <CStatus />
      <CHeader title="Sound" right={<span style={{ fontFamily: P.mono, fontSize: 10, color: P.faint, letterSpacing: '.08em' }}>SONY DSP</span>} />
      <div style={{ flex: 1, overflow: 'hidden' }}>
        {row('DSEE HX', 'Upscale compressed audio to near hi-res', <CToggle on={snd.dsee} onTap={() => set('dsee', !snd.dsee)} />)}
        {row('Vinyl Processor', 'Tonearm resonance + surface noise character', <CToggle on={snd.vinyl} onTap={() => set('vinyl', !snd.vinyl)} />)}
        {row('VPT Surround', 'Studio / Club / Concert Hall acoustics', pill(snd.vpt, () => cycle('vpt', VPTS)))}
        {row('DC Phase Linearizer', 'Analog-amp low-frequency phase response', pill(snd.dcphase, () => cycle('dcphase', DCS)))}
        {row('Dynamic Normalizer', 'Even out volume between tracks', <CToggle on={snd.normalizer} onTap={() => set('normalizer', !snd.normalizer)} />)}
        {row('ClearAudio+', 'Sony one-touch tuning — overrides EQ + DSP', <CToggle on={snd.clearaudio} onTap={() => set('clearaudio', !snd.clearaudio)} />)}
      </div>
      <div style={{ borderTop: `1px solid ${P.line}`, padding: '12px 22px 14px', flexShrink: 0 }}>
        <div style={{ fontFamily: P.mono, fontSize: 9, letterSpacing: '.14em', color: P.faint, lineHeight: 1.7 }}>
          SIGNAL PATH: SOURCE → EQ ({c.eqPreset}) → {[snd.dsee && 'DSEE HX', snd.vinyl && 'VINYL', snd.vpt !== 'Off' && `VPT·${snd.vpt.toUpperCase()}`, snd.dcphase !== 'Off' && 'DC PHASE'].filter(Boolean).join(' → ') || 'DIRECT'} → {c.bt.connected ? `BT·${c.bt.codec}` : 'AMP → 3.5MM'}
        </div>
        {c.snd.clearaudio && <div style={{ marginTop: 8, fontFamily: P.mono, fontSize: 9, letterSpacing: '.1em', color: P.acc }}>! CLEARAUDIO+ ACTIVE — EQ AND MANUAL DSP BYPASSED</div>}
      </div>
    </React.Fragment>
  );
}

// ─── Settings ──────────────────────────────────────────────────
function CSettings() {
  const c = useC(); const P = c.P;
  const row = (label, value, onTap) => (
    <div onClick={onTap} style={{ display: 'flex', alignItems: 'center', gap: 14, height: 58, padding: '0 22px', borderBottom: `1px solid ${P.line}`, cursor: onTap ? 'pointer' : 'default' }}>
      <span style={{ fontSize: 15, fontWeight: 600, flex: 1 }}>{label}</span>
      <span style={{ fontFamily: P.mono, fontSize: 10, color: P.faint, letterSpacing: '.04em' }}>{value}</span>
      {onTap && <span style={{ color: P.faint }}><FIChev /></span>}
    </div>
  );
  return (
    <React.Fragment>
      <CStatus />
      <CHeader title="Settings" />
      <div style={{ padding: '0 22px 9px', fontFamily: P.mono, fontSize: 9, letterSpacing: '.18em', color: P.faint }}>DISPLAY</div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 14, height: 58, padding: '0 22px', borderTop: `1px solid ${P.line}`, borderBottom: `1px solid ${P.line}` }}>
        <span style={{ fontSize: 15, fontWeight: 600, flex: 1 }}>Theme</span>
        <span style={{ display: 'flex', gap: 8 }}>
          {['Day', 'Night'].map((m) => (
            <span key={m} onClick={() => c.setTweak('theme', m)} style={{
              fontFamily: P.mono, fontSize: 10, letterSpacing: '.1em', padding: '7px 13px', cursor: 'pointer',
              border: `1px solid ${c.t.theme === m ? P.acc : P.line}`,
              color: c.t.theme === m ? P.accInk : P.dim, background: c.t.theme === m ? P.acc : 'transparent',
            }}>{m.toUpperCase()}</span>
          ))}
        </span>
      </div>
      {row('Screen-off timer', '30 SEC')}
      {row('Brightness', '3 / 5')}
      <div style={{ padding: '20px 22px 9px', fontFamily: P.mono, fontSize: 9, letterSpacing: '.18em', color: P.faint }}>SYSTEM</div>
      {row('Storage', '12.4 / 16 GB · SD 64 GB', () => {})}
      {row('Database', 'REBUILD · LAST: TODAY', () => {})}
      {row('Battery care', 'CHARGE LIMIT 90%', () => {})}
      {row('USB mode', c.usbDac ? 'DAC' : 'MASS STORAGE', () => c.go('usbdac'))}
      <div style={{ padding: '20px 22px 9px', fontFamily: P.mono, fontSize: 9, letterSpacing: '.18em', color: P.faint }}>ABOUT</div>
      {row('Firmware', 'CINDER 1.0 · RUST')}
      {row('Model', 'SONY NW-A55')}
      <div style={{ marginTop: 'auto' }}></div>
    </React.Fragment>
  );
}

Object.assign(window.CSCREENS, { menu: CMenu, eq: CEq, sound: CSound, settings: CSettings });
Object.assign(window, { CToggle });
