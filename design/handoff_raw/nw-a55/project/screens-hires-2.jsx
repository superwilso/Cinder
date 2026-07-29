// ────────────────────────────────────────────────────────────────
// Direction 1 — HI-RES · extra screens
// Sony NW-A55 firmware screens not in the base set:
//   NP-Meter, USB-DAC, BT Pair, BT Receiver, Track Detail,
//   Sync Lyrics, Sound Settings, Reset/Format, Output Routing,
//   Setup Wizard, Volume Popup, Night Mode.
// ────────────────────────────────────────────────────────────────

const HrX = {
  gold: '#d4a955',
  textDim: 'rgba(230,227,220,.55)',
  surface: 'rgba(255,255,255,.05)',
  border: 'rgba(255,255,255,.08)',
};

// Small reusable list-row used across settings-y screens.
function HrRow({ label, value, on, type = 'nav', accent = false, danger = false }) {
  return (
    <div style={{
      display: 'flex', alignItems: 'center', justifyContent: 'space-between',
      padding: '11px 0', borderBottom: `1px solid ${HrX.border}`,
    }}>
      <span style={{ fontSize: 13, fontWeight: 500, color: danger ? '#e0746b' : (accent ? HrX.gold : 'inherit') }}>{label}</span>
      <span style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
        {type === 'toggle' ? (
          <span style={{ width: 32, height: 18, borderRadius: 10, background: on ? HrX.gold : 'rgba(255,255,255,.12)', position: 'relative' }}>
            <span style={{ position: 'absolute', top: 2, left: on ? 16 : 2, width: 14, height: 14, borderRadius: '50%', background: on ? '#1a1612' : '#fff' }} />
          </span>
        ) : type === 'value' ? (
          <span className="mono" style={{ fontSize: 11, opacity: .6 }}>{value}</span>
        ) : type === 'action' ? null : (
          <>
            {value && <span className="mono" style={{ fontSize: 11, opacity: .6 }}>{value}</span>}
            <IconChevron size={14} style={{ opacity: .4 }} />
          </>
        )}
      </span>
    </div>
  );
}

