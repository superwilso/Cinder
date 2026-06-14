// ────────────────────────────────────────────────────────────────
// Direction 2 — NOCTURNE · extra screens
// ────────────────────────────────────────────────────────────────

const NcX = NcColors; // brought in from screens-nocturne.jsx

// Settings row in Nocturne style.
function NcRow({ label, value, on, type = 'nav', danger = false }) {
  return (
    <div style={{
      display: 'flex', alignItems: 'center', justifyContent: 'space-between',
      padding: '12px 0', borderBottom: `1px solid ${NcX.rule}`,
    }}>
      <span className="serif" style={{ fontSize: 15, color: danger ? '#ff8a7a' : 'inherit' }}>{label}</span>
      <span style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
        {type === 'toggle' ? (
          <span style={{ width: 32, height: 18, borderRadius: 10, background: on ? NcX.accent : 'rgba(255,255,255,.10)', position: 'relative' }}>
            <span style={{ position: 'absolute', top: 2, left: on ? 16 : 2, width: 14, height: 14, borderRadius: '50%', background: on ? '#0a0a14' : '#fff' }} />
          </span>
        ) : type === 'action' ? null : (
          <>
            {value && <span className="mono" style={{ fontSize: 10, opacity: .55 }}>{value}</span>}
            <IconChevron size={12} style={{ opacity: .35 }} />
          </>
        )}
      </span>
    </div>
  );
}

// ─── Now Playing · Meter (digital peak) ─────────────────────────
function Nc_NowPlayingMeter({ track = TRACKS[0] }) {
  const t = track;
  return (
    <div className="scr nocturne">
      <StatusBar badge={<span className="nb-badge">Hi-Res</span>} right={<span className="mono" style={{ fontSize: 9, opacity: .55 }}>Peak / RMS</span>} />

      <div style={{ padding: '20px 30px 0' }}>
        <NcKicker accent>Now Playing · Meter View</NcKicker>
        <div className="display" style={{ fontSize: 26, lineHeight: 1.05, marginTop: 4 }}>{t.title}</div>
        <div className="italic" style={{ fontSize: 14, opacity: .65, marginTop: 2 }}>{t.artist}</div>
      </div>

      {/* Two horizontal meters (Peak + RMS) — long, restrained */}
      <div style={{ padding: '32px 30px 0' }}>
        {[
          { label: 'L · Peak', val: 0.82, peak: '−2.4 dB', heat: true },
          { label: 'L · RMS',  val: 0.55, peak: '−9.1 dB', heat: false },
          { label: 'R · Peak', val: 0.78, peak: '−3.1 dB', heat: true },
          { label: 'R · RMS',  val: 0.50, peak: '−10.0 dB', heat: false },
        ].map(m => (
          <div key={m.label} style={{ marginBottom: 14 }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
              <span className="mono" style={{ fontSize: 9, opacity: .55, letterSpacing: '.14em' }}>{m.label.toUpperCase()}</span>
              <span className="mono" style={{ fontSize: 9, color: NcX.accent }}>{m.peak}</span>
            </div>
            <div style={{ display: 'flex', gap: 1, height: 8 }}>
              {Array.from({ length: 50 }).map((_, i) => {
                const lit = i < m.val * 50;
                const hot = i >= 44;
                return <div key={i} style={{ flex: 1, background: lit ? (hot ? '#ff7a5c' : NcX.accent) : NcX.faint }} />;
              })}
            </div>
          </div>
        ))}
      </div>

      {/* Big numerical readout */}
      <div style={{ padding: '24px 30px 0' }}>
        <NcKicker>Integrated</NcKicker>
        <div className="display" style={{ fontSize: 56, lineHeight: 1, color: NcX.accent, letterSpacing: '-.02em', marginTop: 2 }}>−14.8 LUFS</div>
        <div className="mono" style={{ fontSize: 10, opacity: .55, marginTop: 6 }}>Crest factor 14.2 dB · True peak −0.4 dBTP</div>
      </div>

      <div style={{ position: 'absolute', left: 30, right: 30, bottom: 24 }}>
        <NcWaveform height={20} playedPct={0.38} />
        <div className="mono" style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, opacity: .55, marginTop: 4 }}>
          <span>{t.cur}</span><span>−2:45</span>
        </div>
      </div>
    </div>
  );
}

