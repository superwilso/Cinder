// ────────────────────────────────────────────────────────────────
// Direction 3 — TERMINAL · extra screens
// All in pure-mono ASCII vocabulary, amber phosphor on near-black.
// ────────────────────────────────────────────────────────────────

const TmX = {
  bg:       '#0d0d0d',
  phosphor: '#f0a420',
  text:     '#e8e6dc',
  rule:     'rgba(232,230,220,.25)',
  dim:      'rgba(232,230,220,.55)',
};

// Reusable list row in terminal style.
function TmListRow({ label, value, on, type = 'nav', danger = false }) {
  return (
    <div style={{
      display: 'grid', gridTemplateColumns: '1fr auto',
      padding: '8px 0', borderBottom: `1px solid ${TmX.rule}`,
      fontSize: 12, alignItems: 'center',
    }}>
      <span style={{ color: danger ? '#ff6e5e' : 'inherit' }}>
        &gt; {label}
      </span>
      <span style={{ display: 'flex', alignItems: 'center', gap: 6, color: TmX.phosphor, fontSize: 11 }}>
        {type === 'toggle' ? `[${on ? 'X' : ' '}]` : (value ? value : '')}
        {type === 'nav' && <span style={{ opacity: .5 }}>›</span>}
      </span>
    </div>
  );
}

// Title bar reused on every extra screen.
function TmTitle({ title, sub }) {
  return (
    <div style={{ padding: '12px 24px 0' }}>
      <div className="phosphor" style={{ fontSize: 11, letterSpacing: '.16em' }}>// {title}</div>
      {sub && <div style={{ fontSize: 10, opacity: .55, marginTop: 2 }}>{sub}</div>}
    </div>
  );
}

// ─── Now Playing · Spectrum/VU variant ──────────────────────────
function Tm_NowPlayingMeter({ track = TRACKS[0] }) {
  const t = track;
  // 50-cell L/R bar
  const cell = (lit, peak) => ({
    width: 6, height: 14,
    background: lit ? (peak ? '#ff6e5e' : TmX.phosphor) : 'rgba(232,230,220,.12)',
  });
  return (
    <div className="scr terminal">
      <div className="status" style={{ borderBottom: `1px solid ${TmX.rule}`, fontFamily: 'JetBrains Mono, monospace' }}>
        <div className="l" style={{ gap: 12 }}>
          <span className="phosphor">● NW-A55</span>
          <span style={{ opacity: .6 }}>// METER.VIEW</span>
        </div>
        <div className="r"><span>14:32</span><span>78%</span><span className="batt"><i style={{ '--p': '78%' }}/></span></div>
      </div>

      <div style={{ padding: '16px 24px 0' }}>
        <div style={{ fontSize: 9, opacity: .55 }}>&gt; TRACK 03/12 · FLAC 24/96</div>
        <div style={{ fontSize: 22, textTransform: 'uppercase', marginTop: 4, lineHeight: 1.05 }}>{t.title}</div>
        <div style={{ fontSize: 11, opacity: .8, marginTop: 4 }}>BY {t.artist.toUpperCase()}</div>
      </div>

      {/* Big numerical LUFS readout */}
      <div style={{ padding: '20px 24px 0', borderTop: `1px solid ${TmX.rule}`, marginTop: 18 }}>
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em', marginBottom: 4 }}>[ LUFS · INTEGRATED ]</div>
        <div className="phosphor" style={{ fontSize: 48, fontWeight: 400, marginTop: 4, letterSpacing: '-.02em' }}>-14.8</div>
        <div style={{ fontSize: 10, opacity: .65 }}>CREST 14.2 dB · TRUE PEAK -0.4 dBTP</div>
      </div>

      {/* Per-channel meters */}
      <div style={{ padding: '18px 24px 0' }}>
        {[
          { lab: 'L PK',  val: 0.82, peak: '-2.4 dB' },
          { lab: 'L RMS', val: 0.55, peak: '-9.1 dB' },
          { lab: 'R PK',  val: 0.78, peak: '-3.1 dB' },
          { lab: 'R RMS', val: 0.50, peak: '-10.0 dB'},
        ].map(m => (
          <div key={m.lab} style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4, fontSize: 10 }}>
            <span style={{ width: 42 }}>[{m.lab}]</span>
            <span style={{ flex: 1, display: 'flex', gap: 1 }}>
              {Array.from({ length: 48 }).map((_, k) => {
                const lit = k < m.val * 48;
                const peak = k >= 42;
                return <span key={k} style={cell(lit, peak)} />;
              })}
            </span>
            <span className="phosphor" style={{ width: 56, textAlign: 'right' }}>{m.peak}</span>
          </div>
        ))}
      </div>

      {/* Progress at bottom */}
      <div style={{ position: 'absolute', left: 24, right: 24, bottom: 16, fontSize: 11 }}>
        <span>{t.cur} </span>
        <span className="phosphor">[</span>
        <span className="phosphor">██████████</span>
        <span style={{ opacity: .4 }}>░░░░░░░░░░░░░░░░░</span>
        <span className="phosphor">]</span>
        <span> -2:45</span>
      </div>
    </div>
  );
}