// ─── Now Playing · Analog VU Meter ──────────────────────────────
function HiRes_NowPlayingMeter({ track = TRACKS[0] }) {
  const t = track;
  // Two analog-style needles, slightly offset (L slightly hotter)
  const needles = [{ side: 'L', val: 0.78, peak: '-2.4' }, { side: 'R', val: 0.72, peak: '-3.1' }];
  return (
    <div className="scr hires">
      <StatusBar
        badge={<span className="hires-badge">Hi-Res</span>}
        right={<span className="mono" style={{ opacity: .55, fontSize: 9 }}>VU · ANALOG</span>}
      />

      {/* Two stacked analog VU meters */}
      <div style={{ padding: '18px 24px 0' }}>
        {needles.map((n, i) => {
          // Needle rotation: -55° at 0, +55° at clip. Map val (0..1) → -55..+55
          const rot = -55 + n.val * 110;
          return (
            <div key={n.side} style={{
              background: 'linear-gradient(180deg, #e8dcc4 0%, #c8b88c 100%)',
              padding: '12px 16px 8px',
              marginBottom: 10,
              border: `1px solid ${HrX.border}`,
              position: 'relative',
              height: 140,
              color: '#1a1612',
              fontFamily: 'JetBrains Mono, monospace',
            }}>
              <div style={{ fontSize: 9, letterSpacing: '.16em', position: 'absolute', top: 8, left: 14 }}>VU {n.side}</div>
              <div style={{ fontSize: 9, letterSpacing: '.16em', position: 'absolute', top: 8, right: 14 }}>PEAK {n.peak} dB</div>

              {/* Scale marks (semicircle) */}
              <svg viewBox="0 0 200 80" width="100%" height="100" style={{ position: 'absolute', left: 0, right: 0, bottom: 0 }} preserveAspectRatio="xMidYMid meet">
                {/* arc */}
                <path d="M 20 75 A 80 80 0 0 1 180 75" stroke="#1a1612" strokeWidth="0.6" fill="none" opacity=".35" />
                {/* red zone */}
                <path d="M 130 23 A 80 80 0 0 1 180 75" stroke="#a02020" strokeWidth="2.5" fill="none" />
                {[-20, -10, -7, -5, -3, 0, 3].map((db, idx, arr) => {
                  const ang = (-55 + (idx / (arr.length - 1)) * 110) * Math.PI / 180;
                  const r1 = 70, r2 = 75;
                  const cx = 100, cy = 80;
                  return (
                    <g key={db}>
                      <line x1={cx + r1 * Math.sin(ang)} y1={cy - r1 * Math.cos(ang)} x2={cx + r2 * Math.sin(ang)} y2={cy - r2 * Math.cos(ang)} stroke={db >= 0 ? '#a02020' : '#1a1612'} strokeWidth="0.8" />
                      <text x={cx + (r1 - 6) * Math.sin(ang)} y={cy - (r1 - 6) * Math.cos(ang)} fontSize="5.5" textAnchor="middle" dominantBaseline="middle" fill={db >= 0 ? '#a02020' : '#1a1612'}>{db}</text>
                    </g>
                  );
                })}
                {/* needle */}
                <line x1="100" y1="80" x2={100 + 65 * Math.sin(rot * Math.PI / 180)} y2={80 - 65 * Math.cos(rot * Math.PI / 180)} stroke="#1a1612" strokeWidth="1.4" />
                <circle cx="100" cy="80" r="3" fill="#1a1612" />
              </svg>
            </div>
          );
        })}
      </div>

      {/* Track strip at bottom */}
      <div style={{ position: 'absolute', left: 0, right: 0, bottom: 0, padding: '14px 24px 18px', borderTop: `1px solid ${HrX.border}` }}>
        <div className="mono" style={{ fontSize: 9, color: HrX.gold, letterSpacing: '.14em' }}>TRACK 03 / 12</div>
        <div style={{ fontSize: 18, fontWeight: 600, marginTop: 4, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{t.title}</div>
        <div style={{ fontSize: 11, opacity: .65, marginTop: 2 }}>{t.artist}</div>
        <div className="prog" style={{ marginTop: 10 }}>
          <i style={{ '--p': '38%', background: HrX.gold }} />
        </div>
        <div className="mono" style={{ display: 'flex', justifyContent: 'space-between', fontSize: 9, marginTop: 6, opacity: .55 }}>
          <span>{t.cur}</span><span>−2:45 · FLAC 24/96</span>
        </div>
      </div>
    </div>
  );
}

// ─── Track Detail (codec, bitrate, file path) ────────────────────
function HiRes_TrackDetail({ track = TRACKS[0] }) {
  const t = track;
  return (
    <div className="scr hires">
      <StatusBar badge={<span className="hires-badge">Hi-Res</span>} />
      <div style={{ padding: '14px 24px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 14 }}>
          <IconBack size={18} style={{ opacity: .65 }} />
          <div style={{ fontSize: 22, fontWeight: 600 }}>Track Info</div>
        </div>

        <div style={{ display: 'flex', gap: 16, alignItems: 'flex-start', marginBottom: 18 }}>
          <Art kind={t.art} size={120} label={false} />
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 17, fontWeight: 600, lineHeight: 1.15 }}>{t.title}</div>
            <div style={{ fontSize: 12, opacity: .75, marginTop: 4 }}>{t.artist}</div>
            <div className="mono" style={{ fontSize: 9, opacity: .5, marginTop: 4, letterSpacing: '.08em' }}>{t.album.toUpperCase()} · 2021</div>
          </div>
        </div>

        <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HrX.gold, marginBottom: 6 }}>FORMAT</div>
        {[
          ['Codec',        t.codec === 'DSD' ? 'DSD · DSF' : `${t.codec}`],
          ['Sample rate',  t.rate],
          ['Bit depth',    t.codec === 'DSD' ? '1-bit' : `${t.bits}-bit`],
          ['Bitrate',      t.codec === 'MP3' ? `${t.bitrate} kbps` : `${t.bitrate} kbps avg`],
          ['Channels',     '2 · Stereo'],
        ].map(([k, v]) => (
          <div key={k} style={{ display: 'flex', justifyContent: 'space-between', padding: '8px 0', borderBottom: `1px solid ${HrX.border}` }}>
            <span style={{ fontSize: 12, opacity: .65 }}>{k}</span>
            <span className="mono" style={{ fontSize: 11 }}>{v}</span>
          </div>
        ))}

        <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HrX.gold, marginTop: 16, marginBottom: 6 }}>FILE</div>
        <div style={{ display: 'flex', justifyContent: 'space-between', padding: '8px 0', borderBottom: `1px solid ${HrX.border}` }}>
          <span style={{ fontSize: 12, opacity: .65 }}>Path</span>
          <span className="mono" style={{ fontSize: 10, opacity: .85, maxWidth: 260, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', direction: 'rtl' }}>
            /Music/{t.artist}/{t.album}/03 {t.title}.flac
          </span>
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between', padding: '8px 0', borderBottom: `1px solid ${HrX.border}` }}>
          <span style={{ fontSize: 12, opacity: .65 }}>Size</span>
          <span className="mono" style={{ fontSize: 11 }}>84.2 MB</span>
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between', padding: '8px 0', borderBottom: `1px solid ${HrX.border}` }}>
          <span style={{ fontSize: 12, opacity: .65 }}>Storage</span>
          <span className="mono" style={{ fontSize: 11 }}>SD Card</span>
        </div>
      </div>
    </div>
  );
}

