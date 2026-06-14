// ────────────────────────────────────────────────────────────────
// cinder-proto-screens4.jsx — Bluetooth, Pairing flow, BT Receiver,
// FM Radio, USB-DAC. Registers: bluetooth, pairing, receiver, fm, usbdac.
// ────────────────────────────────────────────────────────────────

// ─── Bluetooth hub ─────────────────────────────────────────────
function CBtScreen() {
  const c = useC(); const P = c.P;
  const bt = c.bt;
  const codecs = ['LDAC', 'aptX HD', 'aptX', 'AAC', 'SBC'];
  return (
    <React.Fragment>
      <CStatus />
      <CHeader title="Bluetooth" right={
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8, fontFamily: P.mono, fontSize: 10, letterSpacing: '.12em', color: bt.on ? P.acc : P.faint }}>
          {bt.on ? 'ON' : 'OFF'}
          <span onClick={() => c.setBt({ ...bt, on: !bt.on, connected: bt.on ? null : bt.connected })} style={{ width: 34, height: 18, border: `1px solid ${bt.on ? P.acc : P.line}`, position: 'relative', display: 'inline-block', cursor: 'pointer' }}>
            <span style={{ position: 'absolute', top: 2, width: 12, height: 12, background: bt.on ? P.acc : P.faint, right: bt.on ? 2 : 'auto', left: bt.on ? 'auto' : 2 }}></span>
          </span>
        </span>
      } />
      {bt.on && bt.connected ? (
        <div style={{ margin: '0 22px', border: `1px solid ${P.line}`, background: P.panel, padding: '18px 18px 16px' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <span style={{ fontFamily: P.mono, fontSize: 9, letterSpacing: '.18em', color: P.acc }}>CONNECTED</span>
            <span style={{ fontFamily: P.mono, fontSize: 9, letterSpacing: '.1em', color: P.dim }}>HP BATT 60%</span>
          </div>
          <div style={{ fontSize: 23, fontWeight: 700, marginTop: 8 }}>{bt.connected}</div>
          <div style={{ fontFamily: P.mono, fontSize: 10, color: P.dim, marginTop: 4, letterSpacing: '.04em' }}>{bt.codec} · 96 kHz · Sound quality preferred</div>
          <div style={{ display: 'flex', gap: 10, marginTop: 16 }}>
            <span onClick={() => c.setBt({ ...bt, connected: null })} style={{ flex: 1, height: 44, display: 'flex', alignItems: 'center', justifyContent: 'center', border: `1px solid ${P.line}`, fontSize: 13, fontWeight: 600, color: P.dim, cursor: 'pointer' }}>Disconnect</span>
            <span onClick={() => c.setBt({ ...bt, codec: codecs[(codecs.indexOf(bt.codec) + 1) % codecs.length] })} style={{ flex: 1, height: 44, display: 'flex', alignItems: 'center', justifyContent: 'center', border: `1px solid ${P.line}`, fontSize: 13, fontWeight: 600, color: P.dim, cursor: 'pointer' }}>Quality · {bt.codec}</span>
          </div>
        </div>
      ) : (
        <div style={{ margin: '0 22px', border: `1px dashed ${P.line}`, padding: '22px 18px', textAlign: 'center', color: P.faint, fontSize: 13 }}>
          {bt.on ? 'No device connected' : 'Bluetooth is off'}
        </div>
      )}
      <div style={{ padding: '22px 22px 8px', fontFamily: P.mono, fontSize: 9, letterSpacing: '.18em', color: P.faint }}>PAIRED DEVICES — TAP TO CONNECT</div>
      <div>
        {FPAIRED.map((d) => (
          <div key={d.name} onClick={() => bt.on && c.setBt({ ...bt, connected: d.name, codec: d.kind.includes('LDAC') ? 'LDAC' : d.kind.includes('AAC') ? 'AAC' : 'SBC' })} style={{ display: 'flex', alignItems: 'center', gap: 14, height: 58, padding: '0 22px', borderBottom: `1px solid ${P.line}`, cursor: 'pointer', opacity: bt.on ? 1 : 0.4 }}>
            <span style={{ color: P.dim }}><FIBt size={16} /></span>
            <span style={{ flex: 1 }}>
              <span style={{ fontSize: 15, fontWeight: 600, display: 'block', color: bt.connected === d.name ? P.acc : P.ink }}>{d.name}</span>
              <span style={{ fontFamily: P.mono, fontSize: 9, color: P.faint, letterSpacing: '.06em' }}>{d.kind}</span>
            </span>
            <span style={{ fontFamily: P.mono, fontSize: 10, letterSpacing: '.1em', color: bt.connected === d.name ? P.faint : P.acc, border: `1px solid ${bt.connected === d.name ? P.line : P.acc}`, padding: '6px 12px' }}>
              {bt.connected === d.name ? 'ACTIVE' : 'CONNECT'}
            </span>
          </div>
        ))}
      </div>
      <div style={{ marginTop: 'auto', padding: '0 22px 18px' }}>
        <div onClick={() => bt.on && c.go('pairing')} style={{ height: 52, background: bt.on ? P.acc : P.line, color: bt.on ? P.accInk : P.faint, display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 10, fontSize: 15, fontWeight: 700, cursor: 'pointer' }}>
          <FIBt size={17} /> Pair new device
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: 12, color: P.faint, fontFamily: P.mono, fontSize: 9, letterSpacing: '.08em' }}>
          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 7 }}><FINfc size={14} /> NFC · TOUCH DEVICE TO REAR PANEL</span>
          <span onClick={() => c.go('receiver')} style={{ color: P.dim, cursor: 'pointer' }}>RECEIVER MODE ›</span>
        </div>
      </div>
    </React.Fragment>
  );
}