// ─── Track Detail ───────────────────────────────────────────────
function Tm_TrackDetail({ track = TRACKS[0] }) {
  const t = track;
  const rows = [
    ['CODEC',        t.codec === 'DSD' ? 'DSD · DSF' : t.codec],
    ['SAMPLE',       t.rate.toUpperCase().replace(' ', '')],
    ['BITS',         t.codec === 'DSD' ? '1' : String(t.bits)],
    ['BITRATE',      `${t.bitrate} kbps`],
    ['CHANNELS',     '2 · STEREO'],
    ['ALBUM',        t.album.toUpperCase()],
    ['ARTIST',       t.artist.toUpperCase()],
    ['YEAR',         '2011'],
    ['TRACK',        '03 / 12'],
    ['DUR',          t.dur],
  ];
  return (
    <div className="scr terminal">
      <TmHeader title="// TRACK.INFO" sub="cat /Music/03.flac" />
      <TmTitle title="TRACK_INFO" />

      <div style={{ padding: '14px 24px 0' }}>
        <div style={{ display: 'grid', gridTemplateColumns: '110px 1fr', gap: 14 }}>
          <Art kind={t.art} size={110} label={false} />
          <div>
            <div className="phosphor" style={{ fontSize: 11 }}>{t.title.toUpperCase()}</div>
            <div style={{ fontSize: 10, opacity: .75, marginTop: 2 }}>BY {t.artist.toUpperCase()}</div>
            <div style={{ fontSize: 10, opacity: .5, marginTop: 2 }}>{t.album.toUpperCase()}</div>
          </div>
        </div>
      </div>

      <div style={{ padding: '18px 24px 0' }}>
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em', marginBottom: 6 }}>[ METADATA ]</div>
        {rows.map(([k, v]) => (
          <div key={k} style={{ display: 'flex', justifyContent: 'space-between', padding: '5px 0', borderBottom: `1px solid ${TmX.rule}`, fontSize: 11 }}>
            <span style={{ opacity: .55 }}>{k}</span>
            <span className="phosphor">{v}</span>
          </div>
        ))}
      </div>

      <div style={{ padding: '14px 24px 0' }}>
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em', marginBottom: 6 }}>[ FILE ]</div>
        <div style={{ fontSize: 10, padding: '6px 8px', border: `1px solid ${TmX.rule}`, wordBreak: 'break-all', lineHeight: 1.5 }}>
          /MUSIC/{t.artist.toUpperCase()}/{t.album.toUpperCase()}/<br/>
          <span className="phosphor">03_{t.title.toUpperCase().replace(/ /g, '_')}.FLAC</span>
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 11, marginTop: 6 }}>
          <span style={{ opacity: .55 }}>SIZE</span>
          <span className="phosphor">84.2 MB · SD CARD</span>
        </div>
      </div>
    </div>
  );
}