// ─── Sync Lyrics ────────────────────────────────────────────────
function HiRes_Lyrics({ track = TRACKS[0] }) {
  const t = track;
  return (
    <div className="scr hires">
      <StatusBar badge={<span className="hires-badge">Hi-Res</span>} />
      <div style={{ padding: '14px 24px 0' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <IconBack size={18} style={{ opacity: .65 }} />
          <div>
            <div style={{ fontSize: 18, fontWeight: 600, lineHeight: 1.1 }}>{t.title}</div>
            <div style={{ fontSize: 11, opacity: .6 }}>{t.artist}</div>
          </div>
          <span className="hires-badge" style={{ marginLeft: 'auto' }}>SYNCED</span>
        </div>
      </div>

      {/* Lyrics column — soft gradient fade top/bottom */}
      <div style={{ padding: '24px 28px', marginTop: 14, position: 'relative' }}>
        <div style={{ position: 'absolute', top: 0, left: 0, right: 0, height: 40, background: 'linear-gradient(180deg, #0a0b0e, transparent)', pointerEvents: 'none' }} />
        {LYRICS.map((l, i) => {
          const isCur = l.current;
          const dist = Math.abs(LYRICS.findIndex(x => x.current) - i);
          const op = isCur ? 1 : Math.max(0.18, 0.65 - dist * 0.12);
          return (
            <div key={i} style={{
              fontSize: isCur ? 22 : 17, fontWeight: isCur ? 600 : 400,
              lineHeight: 1.35, marginBottom: 18,
              opacity: op, color: isCur ? HrX.gold : 'inherit',
              transition: 'all .4s',
            }}>
              <span className="mono" style={{ fontSize: 9, opacity: .5, letterSpacing: '.08em', marginRight: 10 }}>{l.t}</span>
              {l.line}
            </div>
          );
        })}
      </div>

      {/* Mini transport at bottom */}
      <div style={{ position: 'absolute', left: 0, right: 0, bottom: 0, padding: '12px 24px 14px', background: 'rgba(10,11,14,.85)', backdropFilter: 'blur(8px)', borderTop: `1px solid ${HrX.border}` }}>
        <div className="prog"><i style={{ '--p': '38%', background: HrX.gold }} /></div>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: 8 }}>
          <span className="mono" style={{ fontSize: 9, opacity: .55 }}>1:47</span>
          <div style={{ display: 'flex', gap: 18, alignItems: 'center' }}>
            <IconPrev size={20} />
            <div style={{ width: 36, height: 36, borderRadius: '50%', background: HrX.gold, color: '#1a1612', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
              <IconPause size={16} />
            </div>
            <IconNext size={20} />
          </div>
          <span className="mono" style={{ fontSize: 9, opacity: .55 }}>−2:45</span>
        </div>
      </div>
    </div>
  );
}

// ─── Sound Settings root ────────────────────────────────────────
function HiRes_SoundSettings() {
  return (
    <div className="scr hires">
      <StatusBar badge={<span className="hires-badge">Hi-Res</span>} />
      <div style={{ padding: '14px 24px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 18 }}>
          <IconBack size={18} style={{ opacity: .65 }} />
          <div style={{ fontSize: 22, fontWeight: 600 }}>Sound Settings</div>
        </div>

        {SOUND_SETTINGS.map(s => (
          <div key={s.group} style={{ marginBottom: 18 }}>
            <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HrX.gold, marginBottom: 8 }}>{s.group.toUpperCase()}</div>
            {s.items.map((it, ii) => (
              <HrRow key={ii} {...it} accent={it.type === 'action'} />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Bluetooth pair / LDAC quality ──────────────────────────────
function HiRes_Bluetooth() {
  return (
    <div className="scr hires">
      <StatusBar badge={<span className="hires-badge">Hi-Res</span>} right={<IconBluetooth size={12} />} />
      <div style={{ padding: '14px 24px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 16 }}>
          <IconBack size={18} style={{ opacity: .65 }} />
          <div style={{ fontSize: 22, fontWeight: 600 }}>Bluetooth</div>
          <span style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 8 }}>
            <span className="mono" style={{ fontSize: 10, opacity: .55 }}>ON</span>
            <span style={{ width: 32, height: 18, borderRadius: 10, background: HrX.gold, position: 'relative' }}>
              <span style={{ position: 'absolute', top: 2, left: 16, width: 14, height: 14, borderRadius: '50%', background: '#1a1612' }} />
            </span>
          </span>
        </div>

        {/* Connected device card */}
        <div style={{
          padding: '14px', background: HrX.surface,
          border: `1px solid ${HrX.gold}`, marginBottom: 18,
        }}>
          <div className="mono" style={{ fontSize: 9, color: HrX.gold, letterSpacing: '.14em' }}>CONNECTED</div>
          <div style={{ display: 'flex', alignItems: 'center', marginTop: 6 }}>
            <IconHeadphone size={20} style={{ color: HrX.gold }} />
            <div style={{ marginLeft: 10, flex: 1 }}>
              <div style={{ fontSize: 15, fontWeight: 600 }}>WH-1000XM5</div>
              <div className="mono" style={{ fontSize: 10, opacity: .6, marginTop: 2 }}>LDAC · 990 kbps · 92% battery</div>
            </div>
          </div>
        </div>

        {/* LDAC quality picker */}
        <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HrX.gold, marginBottom: 8 }}>WIRELESS PLAYBACK QUALITY</div>
        {LDAC_QUALITY.map((q, i) => (
          <div key={q.label} style={{
            display: 'flex', alignItems: 'flex-start', gap: 12, padding: '10px 0',
            borderTop: i === 0 ? `1px solid ${HrX.border}` : 'none',
            borderBottom: `1px solid ${HrX.border}`,
          }}>
            <span style={{
              width: 18, height: 18, borderRadius: '50%', border: `1.5px solid ${q.selected ? HrX.gold : 'rgba(255,255,255,.25)'}`,
              display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0, marginTop: 2,
            }}>
              {q.selected && <span style={{ width: 8, height: 8, borderRadius: '50%', background: HrX.gold }} />}
            </span>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 13, fontWeight: 500, color: q.selected ? HrX.gold : 'inherit' }}>{q.label}</div>
              <div style={{ fontSize: 11, opacity: .55, marginTop: 2 }}>{q.sub}</div>
            </div>
          </div>
        ))}

        <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HrX.gold, marginTop: 16, marginBottom: 8 }}>PAIRED DEVICES</div>
        {BT_DEVICES.filter(d => d.paired && !d.connected).map(d => (
          <div key={d.name} style={{ display: 'grid', gridTemplateColumns: '20px 1fr 60px 14px', gap: 10, alignItems: 'center', padding: '9px 0', borderBottom: `1px solid ${HrX.border}` }}>
            <IconHeadphone size={14} style={{ opacity: .55 }} />
            <div>
              <div style={{ fontSize: 12 }}>{d.name}</div>
              <div className="mono" style={{ fontSize: 9, opacity: .45 }}>{d.kind} · {d.codec}</div>
            </div>
            <span className="mono" style={{ fontSize: 9, opacity: .5, textAlign: 'right' }}>{'■'.repeat(d.rssi)}{'□'.repeat(4 - d.rssi)}</span>
            <IconChevron size={12} style={{ opacity: .35 }} />
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── BT Receiver mode ───────────────────────────────────────────
function HiRes_BTReceiver() {
  return (
    <div className="scr hires">
      <StatusBar badge={<span className="hires-badge">Hi-Res</span>} />
      <div style={{ padding: '14px 24px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 18 }}>
          <IconBack size={18} style={{ opacity: .65 }} />
          <div style={{ fontSize: 22, fontWeight: 600 }}>BT Receiver</div>
        </div>

        <div style={{
          padding: '18px', background: HrX.surface,
          border: `1px solid ${HrX.gold}`, marginBottom: 16, textAlign: 'center',
        }}>
          <div className="mono" style={{ fontSize: 9, color: HrX.gold, letterSpacing: '.18em' }}>BROADCASTING AS</div>
          <div style={{ fontSize: 24, fontWeight: 600, marginTop: 6 }}>NW-A55</div>
          <div className="mono" style={{ fontSize: 10, opacity: .6, marginTop: 4 }}>RECEIVING FROM ↓</div>
          <div style={{ fontSize: 16, marginTop: 4 }}>iPhone 15 Pro</div>

          {/* Live receiver level */}
          <div style={{ display: 'flex', gap: 2, justifyContent: 'center', alignItems: 'end', height: 26, marginTop: 14 }}>
            {[40, 55, 35, 70, 80, 50, 30, 60, 75, 45, 28, 40, 55, 35, 20, 30].map((h, i) => (
              <div key={i} style={{ width: 6, height: `${h}%`, background: HrX.gold, opacity: i < 11 ? 1 : .25 }} />
            ))}
          </div>
          <div className="mono" style={{ fontSize: 10, color: HrX.gold, marginTop: 8 }}>RX · LDAC · 990 kbps</div>
        </div>

        <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HrX.gold, marginBottom: 8 }}>RECEIVER PLAYBACK QUALITY</div>
        <HrRow label="Codec preference" value="LDAC > AAC > SBC" />
        <HrRow label="Auto-reconnect" type="toggle" on={true} />

        <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HrX.gold, marginTop: 16, marginBottom: 8 }}>SOURCE DEVICES</div>
        <HrRow label="iPhone 15 Pro" value="Connected" accent />
        <HrRow label="MacBook Air M3" value="Paired" />

        <div style={{ marginTop: 24, padding: '12px', border: `1px dashed ${HrX.border}`, fontSize: 10, opacity: .6 }}>
          Note: Sound Settings do not apply when the Walkman is acting as a Bluetooth receiver.
        </div>
      </div>
    </div>
  );
}

// ─── USB-DAC mode (PC → Walkman as DAC) ─────────────────────────
function HiRes_UsbDac() {
  return (
    <div className="scr hires">
      <StatusBar badge={<span className="hires-badge">USB-DAC</span>} right={<span className="mono" style={{ fontSize: 9, opacity: .55 }}>RX · PCM</span>} />
      <div style={{ padding: '16px 24px' }}>
        <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HrX.gold }}>SOURCE</div>
        <div style={{ fontSize: 13, marginTop: 2 }}>MacBook Air M3 · USB-C</div>

        {/* Big numerical readout */}
        <div style={{ marginTop: 22, textAlign: 'center' }}>
          <div className="mono" style={{ fontSize: 9, opacity: .55, letterSpacing: '.18em' }}>SAMPLE RATE</div>
          <div className="mono" style={{ fontSize: 56, fontWeight: 300, color: HrX.gold, marginTop: 4, letterSpacing: '-.02em' }}>96.0</div>
          <div className="mono" style={{ fontSize: 12, opacity: .55, marginTop: -4 }}>kHz · 24-bit · PCM</div>
        </div>

        {/* Receive-level meter */}
        <div style={{ marginTop: 24 }}>
          <div className="mono" style={{ fontSize: 9, letterSpacing: '.14em', color: HrX.gold, marginBottom: 6 }}>RECEIVE LEVEL</div>
          {['L', 'R'].map((ch, i) => (
            <div key={ch} style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 }}>
              <span className="mono" style={{ fontSize: 10, width: 12, opacity: .55 }}>{ch}</span>
              <div style={{ flex: 1, display: 'flex', gap: 1 }}>
                {Array.from({ length: 32 }).map((_, k) => {
                  const lit = k < (i === 0 ? 24 : 22);
                  const peak = k >= 27;
                  return <div key={k} style={{ flex: 1, height: 8, background: lit ? (peak ? '#a02020' : HrX.gold) : 'rgba(255,255,255,.10)' }} />;
                })}
              </div>
              <span className="mono" style={{ fontSize: 9, opacity: .55, width: 40, textAlign: 'right' }}>{i === 0 ? '-3.1' : '-4.2'}</span>
            </div>
          ))}
        </div>

        {/* DAC settings */}
        <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HrX.gold, marginTop: 22, marginBottom: 6 }}>USB-DAC SETTINGS</div>
        <HrRow label="DAC Filter" value="Slow Roll-off" />
        <HrRow label="Charge from connected device" type="toggle" on={true} />
        <HrRow label="DSD over PCM (DoP)" type="toggle" on={true} />

        <div style={{ marginTop: 14, padding: '10px', border: `1px dashed ${HrX.border}`, fontSize: 10, opacity: .65 }}>
          Sound Settings do not apply during USB audio output.
        </div>
      </div>
    </div>
  );
}

// ─── Output Routing ─────────────────────────────────────────────
function HiRes_Output() {
  return (
    <div className="scr hires">
      <StatusBar badge={<span className="hires-badge">Hi-Res</span>} />
      <div style={{ padding: '14px 24px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 18 }}>
          <IconBack size={18} style={{ opacity: .65 }} />
          <div style={{ fontSize: 22, fontWeight: 600 }}>Output</div>
        </div>

        <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HrX.gold, marginBottom: 8 }}>DESTINATION</div>
        {[
          { name: '3.5 mm Stereo Mini',  sub: 'Onkyo IE-FC300 detected', active: true,  available: true  },
          { name: '4.4 mm Balanced',     sub: 'Not on this model',       active: false, available: false },
          { name: 'Bluetooth',           sub: 'WH-1000XM5 · LDAC',        active: false, available: true  },
          { name: 'USB Audio',           sub: 'No host connected',        active: false, available: false },
        ].map((o, i) => (
          <div key={o.name} style={{
            display: 'flex', alignItems: 'flex-start', gap: 12, padding: '11px 0',
            borderTop: i === 0 ? `1px solid ${HrX.border}` : 'none',
            borderBottom: `1px solid ${HrX.border}`,
            opacity: o.available ? 1 : .35,
          }}>
            <span style={{
              width: 18, height: 18, borderRadius: '50%',
              border: `1.5px solid ${o.active ? HrX.gold : 'rgba(255,255,255,.25)'}`,
              display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0, marginTop: 2,
            }}>
              {o.active && <span style={{ width: 8, height: 8, borderRadius: '50%', background: HrX.gold }} />}
            </span>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 14, fontWeight: 500, color: o.active ? HrX.gold : 'inherit' }}>{o.name}</div>
              <div className="mono" style={{ fontSize: 10, opacity: .55, marginTop: 2 }}>{o.sub}</div>
            </div>
          </div>
        ))}

        <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HrX.gold, marginTop: 18, marginBottom: 8 }}>GAIN</div>
        <HrRow label="High Gain · Stereo Mini" value="Off" type="toggle" on={false} />
        <HrRow label="High Gain · Balanced"    value="Off" type="toggle" on={false} />
        <div style={{ fontSize: 10, opacity: .55, marginTop: 8 }}>
          High Gain raises output by ~6 dB for low-sensitivity headphones. May increase noise floor.
        </div>
      </div>
    </div>
  );
}