// ─── Pairing flow ──────────────────────────────────────────────
function CPairing() {
  const c = useC(); const P = c.P;
  const [found, setFound] = React.useState([]);
  const [pairing, setPairing] = React.useState(null);
  const discoverable = [
    { name: 'WH-1000XM5', kind: 'Headphones · LDAC capable' },
    { name: 'JBL Flip 6', kind: 'Speaker · AAC' },
    { name: 'Soundcore Q45', kind: 'Headphones · LDAC capable' },
  ];
  React.useEffect(() => {
    const timers = discoverable.map((d, i) => setTimeout(() => setFound((f) => [...f, d]), 900 + i * 1100));
    return () => timers.forEach(clearTimeout);
  }, []);
  const pair = (d) => {
    setPairing(d.name);
    setTimeout(() => {
      c.setBt({ ...c.bt, on: true, connected: d.name, codec: d.kind.includes('LDAC') ? 'LDAC' : 'AAC' });
      c.go('bluetooth');
    }, 1400);
  };
  return (
    <React.Fragment>
      <CStatus />
      <CHeader title="Pair new" right={<span style={{ fontFamily: P.mono, fontSize: 10, color: P.acc, letterSpacing: '.12em' }}>SCANNING…</span>} />
      <div style={{ margin: '0 22px 18px', border: `1px solid ${P.line}`, background: P.panel, padding: '14px 16px', display: 'flex', alignItems: 'center', gap: 14 }}>
        <FINfc size={22} />
        <span style={{ flex: 1 }}>
          <span style={{ display: 'block', fontSize: 14, fontWeight: 600 }}>One-touch NFC</span>
          <span style={{ display: 'block', fontSize: 11, color: P.dim, marginTop: 3 }}>Touch an NFC device to the rear panel to pair instantly</span>
        </span>
      </div>
      <div style={{ padding: '0 22px 8px', fontFamily: P.mono, fontSize: 9, letterSpacing: '.18em', color: P.faint }}>
        DISCOVERABLE · {found.length}
      </div>
      <div style={{ flex: 1 }}>
        {found.length === 0 && (
          <div style={{ padding: '28px 22px', textAlign: 'center' }}>
            <FBars n={16} seed={9} h={20} gap={4} color={P.acc} dimColor={P.line} style={{ width: 160, margin: '0 auto' }} />
            <div style={{ fontFamily: P.mono, fontSize: 10, color: P.faint, marginTop: 14, letterSpacing: '.1em' }}>LISTENING FOR DEVICES…</div>
          </div>
        )}
        {found.map((d) => (
          <div key={d.name} onClick={() => !pairing && pair(d)} style={{ display: 'flex', alignItems: 'center', gap: 14, height: 62, padding: '0 22px', borderBottom: `1px solid ${P.line}`, cursor: 'pointer' }}>
            <span style={{ color: P.dim }}><FIBt size={16} /></span>
            <span style={{ flex: 1 }}>
              <span style={{ fontSize: 15, fontWeight: 600, display: 'block' }}>{d.name}</span>
              <span style={{ fontFamily: P.mono, fontSize: 9, color: P.faint, letterSpacing: '.06em' }}>{d.kind}</span>
            </span>
            <span style={{ fontFamily: P.mono, fontSize: 10, letterSpacing: '.1em', color: pairing === d.name ? P.faint : P.acc, border: `1px solid ${pairing === d.name ? P.line : P.acc}`, padding: '6px 12px' }}>
              {pairing === d.name ? 'PAIRING…' : 'PAIR'}
            </span>
          </div>
        ))}
      </div>
      <div style={{ borderTop: `1px solid ${P.line}`, padding: '12px 22px 16px', fontFamily: P.mono, fontSize: 9, letterSpacing: '.1em', color: P.faint, lineHeight: 1.8, flexShrink: 0 }}>
        TIP: HOLD YOUR HEADPHONES' POWER BUTTON ~7s UNTIL THE LED BLINKS BLUE.
      </div>
    </React.Fragment>
  );
}