// ─── Sync Lyrics ────────────────────────────────────────────────
function Tm_Lyrics({ track = TRACKS[0] }) {
  return (
    <div className="scr terminal">
      <TmHeader title="// SYNC.LYRICS" sub="STREAM 01" />
      <TmTitle title={`${track.title.toUpperCase()} · ${track.artist.toUpperCase()}`} />

      <div style={{ padding: '16px 24px 0', fontSize: 13, lineHeight: 1.55 }}>
        {LYRICS.map((l, i) => {
          const isCur = l.current;
          const dist = Math.abs(LYRICS.findIndex(x => x.current) - i);
          const op = isCur ? 1 : Math.max(0.18, 0.6 - dist * 0.1);
          return (
            <div key={i} style={{ marginBottom: 16, opacity: op, color: isCur ? TmX.phosphor : 'inherit' }}>
              <span style={{ fontSize: 10, opacity: .55, marginRight: 8 }}>[{l.t}]</span>
              {isCur && <span className="phosphor">► </span>}
              {l.line.toUpperCase()}
            </div>
          );
        })}
      </div>

      {/* Mini transport */}
      <div style={{ position: 'absolute', left: 0, right: 0, bottom: 0, padding: '10px 24px', borderTop: `1px solid ${TmX.rule}`, background: '#0d0d0d', fontSize: 11 }}>
        <span>1:47 </span>
        <span className="phosphor">[████████░░░░░░░░░░░░░░░░░░]</span>
        <span> 4:32</span>
        <div style={{ display: 'flex', justifyContent: 'center', gap: 24, marginTop: 6 }}>
          <span>[‹‹]</span>
          <span className="phosphor" style={{ border: `1px solid ${TmX.phosphor}`, padding: '2px 18px' }}>► PLAY</span>
          <span>[››]</span>
        </div>
      </div>
    </div>
  );
}