// ─── Track Detail ───────────────────────────────────────────────
function Nc_TrackDetail({ track = TRACKS[0] }) {
  const t = track;
  return (
    <div className="scr nocturne">
      <StatusBar badge={<span className="nb-badge">Hi-Res</span>} />
      <div style={{ padding: '14px 30px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 18 }}>
          <IconBack size={18} style={{ opacity: .55 }} />
          <div className="display" style={{ fontSize: 30 }}>Track <span className="italic">Info</span></div>
        </div>

        <Art kind={t.art} size={420} label={false} style={{ width: '100%', aspectRatio: '1', height: 'auto', marginBottom: 14 }} />

        <div className="display" style={{ fontSize: 26, lineHeight: 1.05 }}>{t.title}</div>
        <div className="italic" style={{ fontSize: 16, opacity: .75 }}>{t.artist}</div>

        <NcKicker accent style={{ marginTop: 18, marginBottom: 6 }}>Format</NcKicker>
        {[
          ['Codec', t.codec === 'DSD' ? 'DSD · DSF' : t.codec],
          ['Sample rate', t.rate],
          ['Bit depth', t.codec === 'DSD' ? '1-bit' : `${t.bits}-bit`],
          ['Bitrate', `${t.bitrate} kbps`],
        ].map(([k, v]) => (
          <div key={k} style={{ display: 'flex', justifyContent: 'space-between', padding: '8px 0', borderBottom: `1px solid ${NcX.rule}` }}>
            <span className="serif" style={{ fontSize: 14, opacity: .7 }}>{k}</span>
            <span className="mono" style={{ fontSize: 11 }}>{v}</span>
          </div>
        ))}

        <NcKicker accent style={{ marginTop: 14, marginBottom: 6 }}>File</NcKicker>
        <div className="mono" style={{ fontSize: 10, opacity: .65, padding: '8px 0', borderBottom: `1px solid ${NcX.rule}`, wordBreak: 'break-all' }}>
          /Music/{t.artist}/{t.album}/03 {t.title}.flac
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between', padding: '8px 0' }}>
          <span className="serif" style={{ fontSize: 14, opacity: .7 }}>Size</span>
          <span className="mono" style={{ fontSize: 11 }}>84.2 MB · SD Card</span>
        </div>
      </div>
    </div>
  );
}

// ─── Sync Lyrics ────────────────────────────────────────────────
function Nc_Lyrics({ track = TRACKS[0] }) {
  return (
    <div className="scr nocturne">
      <StatusBar badge={<span className="nb-badge">Synced Lyrics</span>} />
      <div style={{ padding: '20px 36px 0' }}>
        <div className="italic" style={{ fontSize: 14, opacity: .55 }}>from</div>
        <div className="display" style={{ fontSize: 22, lineHeight: 1.05 }}>{track.title}</div>
        <div className="italic" style={{ fontSize: 12, opacity: .55 }}>{track.artist}</div>

        <div style={{ marginTop: 30 }}>
          {LYRICS.map((l, i) => {
            const isCur = l.current;
            const dist = Math.abs(LYRICS.findIndex(x => x.current) - i);
            const op = isCur ? 1 : Math.max(0.2, 0.7 - dist * 0.12);
            return (
              <div key={i} className={isCur ? 'display' : 'serif'} style={{
                fontSize: isCur ? 26 : 18, lineHeight: 1.4, marginBottom: 16,
                opacity: op, color: isCur ? NcX.accent : 'inherit',
                fontStyle: isCur ? 'italic' : 'normal',
              }}>{l.line}</div>
            );
          })}
        </div>
      </div>

      <div style={{ position: 'absolute', left: 30, right: 30, bottom: 22 }}>
        <NcWaveform height={20} playedPct={0.38} />
        <div className="mono" style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, opacity: .55, marginTop: 4 }}>
          <span>1:47</span><span>−2:45</span>
        </div>
      </div>
    </div>
  );
}