// ─── BT Receiver mode ──────────────────────────────────────────
function CReceiver() {
  const c = useC(); const P = c.P;
  return (
    <React.Fragment>
      <CStatus />
      <CHeader title="BT Receiver" right={
        <span onClick={() => c.setRx(!c.rx)} style={{ width: 34, height: 18, border: `1px solid ${c.rx ? P.acc : P.line}`, position: 'relative', display: 'inline-block', cursor: 'pointer' }}>
          <span style={{ position: 'absolute', top: 2, width: 12, height: 12, background: c.rx ? P.acc : P.faint, right: c.rx ? 2 : 'auto', left: c.rx ? 'auto' : 2 }}></span>
        </span>
      } />
      {c.rx ? (
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', textAlign: 'center', padding: '0 40px' }}>
          <span style={{ color: P.acc }}><FIRx size={44} /></span>
          <div style={{ fontSize: 22, fontWeight: 700, marginTop: 22 }}>Discoverable as "NW-A55"</div>
          <div style={{ fontSize: 13, color: P.dim, marginTop: 10, lineHeight: 1.6 }}>Play from your phone — the Walkman becomes the DAC + amp for your wired headphones.</div>
          <FBars n={22} seed={5} h={26} gap={3} color={P.acc} dimColor={P.line} style={{ width: 220, marginTop: 28 }} />
          <div style={{ fontFamily: P.mono, fontSize: 10, color: P.faint, marginTop: 12, letterSpacing: '.12em' }}>WAITING FOR SOURCE · LDAC / AAC / SBC</div>
        </div>
      ) : (
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', textAlign: 'center', padding: '0 40px' }}>
          <span style={{ color: P.faint }}><FIRx size={44} /></span>
          <div style={{ fontSize: 19, fontWeight: 700, marginTop: 22, color: P.dim }}>Receiver mode is off</div>
          <div style={{ fontSize: 13, color: P.faint, marginTop: 10, lineHeight: 1.6 }}>Turn on to stream from a phone into the Walkman's DAC and amp. Local playback pauses while active.</div>
        </div>
      )}
      <div style={{ borderTop: `1px solid ${P.line}`, padding: '12px 22px 16px', fontFamily: P.mono, fontSize: 9, letterSpacing: '.1em', color: P.faint, lineHeight: 1.8, flexShrink: 0 }}>
        NOTE: EQ + DSP APPLY TO RECEIVED AUDIO TOO.
      </div>
    </React.Fragment>
  );
}

// ─── FM Radio ──────────────────────────────────────────────────
function CFm() {
  const c = useC(); const P = c.P;
  const presets = [87.6, 88.6, 92.3, 96.1, 99.9, 104.7];
  const fm = c.fm;
  const MIN = 76, MAX = 108;
  const tickFreqs = [];
  for (let f = 80; f <= 106; f += 2) tickFreqs.push(f);
  return (
    <React.Fragment>
      <CStatus />
      <CHeader title="FM Radio" right={<span style={{ fontFamily: P.mono, fontSize: 10, color: P.faint, letterSpacing: '.1em' }}>STEREO</span>} />
      <div style={{ textAlign: 'center', padding: '26px 0 0' }}>
        <span style={{ fontFamily: P.mono, fontSize: 86, fontWeight: 300, letterSpacing: '-.03em', color: P.ink }}>{fm.freq.toFixed(1)}</span>
        <span style={{ fontFamily: P.mono, fontSize: 17, color: P.dim, marginLeft: 9 }}>MHz</span>
      </div>
      <div style={{ margin: '20px 30px 0', position: 'relative', height: 56 }}>
        <div style={{ position: 'absolute', left: 0, right: 0, top: 26, borderTop: `1px solid ${P.line}` }}></div>
        {tickFreqs.map((f) => {
          const x = ((f - MIN) / (MAX - MIN)) * 100;
          return (
            <div key={f} style={{ position: 'absolute', left: `${x}%`, top: 16, width: 1, height: 20, background: P.line }}>
              <span style={{ position: 'absolute', top: 24, left: -10, width: 20, textAlign: 'center', fontFamily: P.mono, fontSize: 8, color: P.faint }}>{f}</span>
            </div>
          );
        })}
        <div style={{ position: 'absolute', left: `${((fm.freq - MIN) / (MAX - MIN)) * 100}%`, top: 6, width: 2, height: 40, background: P.acc }}></div>
      </div>
      <div style={{ display: 'flex', justifyContent: 'center', gap: 12, marginTop: 26 }}>
        {[['−0.1', -0.1], ['SEEK −', -1.7], ['SEEK +', 2.1], ['+0.1', 0.1]].map(([label, d]) => (
          <span key={label} onClick={() => c.setFm({ ...fm, freq: Math.round(Math.max(MIN, Math.min(MAX, fm.freq + d)) * 10) / 10 })} style={{
            fontFamily: P.mono, fontSize: 11, letterSpacing: '.08em', border: `1px solid ${P.line}`,
            padding: '12px 16px', color: P.dim, cursor: 'pointer',
          }}>{label}</span>
        ))}
      </div>
      <div style={{ padding: '28px 22px 8px', fontFamily: P.mono, fontSize: 9, letterSpacing: '.18em', color: P.faint }}>PRESETS — HOLD TO SAVE</div>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 10, padding: '0 22px' }}>
        {presets.map((f, i) => {
          const active = Math.abs(f - fm.freq) < 0.05;
          return (
            <span key={f} onClick={() => c.setFm({ freq: f, preset: i + 1 })} style={{
              height: 52, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
              border: `1px solid ${active ? P.acc : P.line}`, background: active ? P.acc : 'transparent',
              color: active ? P.accInk : P.dim, cursor: 'pointer',
            }}>
              <span style={{ fontFamily: P.mono, fontSize: 15, fontWeight: active ? 700 : 400 }}>{f.toFixed(1)}</span>
              <span style={{ fontFamily: P.mono, fontSize: 8, letterSpacing: '.14em', marginTop: 2, opacity: 0.7 }}>P{i + 1}</span>
            </span>
          );
        })}
      </div>
      <div style={{ marginTop: 'auto', borderTop: `1px solid ${P.line}`, padding: '12px 22px 16px', fontFamily: P.mono, fontSize: 9, letterSpacing: '.1em', color: P.faint, flexShrink: 0 }}>
        ANTENNA: HEADPHONE CABLE — WIRED HEADPHONES REQUIRED FOR FM.
      </div>
    </React.Fragment>
  );
}