// ─── Sound Settings ─────────────────────────────────────────────
function Tm_SoundSettings() {
  return (
    <div className="scr terminal">
      <TmHeader title="// SOUND.SETTINGS" sub="EQ:A1 · DSEE:HX" />
      <TmTitle title="SOUND_SETTINGS" />

      <div style={{ padding: '14px 24px 0' }}>
        {SOUND_SETTINGS.map(s => (
          <div key={s.group} style={{ marginBottom: 14 }}>
            <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em', marginBottom: 4 }}>
              [ {s.group.toUpperCase()} ] ────────────────────────────
            </div>
            {s.items.map((it, ii) => (
              <TmListRow key={ii} label={it.label.toUpperCase()} value={it.value?.toUpperCase()} on={it.on} type={it.type} />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Bluetooth pair / LDAC ──────────────────────────────────────
function Tm_Bluetooth() {
  return (
    <div className="scr terminal">
      <TmHeader title="// BT.PAIR" sub="HCI0 · UP" />
      <TmTitle title="BLUETOOTH" sub="STATE: ON · DEV: 1 PAIRED" />

      <div style={{ padding: '14px 24px 0' }}>
        {/* Connected card */}
        <div style={{ padding: '10px 12px', border: `1px solid ${TmX.phosphor}`, background: 'rgba(240,164,32,.08)', marginBottom: 14 }}>
          <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em' }}>[ CONNECTED ]</div>
          <div style={{ fontSize: 14, marginTop: 4 }}>► WH-1000XM5</div>
          <div style={{ fontSize: 10, opacity: .65, marginTop: 2 }}>HEADPHONES · LDAC · 990 KBPS · BATT 92%</div>
        </div>

        {/* LDAC quality */}
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em', marginBottom: 4 }}>
          [ WIRELESS.QUALITY ] ─────────────────────────
        </div>
        {LDAC_QUALITY.map(q => (
          <div key={q.label} style={{ display: 'flex', gap: 10, padding: '7px 0', borderBottom: `1px solid ${TmX.rule}`, fontSize: 11 }}>
            <span className="phosphor" style={{ width: 16 }}>{q.selected ? '(●)' : '( )'}</span>
            <div style={{ flex: 1 }}>
              <div style={{ color: q.selected ? TmX.phosphor : 'inherit' }}>{q.label.toUpperCase()}</div>
              <div style={{ fontSize: 9, opacity: .55, marginTop: 2 }}>{q.sub.toUpperCase()}</div>
            </div>
          </div>
        ))}

        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em', marginTop: 14, marginBottom: 4 }}>
          [ PAIRED.DEVICES ] ──────────────────────────
        </div>
        {BT_DEVICES.filter(d => d.paired && !d.connected).map(d => (
          <div key={d.name} style={{ display: 'grid', gridTemplateColumns: '1fr 60px 14px', padding: '7px 0', borderBottom: `1px solid ${TmX.rule}`, fontSize: 11, alignItems: 'center' }}>
            <div>
              <div>{d.name.toUpperCase()}</div>
              <div style={{ fontSize: 9, opacity: .5 }}>{d.kind.toUpperCase()} · {d.codec}</div>
            </div>
            <span className="phosphor" style={{ fontSize: 10 }}>{'█'.repeat(d.rssi)}{'░'.repeat(4 - d.rssi)}</span>
            <span style={{ opacity: .4 }}>›</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── BT Receiver ────────────────────────────────────────────────
function Tm_BTReceiver() {
  return (
    <div className="scr terminal">
      <TmHeader title="// BT.RECEIVER" sub="MODE: RX" />
      <TmTitle title="BT_RECEIVER" />

      <div style={{ padding: '24px 24px 0', textAlign: 'center' }}>
        <div style={{ border: `1px solid ${TmX.phosphor}`, padding: '18px 12px' }}>
          <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.18em' }}>[ BROADCASTING_AS ]</div>
          <div style={{ fontSize: 26, marginTop: 4 }}>NW-A55</div>
          <div style={{ fontSize: 10, opacity: .55, marginTop: 14 }}>RECEIVING.FROM ↓</div>
          <div style={{ fontSize: 18, marginTop: 4 }}>IPHONE_15_PRO</div>

          {/* Live RX */}
          <div style={{ display: 'flex', gap: 2, justifyContent: 'center', alignItems: 'end', height: 26, marginTop: 16 }}>
            {[40, 55, 35, 70, 80, 50, 30, 60, 75, 45, 28, 40, 55, 35, 20, 30].map((h, i) => (
              <div key={i} style={{ width: 6, height: `${h}%`, background: TmX.phosphor, opacity: i < 11 ? 1 : .25 }} />
            ))}
          </div>
          <div className="phosphor" style={{ fontSize: 10, marginTop: 8 }}>RX · LDAC · 990 KBPS</div>
        </div>
      </div>

      <div style={{ padding: '18px 24px 0' }}>
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em', marginBottom: 4 }}>
          [ SOURCE.DEVICES ] ──────────────────────────
        </div>
        <TmListRow label="IPHONE 15 PRO" value="CONNECTED" />
        <TmListRow label="MACBOOK AIR M3" value="PAIRED" />

        <div style={{ marginTop: 14, padding: '8px 10px', border: `1px dashed ${TmX.rule}`, fontSize: 10, opacity: .65, lineHeight: 1.5 }}>
          NOTE: SOUND.SETTINGS BYPASSED IN RX MODE.
        </div>
      </div>
    </div>
  );
}

// ─── USB-DAC ────────────────────────────────────────────────────
function Tm_UsbDac() {
  return (
    <div className="scr terminal">
      <TmHeader title="// USB.DAC" sub="RX · PCM" />
      <TmTitle title="USB_DAC_MODE" sub="HOST: MBA_M3 · USB-C" />

      <div style={{ padding: '18px 24px 0', textAlign: 'center' }}>
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.18em' }}>[ SAMPLE_RATE ]</div>
        <div className="phosphor" style={{ fontSize: 64, fontWeight: 400, marginTop: 4, letterSpacing: '-.04em' }}>96.0</div>
        <div style={{ fontSize: 11, opacity: .65, marginTop: -4 }}>KHZ · 24 BIT · PCM</div>
      </div>

      <div style={{ padding: '20px 24px 0' }}>
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em', marginBottom: 6 }}>
          [ RECEIVE_LEVEL ] ────────────────────────────
        </div>
        {['L', 'R'].map((ch, i) => (
          <div key={ch} style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4, fontSize: 10 }}>
            <span style={{ width: 16 }}>[{ch}]</span>
            <span style={{ flex: 1, display: 'flex', gap: 1 }}>
              {Array.from({ length: 40 }).map((_, k) => {
                const lit = k < (i === 0 ? 30 : 28);
                const peak = k >= 36;
                return <span key={k} style={{ flex: 1, height: 8, background: lit ? (peak ? '#ff6e5e' : TmX.phosphor) : 'rgba(232,230,220,.12)' }} />;
              })}
            </span>
            <span className="phosphor" style={{ width: 50, textAlign: 'right' }}>{i === 0 ? '-3.1' : '-4.2'}</span>
          </div>
        ))}
      </div>

      <div style={{ padding: '18px 24px 0' }}>
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em', marginBottom: 4 }}>
          [ DAC.SETTINGS ] ─────────────────────────────
        </div>
        <TmListRow label="DAC FILTER" value="SLOW ROLL-OFF" />
        <TmListRow label="CHARGE FROM HOST" type="toggle" on={true} />
        <TmListRow label="DSD OVER PCM (DoP)" type="toggle" on={true} />
      </div>

      <div style={{ position: 'absolute', left: 24, right: 24, bottom: 18, fontSize: 10, opacity: .65, border: `1px dashed ${TmX.rule}`, padding: '8px' }}>
        ⚠ SOUND.SETTINGS BYPASSED IN USB-DAC OUTPUT MODE.
      </div>
    </div>
  );
}

// ─── Output Routing ─────────────────────────────────────────────
function Tm_Output() {
  return (
    <div className="scr terminal">
      <TmHeader title="// OUTPUT" sub="ROUTE / GAIN" />
      <TmTitle title="OUTPUT_ROUTING" />

      <div style={{ padding: '14px 24px 0' }}>
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em', marginBottom: 4 }}>
          [ DESTINATION ] ─────────────────────────────
        </div>
        {[
          { name: '3.5 MM STEREO MINI',  sub: 'ONKYO IE-FC300 DETECTED', active: true,  available: true  },
          { name: '4.4 MM BALANCED',     sub: 'NOT ON THIS MODEL',        active: false, available: false },
          { name: 'BLUETOOTH',           sub: 'WH-1000XM5 · LDAC',         active: false, available: true  },
          { name: 'USB AUDIO',           sub: 'NO HOST CONNECTED',         active: false, available: false },
        ].map(o => (
          <div key={o.name} style={{ display: 'grid', gridTemplateColumns: '24px 1fr', gap: 8, padding: '8px 0', borderBottom: `1px solid ${TmX.rule}`, fontSize: 11, opacity: o.available ? 1 : .35 }}>
            <span className="phosphor">{o.active ? '(●)' : '( )'}</span>
            <div>
              <div style={{ color: o.active ? TmX.phosphor : 'inherit' }}>{o.name}</div>
              <div style={{ fontSize: 9, opacity: .55, marginTop: 2 }}>{o.sub}</div>
            </div>
          </div>
        ))}

        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em', marginTop: 14, marginBottom: 4 }}>
          [ GAIN ] ─────────────────────────────────────
        </div>
        <TmListRow label="HIGH GAIN · STEREO MINI" type="toggle" on={false} />
        <TmListRow label="HIGH GAIN · BALANCED"    type="toggle" on={false} />

        <div style={{ marginTop: 12, fontSize: 10, opacity: .65 }}>
          HIGH GAIN +6 dB FOR LOW-SENS HP. MAY RAISE NOISE FLOOR.
        </div>
      </div>
    </div>
  );
}

// ─── Reset / Format ─────────────────────────────────────────────
function Tm_Reset() {
  return (
    <div className="scr terminal">
      <TmHeader title="// RESET.FORMAT" sub="!!! DESTRUCTIVE" />
      <TmTitle title="RESET / FORMAT" />

      <div style={{ padding: '14px 24px 0' }}>
        {RESET_ITEMS.map(r => (
          <div key={r.label} style={{ padding: '10px 0', borderBottom: `1px solid ${TmX.rule}` }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
              <span style={{ fontSize: 12, color: r.destructive ? '#ff6e5e' : 'inherit' }}>
                &gt; {r.label.toUpperCase()}
              </span>
              <span className="phosphor" style={{ fontSize: 10 }}>{r.destructive ? '[!]' : '[›]'}</span>
            </div>
            <div style={{ fontSize: 10, opacity: .55, marginTop: 4, lineHeight: 1.4 }}>{r.desc.toUpperCase()}</div>
          </div>
        ))}

        <div style={{ marginTop: 14, padding: '10px', border: `1px solid #ff6e5e`, background: 'rgba(255,110,94,.06)' }}>
          <div style={{ fontSize: 10, color: '#ff6e5e', letterSpacing: '.14em' }}>⚠ WARNING</div>
          <div style={{ fontSize: 10, marginTop: 4, opacity: .85, lineHeight: 1.4 }}>
            STORAGE FORMATS ARE PERMANENT.<br/>
            REBUILDING DB ~ 4 MIN ON FULL 64 GB SD.
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── Volume Popup ───────────────────────────────────────────────
function Tm_Volume() {
  return (
    <div className="scr terminal">
      <div style={{ position: 'absolute', inset: 0, opacity: .25 }}>
        <Tm_NowPlayingHero />
      </div>
      <div style={{ position: 'absolute', inset: 0, background: 'rgba(13,13,13,.7)' }} />

      <div style={{
        position: 'absolute', left: 24, right: 24, top: 240,
        padding: '20px',
        background: TmX.bg,
        border: `1px solid ${TmX.phosphor}`,
      }}>
        <div className="phosphor" style={{ fontSize: 10, letterSpacing: '.18em' }}>[ VOLUME ]</div>
        <div className="phosphor" style={{ fontSize: 56, fontWeight: 400, marginTop: 4, letterSpacing: '-.02em' }}>21<span style={{ opacity: .55, fontSize: 18 }}> /120</span></div>

        <div style={{ display: 'flex', gap: 1, marginTop: 14 }}>
          {Array.from({ length: 30 }).map((_, i) => {
            const lit = i < 18;
            const warn = i >= 25;
            return <div key={i} style={{ flex: 1, height: 16, background: lit ? (warn ? '#ff6e5e' : TmX.phosphor) : 'rgba(232,230,220,.12)' }} />;
          })}
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, marginTop: 10, opacity: .65 }}>
          <span>-50 dB</span>
          <span className="phosphor">-21 dB · CURRENT</span>
          <span style={{ color: '#ff6e5e' }}>+0 dB · LIM</span>
        </div>

        <div style={{ marginTop: 14, fontSize: 10, opacity: .65, lineHeight: 1.45 }}>
          AVLS ENGAGED. EU REGULATION CAPS STEREO MINI &gt; 25/120.
        </div>
      </div>
    </div>
  );
}

// ─── Setup Wizard ───────────────────────────────────────────────
function Tm_Wizard() {
  return (
    <div className="scr terminal">
      <TmHeader title="// SETUP.WIZARD" sub="03 / 04" />
      <TmTitle title="HIGH_QUALITY_SOUND" sub="STEP 03 OF 04" />

      <div style={{ padding: '14px 24px 0' }}>
        {/* Step strip */}
        <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 9 }}>
          {WIZARD_STEPS.map((s, i) => (
            <div key={s.key} style={{ flex: 1, textAlign: 'center', color: s.done ? TmX.phosphor : (i === 2 ? TmX.text : TmX.dim) }}>
              <div style={{ height: 1, background: s.done ? TmX.phosphor : (i === 2 ? TmX.text : TmX.rule), marginBottom: 4 }} />
              [0{s.n}] {s.label.toUpperCase()}
            </div>
          ))}
        </div>

        <div style={{ fontSize: 22, marginTop: 24, textTransform: 'uppercase', lineHeight: 1.05 }}>
          HIGH-QUALITY<br/>SOUND
        </div>
        <div style={{ fontSize: 11, opacity: .75, marginTop: 12, lineHeight: 1.55 }}>
          ENABLE DSEE HX TO RESTORE HIGH-FREQ DETAIL ON COMPRESSED SOURCES.
          ENABLE DC PHASE LINEARIZER TO EMULATE ANALOG-AMP PHASE RESPONSE.
        </div>

        <div style={{ marginTop: 20 }}>
          {[
            { label: 'DSEE HX',                 sub: 'RESTORE HIGH-FREQ DETAIL.',   on: true },
            { label: 'DC PHASE LINEARIZER',     sub: 'ANALOG-AMP PHASE EMULATION.', on: true },
            { label: 'DYNAMIC NORMALIZER',      sub: 'EVEN OUT LOUDNESS.',          on: false },
          ].map(f => (
            <div key={f.label} style={{ display: 'flex', justifyContent: 'space-between', padding: '10px 0', borderBottom: `1px solid ${TmX.rule}`, fontSize: 11 }}>
              <div>
                <div>&gt; {f.label}</div>
                <div style={{ fontSize: 9, opacity: .55, marginTop: 2 }}>{f.sub}</div>
              </div>
              <span className="phosphor" style={{ marginTop: 2 }}>[{f.on ? 'X' : ' '}]</span>
            </div>
          ))}
        </div>

        <div style={{ position: 'absolute', left: 24, right: 24, bottom: 24, display: 'flex', gap: 6 }}>
          <div style={{ flex: 1, padding: '12px', border: `1px solid ${TmX.rule}`, textAlign: 'center', fontSize: 11 }}>[SKIP]</div>
          <div style={{ flex: 2, padding: '12px', border: `1px solid ${TmX.phosphor}`, color: TmX.phosphor, textAlign: 'center', fontSize: 11 }}>[CONTINUE ►]</div>
        </div>
      </div>
    </div>
  );
}

// ─── Night Mode ─────────────────────────────────────────────────
function Tm_Night() {
  return (
    <div className="scr terminal" style={{ background: '#000' }}>
      <div className="status" style={{ color: TmX.phosphor, fontFamily: 'JetBrains Mono, monospace', borderBottom: `1px solid rgba(240,164,32,.2)` }}>
        <div className="l" style={{ gap: 12 }}><span>● NW-A55</span><span style={{ opacity: .65 }}>NIGHT.MODE</span></div>
        <div className="r"><span style={{ opacity: .65 }}>78%</span></div>
      </div>

      <div style={{ padding: '20px 24px 0' }}>
        <div className="phosphor" style={{ fontSize: 64, fontWeight: 400, letterSpacing: '-.02em', lineHeight: 1 }}>14:32</div>
        <div style={{ fontSize: 11, opacity: .55, letterSpacing: '.2em', marginTop: 4 }}>THU.27.MAY</div>
      </div>

      <div style={{ padding: '20px 24px 0' }}>
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em' }}>[ NOW_PLAYING ]</div>
        <div style={{ fontSize: 16, marginTop: 4 }}>{TRACKS[0].title.toUpperCase()}</div>
        <div style={{ fontSize: 11, opacity: .55, marginTop: 2 }}>{TRACKS[0].artist.toUpperCase()}</div>
        <div style={{ marginTop: 8, fontSize: 11 }}>
          <span className="phosphor">[█████░░░░░░░░░]</span>
          <span style={{ opacity: .55 }}> 1:47/4:32</span>
        </div>
      </div>

      <div style={{ padding: '22px 24px 0' }}>
        <div className="phosphor" style={{ fontSize: 9, letterSpacing: '.16em', marginBottom: 10 }}>[ QUICK_ACCESS ]</div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 4 }}>
          {NIGHT_TILES.map(t => (
            <div key={t.key} style={{ padding: '12px', border: `1px solid rgba(240,164,32,.3)`, background: 'rgba(240,164,32,.04)' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, color: TmX.phosphor, fontSize: 10 }}>
                <span>[{t.label.toUpperCase()}]</span>
              </div>
              <div style={{ fontSize: 12, marginTop: 6 }}>{t.sub}</div>
            </div>
          ))}
        </div>
      </div>

      <div style={{ position: 'absolute', bottom: 18, left: 0, right: 0, textAlign: 'center' }}>
        <div className="phosphor" style={{ fontSize: 9, opacity: .55, letterSpacing: '.22em' }}>
          ▼ HOLD ANY KEY · EXIT
        </div>
      </div>
    </div>
  );
}

Object.assign(window, {
  Tm_NowPlayingMeter, Tm_TrackDetail, Tm_Lyrics,
  Tm_SoundSettings, Tm_Bluetooth, Tm_BTReceiver, Tm_UsbDac,
  Tm_Output, Tm_Reset, Tm_Volume, Tm_Wizard, Tm_Night,
});