// ─── Sound Settings ─────────────────────────────────────────────
function Nc_SoundSettings() {
  return (
    <div className="scr nocturne">
      <StatusBar badge={<span className="nb-badge">Hi-Res</span>} />
      <div style={{ padding: '14px 30px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 18 }}>
          <IconBack size={18} style={{ opacity: .55 }} />
          <div className="display" style={{ fontSize: 30 }}>Sound <span className="italic">Settings</span></div>
        </div>

        {SOUND_SETTINGS.map(s => (
          <div key={s.group} style={{ marginBottom: 18 }}>
            <NcKicker accent style={{ marginBottom: 6 }}>{s.group}</NcKicker>
            {s.items.map((it, ii) => (
              <NcRow key={ii} {...it} />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Bluetooth pair / LDAC ──────────────────────────────────────
function Nc_Bluetooth() {
  return (
    <div className="scr nocturne">
      <StatusBar badge={<span className="nb-badge">Hi-Res</span>} right={<IconBluetooth size={12} style={{ color: NcX.accent }} />} />
      <div style={{ padding: '14px 30px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 18 }}>
          <IconBack size={18} style={{ opacity: .55 }} />
          <div className="display" style={{ fontSize: 30 }}>Bluetooth</div>
          <span style={{ marginLeft: 'auto', width: 32, height: 18, borderRadius: 10, background: NcX.accent, position: 'relative' }}>
            <span style={{ position: 'absolute', top: 2, left: 16, width: 14, height: 14, borderRadius: '50%', background: '#0a0a14' }} />
          </span>
        </div>

        {/* Connected hero */}
        <div style={{
          padding: '16px 18px',
          background: 'rgba(196,182,255,.06)',
          borderTop: `1px solid ${NcX.accent}`,
          borderBottom: `1px solid ${NcX.accent}`,
        }}>
          <NcKicker accent>Connected</NcKicker>
          <div className="display" style={{ fontSize: 22, marginTop: 4 }}>WH-1000XM5</div>
          <div className="mono" style={{ fontSize: 10, opacity: .65, marginTop: 4 }}>LDAC · 990 kbps · 92% battery</div>
        </div>

        <NcKicker accent style={{ marginTop: 18, marginBottom: 6 }}>Wireless Playback Quality</NcKicker>
        {LDAC_QUALITY.map((q, i) => (
          <div key={q.label} style={{
            display: 'flex', alignItems: 'flex-start', gap: 12, padding: '12px 0',
            borderTop: i === 0 ? `1px solid ${NcX.rule}` : 'none',
            borderBottom: `1px solid ${NcX.rule}`,
          }}>
            <span style={{
              width: 18, height: 18, borderRadius: '50%',
              border: `1.5px solid ${q.selected ? NcX.accent : 'rgba(255,255,255,.25)'}`,
              display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0, marginTop: 2,
            }}>{q.selected && <span style={{ width: 8, height: 8, borderRadius: '50%', background: NcX.accent }} />}</span>
            <div style={{ flex: 1 }}>
              <div className="serif" style={{ fontSize: 15, color: q.selected ? NcX.accent : 'inherit' }}>{q.label}</div>
              <div className="italic" style={{ fontSize: 11, opacity: .6, marginTop: 2 }}>{q.sub}</div>
            </div>
          </div>
        ))}

        <NcKicker accent style={{ marginTop: 16, marginBottom: 6 }}>Paired</NcKicker>
        {BT_DEVICES.filter(d => d.paired && !d.connected).map(d => (
          <div key={d.name} style={{ display: 'grid', gridTemplateColumns: '1fr 50px 14px', gap: 10, alignItems: 'center', padding: '10px 0', borderBottom: `1px solid ${NcX.rule}` }}>
            <div>
              <div className="serif" style={{ fontSize: 14 }}>{d.name}</div>
              <div className="italic" style={{ fontSize: 11, opacity: .5 }}>{d.kind} · {d.codec}</div>
            </div>
            <span className="mono" style={{ fontSize: 9, opacity: .55, textAlign: 'right' }}>{d.rssi}/4</span>
            <IconChevron size={12} style={{ opacity: .35 }} />
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── BT Receiver ────────────────────────────────────────────────
function Nc_BTReceiver() {
  return (
    <div className="scr nocturne">
      <StatusBar badge={<span className="nb-badge">RX</span>} />
      <div style={{ padding: '14px 30px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 18 }}>
          <IconBack size={18} style={{ opacity: .55 }} />
          <div className="display" style={{ fontSize: 30 }}>BT <span className="italic">Receiver</span></div>
        </div>

        {/* Hero — broadcasting */}
        <div style={{ textAlign: 'center', padding: '30px 0 18px' }}>
          <NcKicker accent>Broadcasting as</NcKicker>
          <div className="display" style={{ fontSize: 44, lineHeight: 1.0, marginTop: 6 }}>NW-A55</div>
          <div className="italic" style={{ fontSize: 14, opacity: .55, marginTop: 18 }}>receiving from</div>
          <div className="display" style={{ fontSize: 24, marginTop: 2 }}>iPhone 15 Pro</div>

          {/* Animated-feel dot wave */}
          <div style={{ display: 'flex', gap: 4, justifyContent: 'center', alignItems: 'center', marginTop: 22 }}>
            {[0,1,2,3,4,5,6,7,8,9,10].map(i => (
              <span key={i} style={{
                width: 6, height: 6 + Math.abs(5 - i) * 3,
                borderRadius: 999, background: NcX.accent,
                opacity: i < 7 ? 1 : .35,
              }} />
            ))}
          </div>
          <div className="mono" style={{ fontSize: 10, color: NcX.accent, marginTop: 14, letterSpacing: '.14em' }}>RX · LDAC · 990 kbps</div>
        </div>

        <NcKicker accent style={{ marginTop: 8, marginBottom: 6 }}>Source Devices</NcKicker>
        <NcRow label="iPhone 15 Pro" value="Connected" />
        <NcRow label="MacBook Air M3" value="Paired" />

        <div style={{ marginTop: 18, padding: '12px 14px', border: `1px dashed ${NcX.rule}` }}>
          <div className="italic" style={{ fontSize: 12, opacity: .65, lineHeight: 1.45 }}>
            Sound Settings do not apply when the Walkman is acting as a Bluetooth receiver.
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── USB-DAC ────────────────────────────────────────────────────
function Nc_UsbDac() {
  return (
    <div className="scr nocturne">
      <StatusBar badge={<span className="nb-badge">USB-DAC</span>} right={<span className="mono" style={{ fontSize: 9, opacity: .55 }}>RX · PCM</span>} />
      <div style={{ padding: '14px 30px' }}>
        <NcKicker accent>Source</NcKicker>
        <div className="serif" style={{ fontSize: 15, marginTop: 2 }}>MacBook Air M3 · USB-C</div>

        <div style={{ textAlign: 'center', marginTop: 38 }}>
          <NcKicker>Sample Rate</NcKicker>
          <div className="display" style={{ fontSize: 100, lineHeight: 1, color: NcX.accent, letterSpacing: '-.04em', marginTop: 4 }}>96.0</div>
          <div className="mono" style={{ fontSize: 12, opacity: .55, marginTop: -6 }}>kHz · 24-bit · PCM</div>
        </div>

        <div style={{ marginTop: 36 }}>
          <NcKicker accent style={{ marginBottom: 8 }}>Receive Level</NcKicker>
          {['L', 'R'].map((ch, i) => (
            <div key={ch} style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 6 }}>
              <span className="mono" style={{ fontSize: 10, width: 14, opacity: .55 }}>{ch}</span>
              <div style={{ flex: 1, display: 'flex', gap: 1 }}>
                {Array.from({ length: 40 }).map((_, k) => {
                  const lit = k < (i === 0 ? 30 : 28);
                  const peak = k >= 36;
                  return <div key={k} style={{ flex: 1, height: 6, background: lit ? (peak ? '#ff7a5c' : NcX.accent) : NcX.faint }} />;
                })}
              </div>
              <span className="mono" style={{ fontSize: 9, opacity: .55, width: 40, textAlign: 'right' }}>{i === 0 ? '−3.1' : '−4.2'}</span>
            </div>
          ))}
        </div>

        <NcKicker accent style={{ marginTop: 24, marginBottom: 6 }}>USB-DAC Settings</NcKicker>
        <NcRow label="DAC Filter" value="Slow Roll-off" />
        <NcRow label="Charge from connected device" type="toggle" on={true} />
        <NcRow label="DSD over PCM (DoP)" type="toggle" on={true} />

        <div className="italic" style={{ fontSize: 12, opacity: .55, marginTop: 14, lineHeight: 1.45 }}>
          Sound Settings do not apply during USB audio output.
        </div>
      </div>
    </div>
  );
}

// ─── Output Routing ─────────────────────────────────────────────
function Nc_Output() {
  return (
    <div className="scr nocturne">
      <StatusBar badge={<span className="nb-badge">Hi-Res</span>} />
      <div style={{ padding: '14px 30px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 18 }}>
          <IconBack size={18} style={{ opacity: .55 }} />
          <div className="display" style={{ fontSize: 30 }}>Output</div>
        </div>

        <NcKicker accent style={{ marginBottom: 8 }}>Destination</NcKicker>
        {[
          { name: '3.5 mm Stereo Mini', sub: 'Onkyo IE-FC300 detected',  active: true,  available: true },
          { name: '4.4 mm Balanced',    sub: 'Not on this model',         active: false, available: false },
          { name: 'Bluetooth',          sub: 'WH-1000XM5 · LDAC',          active: false, available: true },
          { name: 'USB Audio',          sub: 'No host connected',          active: false, available: false },
        ].map((o, i) => (
          <div key={o.name} style={{
            display: 'flex', alignItems: 'flex-start', gap: 12, padding: '12px 0',
            borderTop: i === 0 ? `1px solid ${NcX.rule}` : 'none',
            borderBottom: `1px solid ${NcX.rule}`,
            opacity: o.available ? 1 : .35,
          }}>
            <span style={{
              width: 18, height: 18, borderRadius: '50%',
              border: `1.5px solid ${o.active ? NcX.accent : 'rgba(255,255,255,.25)'}`,
              display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0, marginTop: 2,
            }}>{o.active && <span style={{ width: 8, height: 8, borderRadius: '50%', background: NcX.accent }} />}</span>
            <div style={{ flex: 1 }}>
              <div className="serif" style={{ fontSize: 16, color: o.active ? NcX.accent : 'inherit' }}>{o.name}</div>
              <div className="italic" style={{ fontSize: 11, opacity: .55, marginTop: 2 }}>{o.sub}</div>
            </div>
          </div>
        ))}

        <NcKicker accent style={{ marginTop: 18, marginBottom: 6 }}>Gain</NcKicker>
        <NcRow label="High Gain · Stereo Mini" type="toggle" on={false} />
        <NcRow label="High Gain · Balanced"    type="toggle" on={false} />
        <div className="italic" style={{ fontSize: 12, opacity: .55, marginTop: 10, lineHeight: 1.45 }}>
          High Gain raises output by ~6 dB for low-sensitivity headphones.
        </div>
      </div>
    </div>
  );
}

// ─── Reset / Format ─────────────────────────────────────────────
function Nc_Reset() {
  return (
    <div className="scr nocturne">
      <StatusBar badge={<span className="nb-badge">Hi-Res</span>} />
      <div style={{ padding: '14px 30px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 18 }}>
          <IconBack size={18} style={{ opacity: .55 }} />
          <div className="display" style={{ fontSize: 30 }}>Reset / <span className="italic">Format</span></div>
        </div>

        {RESET_ITEMS.map((r, i) => (
          <div key={r.label} style={{ padding: '14px 0', borderTop: i === 0 ? `1px solid ${NcX.rule}` : 'none', borderBottom: `1px solid ${NcX.rule}` }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <span className="serif" style={{ fontSize: 15, color: r.destructive ? '#ff8a7a' : 'inherit' }}>{r.label}</span>
              <IconChevron size={12} style={{ opacity: .35, marginLeft: 'auto' }} />
            </div>
            <div className="italic" style={{ fontSize: 12, opacity: .55, marginTop: 4, lineHeight: 1.4 }}>{r.desc}</div>
          </div>
        ))}

        <div style={{ marginTop: 18, padding: '14px', background: 'rgba(255,138,122,.07)', border: `1px solid rgba(255,138,122,.22)` }}>
          <NcKicker style={{ color: '#ff8a7a' }}>Heads up</NcKicker>
          <div className="italic" style={{ fontSize: 12, marginTop: 4, opacity: .85 }}>
            Storage formats are permanent. Rebuilding the database takes ~4 min on a full 64 GB card.
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── Volume popup ───────────────────────────────────────────────
function Nc_Volume() {
  return (
    <div className="scr nocturne">
      <div style={{ position: 'absolute', inset: 0, opacity: .2 }}>
        <Nc_NowPlayingHero />
      </div>
      <div style={{ position: 'absolute', inset: 0, background: 'rgba(6,6,8,.6)', backdropFilter: 'blur(2px)' }} />

      <div style={{
        position: 'absolute', left: 32, right: 32, top: 220,
        padding: '28px 30px',
        background: '#0b0b10',
        borderTop: `1px solid ${NcX.accent}`,
        borderBottom: `1px solid ${NcX.accent}`,
      }}>
        <NcKicker accent>Volume</NcKicker>
        <div style={{ display: 'flex', alignItems: 'baseline', gap: 10, marginTop: 4 }}>
          <span className="display" style={{ fontSize: 72, lineHeight: 1, color: NcX.accent, letterSpacing: '-.03em' }}>21</span>
          <span className="italic" style={{ fontSize: 16, opacity: .55 }}>/ 120</span>
        </div>

        <div style={{ display: 'flex', gap: 2, marginTop: 18 }}>
          {Array.from({ length: 30 }).map((_, i) => {
            const lit = i < 18;
            const warn = i >= 25;
            return <div key={i} style={{ flex: 1, height: 18, background: lit ? (warn ? '#ff7a5c' : NcX.accent) : NcX.faint }} />;
          })}
        </div>

        <div className="mono" style={{ fontSize: 9, opacity: .55, marginTop: 12, display: 'flex', justifyContent: 'space-between', letterSpacing: '.1em' }}>
          <span>−50 dB</span>
          <span style={{ color: NcX.accent }}>−21 dB · current</span>
          <span style={{ color: '#ff7a5c' }}>+0 dB · LIM</span>
        </div>

        <div className="italic" style={{ fontSize: 12, opacity: .55, marginTop: 14, lineHeight: 1.4 }}>
          AVLS is engaged. Volumes above 25 / 120 with Stereo Mini are restricted by EU regulation.
        </div>
      </div>
    </div>
  );
}

// ─── Setup Wizard ───────────────────────────────────────────────
function Nc_Wizard() {
  return (
    <div className="scr nocturne">
      <StatusBar badge={<span className="nb-badge">Setup</span>} />
      <div style={{ padding: '14px 30px' }}>
        {/* Step strip */}
        <div className="mono" style={{ display: 'flex', justifyContent: 'space-between', fontSize: 9, letterSpacing: '.14em', marginBottom: 8 }}>
          {WIZARD_STEPS.map((s, i) => (
            <div key={s.key} style={{ flex: 1, textAlign: 'center', color: s.done ? NcX.accent : (i === 2 ? '#fff' : 'rgba(255,255,255,.35)') }}>
              <div style={{ height: 1, background: s.done ? NcX.accent : (i === 2 ? '#fff' : 'rgba(255,255,255,.15)'), marginBottom: 6 }} />
              0{s.n} · {s.label}
            </div>
          ))}
        </div>

        <div className="display" style={{ fontSize: 44, lineHeight: 1.05, marginTop: 28 }}>
          High-Quality<br/><span className="italic">Sound</span>
        </div>
        <div className="serif" style={{ fontSize: 14, opacity: .75, marginTop: 14, lineHeight: 1.55 }}>
          Your Walkman can upscale compressed audio with DSEE HX and balance the analog phase response with DC Phase Linearizer. Turn these on now for the recommended out-of-box sound.
        </div>

        <div style={{ marginTop: 22 }}>
          {[
            { label: 'DSEE HX',             sub: 'Restore high-frequency detail to MP3/AAC.',  on: true },
            { label: 'DC Phase Linearizer', sub: 'Emulate analog-amp phase response.',         on: true },
            { label: 'Dynamic Normalizer',  sub: 'Even out loudness across tracks.',           on: false },
          ].map((f, i) => (
            <div key={f.label} style={{ display: 'flex', alignItems: 'flex-start', gap: 12, padding: '14px 0', borderTop: i === 0 ? `1px solid ${NcX.rule}` : 'none', borderBottom: `1px solid ${NcX.rule}` }}>
              <div style={{ flex: 1 }}>
                <div className="serif" style={{ fontSize: 15 }}>{f.label}</div>
                <div className="italic" style={{ fontSize: 12, opacity: .55, marginTop: 2 }}>{f.sub}</div>
              </div>
              <span style={{ width: 32, height: 18, borderRadius: 10, background: f.on ? NcX.accent : 'rgba(255,255,255,.10)', position: 'relative', flexShrink: 0, marginTop: 3 }}>
                <span style={{ position: 'absolute', top: 2, left: f.on ? 16 : 2, width: 14, height: 14, borderRadius: '50%', background: f.on ? '#0a0a14' : '#fff' }} />
              </span>
            </div>
          ))}
        </div>

        <div style={{ display: 'flex', gap: 8, position: 'absolute', left: 30, right: 30, bottom: 26 }}>
          <button style={{ flex: 1, padding: '14px', background: 'transparent', border: `1px solid ${NcX.rule}`, color: 'inherit', fontFamily: 'inherit', fontSize: 12, letterSpacing: '.08em' }}>Skip</button>
          <button style={{ flex: 2, padding: '14px', background: NcX.accent, color: '#0a0a14', border: 'none', fontFamily: 'inherit', fontSize: 13, fontWeight: 600, letterSpacing: '.06em' }}>Continue →</button>
        </div>
      </div>
    </div>
  );
}

// ─── Night Mode ─────────────────────────────────────────────────
function Nc_Night() {
  return (
    <div className="scr nocturne amoled">
      <div className="status" style={{ color: NcX.accent, opacity: .9 }}>
        <div className="l"><span>14:32</span><span className="mono" style={{ opacity: .55, fontSize: 9 }}>· NIGHT</span></div>
        <div className="r"><span style={{ opacity: .55 }}>78%</span></div>
      </div>

      {/* Clock */}
      <div style={{ padding: '24px 30px 0' }}>
        <div className="display" style={{ fontSize: 96, lineHeight: 1, color: NcX.accent, letterSpacing: '-.04em' }}>14:32</div>
        <div className="italic" style={{ fontSize: 14, opacity: .55, marginTop: 4 }}>Thursday, 27 May</div>
      </div>

      {/* Now playing text-only */}
      <div style={{ padding: '20px 30px 0' }}>
        <NcKicker accent>Now Playing</NcKicker>
        <div className="display" style={{ fontSize: 22, marginTop: 4 }}>{TRACKS[0].title}</div>
        <div className="italic" style={{ fontSize: 13, opacity: .55, marginTop: 2 }}>{TRACKS[0].artist}</div>
      </div>

      {/* Quick access */}
      <div style={{ padding: '24px 30px 0' }}>
        <NcKicker accent style={{ marginBottom: 10 }}>Quick Access</NcKicker>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 8 }}>
          {NIGHT_TILES.map(t => (
            <div key={t.key} style={{ padding: '14px 14px', border: `1px solid rgba(196,182,255,.22)` }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, color: NcX.accent }}>
                {t.key === 'bt'     && <IconBluetooth size={14} />}
                {t.key === 'lib'    && <IconList size={14} />}
                {t.key === 'queue'  && <IconQueue size={14} />}
                {t.key === 'eq'     && <IconSlider size={14} />}
                {t.key === 'vol'    && <IconVolume size={14} />}
                {t.key === 'bright' && <span style={{ fontSize: 14 }}>☼</span>}
                <span className="mono" style={{ fontSize: 9, letterSpacing: '.14em', textTransform: 'uppercase' }}>{t.label}</span>
              </div>
              <div className="serif" style={{ fontSize: 13, marginTop: 6 }}>{t.sub}</div>
            </div>
          ))}
        </div>
      </div>

      <div className="mono" style={{ position: 'absolute', bottom: 18, left: 0, right: 0, textAlign: 'center', fontSize: 9, opacity: .35, letterSpacing: '.24em', color: NcX.accent }}>
        HOLD ANY KEY · EXIT NIGHT MODE
      </div>
    </div>
  );
}

Object.assign(window, {
  Nc_NowPlayingMeter, Nc_TrackDetail, Nc_Lyrics,
  Nc_SoundSettings, Nc_Bluetooth, Nc_BTReceiver, Nc_UsbDac,
  Nc_Output, Nc_Reset, Nc_Volume, Nc_Wizard, Nc_Night,
});