// ─── USB-DAC ───────────────────────────────────────────────────
function CUsbDac() {
  const c = useC(); const P = c.P;
  return (
    <React.Fragment>
      <CStatus />
      <CHeader title="USB-DAC" right={
        <span onClick={() => c.setUsbDac(!c.usbDac)} style={{ width: 34, height: 18, border: `1px solid ${c.usbDac ? P.acc : P.line}`, position: 'relative', display: 'inline-block', cursor: 'pointer' }}>
          <span style={{ position: 'absolute', top: 2, width: 12, height: 12, background: c.usbDac ? P.acc : P.faint, right: c.usbDac ? 2 : 'auto', left: c.usbDac ? 'auto' : 2 }}></span>
        </span>
      } />
      {c.usbDac ? (
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', textAlign: 'center', padding: '0 40px' }}>
          <span style={{ color: P.acc }}><FIUsb size={44} /></span>
          <div style={{ fontSize: 22, fontWeight: 700, marginTop: 22 }}>USB-DAC active</div>
          <div style={{ fontFamily: P.mono, fontSize: 11, color: P.acc, marginTop: 12, letterSpacing: '.1em' }}>PC → NW-A55 → HEADPHONES</div>
          <div style={{ margin: '26px 0 0', border: `1px solid ${P.line}`, background: P.panel, padding: '14px 22px', fontFamily: P.mono, fontSize: 11, color: P.dim, lineHeight: 2, textAlign: 'left' }}>
            INPUT&nbsp;&nbsp;: PCM 24BIT / 96.0 KHZ<br />
            SOURCE : DESKTOP-7F3K (USB)<br />
            DSP&nbsp;&nbsp;&nbsp;&nbsp;: EQ {c.eqPreset} {c.snd.dsee ? '· DSEE HX' : ''}<br />
            OUTPUT : 3.5MM UNBALANCED
          </div>
        </div>
      ) : (
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', textAlign: 'center', padding: '0 40px' }}>
          <span style={{ color: P.faint }}><FIUsb size={44} /></span>
          <div style={{ fontSize: 19, fontWeight: 700, marginTop: 22, color: P.dim }}>USB-DAC is off</div>
          <div style={{ fontSize: 13, color: P.faint, marginTop: 10, lineHeight: 1.6 }}>Turn on, then connect to a computer — the Walkman becomes its sound card. Local playback pauses while active.</div>
        </div>
      )}
      <div style={{ borderTop: `1px solid ${P.line}`, padding: '12px 22px 16px', fontFamily: P.mono, fontSize: 9, letterSpacing: '.1em', color: P.faint, lineHeight: 1.8, flexShrink: 0 }}>
        CHARGING WHILE IN DAC MODE: ON
      </div>
    </React.Fragment>
  );
}

Object.assign(window.CSCREENS, { bluetooth: CBtScreen, pairing: CPairing, receiver: CReceiver, fm: CFm, usbdac: CUsbDac });