// ─── Reset / Format hub ─────────────────────────────────────────
function HiRes_Reset() {
  return (
    <div className="scr hires">
      <StatusBar badge={<span className="hires-badge">Hi-Res</span>} />
      <div style={{ padding: '14px 24px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 18 }}>
          <IconBack size={18} style={{ opacity: .65 }} />
          <div style={{ fontSize: 22, fontWeight: 600 }}>Reset / Format</div>
        </div>

        {RESET_ITEMS.map((r, i) => (
          <div key={r.label} style={{ padding: '14px 0', borderBottom: `1px solid ${HrX.border}`, borderTop: i === 0 ? `1px solid ${HrX.border}` : 'none' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <span style={{ fontSize: 14, fontWeight: 500, color: r.destructive ? '#e0746b' : 'inherit' }}>{r.label}</span>
              <IconChevron size={12} style={{ opacity: .35, marginLeft: 'auto' }} />
            </div>
            <div style={{ fontSize: 10, opacity: .55, marginTop: 4, lineHeight: 1.4 }}>{r.desc}</div>
          </div>
        ))}

        <div style={{
          marginTop: 18, padding: '12px',
          background: 'rgba(224,116,107,.08)', border: `1px solid rgba(224,116,107,.3)`,
        }}>
          <div className="mono" style={{ fontSize: 9, color: '#e0746b', letterSpacing: '.14em' }}>HEADS UP</div>
          <div style={{ fontSize: 11, marginTop: 4, opacity: .85 }}>
            Storage formats are permanent. Rebuilding the database takes ~4 min on a full 64 GB card.
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── Volume popup (overlay) ─────────────────────────────────────
function HiRes_Volume() {
  return (
    <div className="scr hires" style={{ background: '#0a0b0e' }}>
      {/* Dimmed underlayer hinting NP */}
      <div style={{ position: 'absolute', inset: 0, opacity: .25 }}>
        <HiRes_NowPlayingHero />
      </div>
      <div style={{ position: 'absolute', inset: 0, background: 'rgba(5,6,8,.65)', backdropFilter: 'blur(2px)' }} />

      {/* Vol popup card */}
      <div style={{
        position: 'absolute', left: 32, right: 32, top: 240,
        padding: '24px 28px',
        background: '#14161a',
        border: `1px solid ${HrX.border}`,
      }}>
        <div className="mono" style={{ fontSize: 9, letterSpacing: '.18em', color: HrX.gold }}>VOLUME</div>
        <div style={{ display: 'flex', alignItems: 'baseline', gap: 8, marginTop: 6 }}>
          <span className="mono" style={{ fontSize: 56, fontWeight: 300, letterSpacing: '-.02em', color: HrX.gold }}>21</span>
          <span className="mono" style={{ fontSize: 14, opacity: .55 }}>/ 120</span>
        </div>

        {/* 30-cell volume bar */}
        <div style={{ display: 'flex', gap: 2, marginTop: 14 }}>
          {Array.from({ length: 30 }).map((_, i) => {
            const lit = i < 18;
            const warn = i >= 25;
            return <div key={i} style={{ flex: 1, height: 14, background: lit ? (warn ? '#a02020' : HrX.gold) : 'rgba(255,255,255,.08)' }} />;
          })}
        </div>

        <div className="mono" style={{ fontSize: 9, opacity: .55, marginTop: 10, display: 'flex', justifyContent: 'space-between', letterSpacing: '.1em' }}>
          <span>−50 dB</span>
          <span style={{ color: HrX.gold }}>−21 dB · current</span>
          <span style={{ color: '#a02020' }}>+0 dB · LIM</span>
        </div>

        <div style={{ fontSize: 10, opacity: .55, marginTop: 12, lineHeight: 1.4 }}>
          AVLS is engaged. Volumes above 25/120 with Stereo Mini are restricted by EU regulation.
        </div>
      </div>
    </div>
  );
}

// ─── Setup Wizard ───────────────────────────────────────────────
function HiRes_Wizard() {
  return (
    <div className="scr hires">
      <StatusBar badge={<span className="hires-badge">SETUP</span>} />
      <div style={{ padding: '14px 24px' }}>
        {/* Steps strip */}
        <div className="mono" style={{ display: 'flex', justifyContent: 'space-between', fontSize: 9, letterSpacing: '.14em', marginBottom: 16 }}>
          {WIZARD_STEPS.map((s, i) => (
            <div key={s.key} style={{ flex: 1, textAlign: 'center', color: s.done ? HrX.gold : (i === 2 ? '#fff' : 'rgba(255,255,255,.35)') }}>
              <div style={{ height: 2, background: s.done ? HrX.gold : (i === 2 ? '#fff' : 'rgba(255,255,255,.15)'), marginBottom: 6 }} />
              0{s.n} · {s.label.toUpperCase()}
            </div>
          ))}
        </div>

        <div style={{ fontSize: 26, fontWeight: 600, lineHeight: 1.1, marginTop: 16 }}>
          High-Quality Sound
        </div>
        <div style={{ fontSize: 13, opacity: .75, marginTop: 8, lineHeight: 1.45 }}>
          Your Walkman can upscale compressed audio with DSEE HX and balance the analog phase response with DC Phase Linearizer. Turn these on now for the recommended out-of-box sound.
        </div>

        <div style={{ marginTop: 18 }}>
          {[
            { label: 'DSEE HX',                 sub: 'Restore high-frequency detail to MP3/AAC.',  on: true },
            { label: 'DC Phase Linearizer',     sub: 'Emulate analog-amp phase response.',         on: true },
            { label: 'Dynamic Normalizer',      sub: 'Even out loudness across tracks.',           on: false },
          ].map((f, i) => (
            <div key={f.label} style={{ display: 'flex', alignItems: 'flex-start', gap: 12, padding: '12px 0', borderTop: i === 0 ? `1px solid ${HrX.border}` : 'none', borderBottom: `1px solid ${HrX.border}` }}>
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: 14, fontWeight: 500 }}>{f.label}</div>
                <div style={{ fontSize: 11, opacity: .55, marginTop: 2 }}>{f.sub}</div>
              </div>
              <span style={{ width: 32, height: 18, borderRadius: 10, background: f.on ? HrX.gold : 'rgba(255,255,255,.12)', position: 'relative', flexShrink: 0, marginTop: 4 }}>
                <span style={{ position: 'absolute', top: 2, left: f.on ? 16 : 2, width: 14, height: 14, borderRadius: '50%', background: f.on ? '#1a1612' : '#fff' }} />
              </span>
            </div>
          ))}
        </div>

        {/* Footer */}
        <div style={{ display: 'flex', gap: 8, marginTop: 22, position: 'absolute', left: 24, right: 24, bottom: 24 }}>
          <button style={{
            flex: 1, padding: '12px', background: 'transparent',
            border: `1px solid ${HrX.border}`, color: 'inherit',
            fontFamily: 'inherit', fontSize: 12, letterSpacing: '.1em',
          }}>SKIP FOR NOW</button>
          <button style={{
            flex: 2, padding: '12px', background: HrX.gold, color: '#1a1612',
            border: `1px solid ${HrX.gold}`, fontFamily: 'inherit',
            fontSize: 12, fontWeight: 600, letterSpacing: '.1em',
          }}>CONTINUE →</button>
        </div>
      </div>
    </div>
  );
}

// ─── Night Mode ─────────────────────────────────────────────────
// Darkest possible (true black), no album art, quick-access tiles.
function HiRes_Night() {
  return (
    <div className="scr hires" style={{ background: '#000' }}>
      <div className="status" style={{ color: '#d4a955', opacity: .85 }}>
        <div className="l"><span>14:32</span><span className="mono" style={{ opacity: .55, fontSize: 9 }}>· NIGHT MODE</span></div>
        <div className="r"><span style={{ opacity: .65 }}>78%</span></div>
      </div>

      {/* Big clock */}
      <div style={{ padding: '18px 24px 0' }}>
        <div className="mono" style={{ fontSize: 64, fontWeight: 200, color: HrX.gold, letterSpacing: '-.02em', lineHeight: 1 }}>14:32</div>
        <div className="mono" style={{ fontSize: 10, opacity: .55, letterSpacing: '.2em', marginTop: 4 }}>THU · 27 MAY</div>
      </div>

      {/* Now playing text-only */}
      <div style={{ padding: '18px 24px 0' }}>
        <div className="mono" style={{ fontSize: 9, color: HrX.gold, letterSpacing: '.16em' }}>NOW PLAYING</div>
        <div style={{ fontSize: 16, marginTop: 4, opacity: .9 }}>{TRACKS[0].title}</div>
        <div style={{ fontSize: 11, opacity: .55, marginTop: 2 }}>{TRACKS[0].artist}</div>
        <div className="prog" style={{ marginTop: 10, opacity: .35 }}><i style={{ '--p': '38%', background: HrX.gold }} /></div>
      </div>

      {/* Quick-access grid */}
      <div style={{ padding: '22px 24px 0' }}>
        <div className="mono" style={{ fontSize: 9, color: HrX.gold, letterSpacing: '.16em', marginBottom: 10 }}>QUICK ACCESS</div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 8 }}>
          {NIGHT_TILES.map(t => (
            <div key={t.key} style={{
              padding: '14px 14px', border: `1px solid rgba(212,169,85,.25)`,
              background: 'rgba(212,169,85,.04)',
            }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, color: HrX.gold }}>
                {t.key === 'bt'     && <IconBluetooth size={14} />}
                {t.key === 'lib'    && <IconList size={14} />}
                {t.key === 'queue'  && <IconQueue size={14} />}
                {t.key === 'eq'     && <IconSlider size={14} />}
                {t.key === 'vol'    && <IconVolume size={14} />}
                {t.key === 'bright' && <span style={{ fontSize: 14 }}>☼</span>}
                <span className="mono" style={{ fontSize: 10, letterSpacing: '.12em', textTransform: 'uppercase' }}>{t.label}</span>
              </div>
              <div style={{ fontSize: 12, marginTop: 6, opacity: .85 }}>{t.sub}</div>
            </div>
          ))}
        </div>
      </div>

      <div style={{ position: 'absolute', bottom: 18, left: 0, right: 0, textAlign: 'center' }}>
        <div className="mono" style={{ fontSize: 9, opacity: .35, letterSpacing: '.24em', color: HrX.gold }}>
          HOLD ANY KEY · EXIT NIGHT MODE
        </div>
      </div>
    </div>
  );
}

Object.assign(window, {
  HiRes_NowPlayingMeter, HiRes_TrackDetail, HiRes_Lyrics,
  HiRes_SoundSettings, HiRes_Bluetooth, HiRes_BTReceiver,
  HiRes_UsbDac, HiRes_Output, HiRes_Reset,
  HiRes_Volume, HiRes_Wizard, HiRes_Night,
});
