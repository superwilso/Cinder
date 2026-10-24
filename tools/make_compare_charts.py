#!/usr/bin/env python3
"""Draw docs/compare/*.svg: the measured comparisons in docs/COMPARE_cinder_sony.md.

    python3 tools/make_compare_charts.py

Static SVG, one fixed light surface, so the pictures read the same on a light or a dark GitHub
page. Every number here is copied from the 2026-10-11 measurements in
docs/COMPARE_cinder_sony.md and analysis/RE_clear_bass.md section 6.6; change it there first. The two series colours are the
ones tools were checked with (colour-blind separation and contrast against the surface).
"""
import os

OUT = os.path.join(os.path.dirname(__file__), "..", "docs", "compare")
SURF, INK, DIM, GRID = "#fcfcfb", "#0b0b0b", "#52514e", "#e6e5e0"
BLUE, ORANGE, QUIET = "#2a78d6", "#eb6834", "#c9c8c2"
FONT = "font-family=\"-apple-system,'Segoe UI',Helvetica,Arial,sans-serif\""


def page(w, h, title, sub, body):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" {FONT} role="img" aria-label="{title}">'
            f'<rect width="{w}" height="{h}" rx="8" fill="{SURF}"/>'
            f'<text x="24" y="34" font-size="17" font-weight="600" fill="{INK}">{title}</text>'
            f'<text x="24" y="56" font-size="13" fill="{DIM}">{sub}</text>{body}</svg>')


def hbars(name, title, sub, rows, unit, note):
    """rows: (label, value). One measure, one hue; the labels on the axis carry identity."""
    w, x0, x1, top, pitch, bar = 720, 250, 640, 84, 52, 22
    top_v = max(v for _, v in rows) or 1
    body = f'<line x1="{x0}" y1="{top - 8}" x2="{x0}" y2="{top + pitch * len(rows) - 14}" stroke="{GRID}" stroke-width="1"/>'
    for i, (label, v) in enumerate(rows):
        y = top + i * pitch
        bw = (x1 - x0) * v / top_v
        body += f'<text x="{x0 - 12}" y="{y + 16}" font-size="14" text-anchor="end" fill="{INK}">{label}</text>'
        if bw >= 4:   # rounded at the data end, square on the baseline
            body += f'<path d="M{x0},{y} h{bw - 4} a4,4 0 0 1 4,4 v{bar - 8} a4,4 0 0 1 -4,4 h{-(bw - 4)} z" fill="{BLUE}"/>'
        body += f'<text x="{x0 + max(bw, 0) + 8}" y="{y + 16}" font-size="14" font-weight="600" fill="{INK}">{v:g}{unit}</text>'
    h = top + pitch * len(rows) + 30
    body += f'<text x="24" y="{h - 16}" font-size="12" fill="{DIM}">{note}</text>'
    save(name, page(w, h, title, sub, body))


def columns(name, title, sub, rows, note):
    """rows: (label, dB). Polarity: raised and lowered get a hue each, and a legend."""
    w, h, x0, x1, zero, scale = 720, 380, 70, 690, 222, 16
    body = ""
    for g in (-4, 0, 4, 8):
        y = zero - g * scale
        body += f'<line x1="{x0}" y1="{y}" x2="{x1}" y2="{y}" stroke="{INK if g == 0 else GRID}" stroke-width="1"/>'
        body += f'<text x="{x0 - 8}" y="{y + 4}" font-size="12" text-anchor="end" fill="{DIM}">{g:+d} dB</text>'.replace("+0 dB", "0 dB")
    step = (x1 - x0) / len(rows)
    for i, (label, v) in enumerate(rows):
        cx, bw, bh = x0 + step * (i + 0.5), 56, abs(v) * scale
        if v >= 0:
            body += f'<path d="M{cx - bw / 2},{zero} v{-(bh - 4)} a4,4 0 0 1 4,-4 h{bw - 8} a4,4 0 0 1 4,4 v{bh - 4} z" fill="{BLUE}"/>'
            ty = zero - bh - 8
        else:
            body += f'<path d="M{cx - bw / 2},{zero} v{bh - 4} a4,4 0 0 0 4,4 h{bw - 8} a4,4 0 0 0 4,-4 v{-(bh - 4)} z" fill="{ORANGE}"/>'
            ty = zero + bh + 18
        body += f'<text x="{cx}" y="{ty}" font-size="14" font-weight="600" text-anchor="middle" fill="{INK}">{v:+.1f}</text>'
        body += f'<text x="{cx}" y="{h - 62}" font-size="13" text-anchor="middle" fill="{INK}">{label}</text>'
    body += (f'<rect x="{x0}" y="{h - 40}" width="12" height="12" rx="2" fill="{BLUE}"/><text x="{x0 + 18}" y="{h - 30}" font-size="12" fill="{DIM}">raised</text>'
             f'<rect x="{x0 + 76}" y="{h - 40}" width="12" height="12" rx="2" fill="{ORANGE}"/><text x="{x0 + 94}" y="{h - 30}" font-size="12" fill="{DIM}">lowered</text>'
             f'<text x="{x0 + 170}" y="{h - 30}" font-size="12" fill="{DIM}">{note}</text>')
    save(name, page(w, h, title, sub, body))


def chain(name):
    """The effect chain as each firmware builds it. Identity by label; one accent for Clear Bass."""
    rows = [
        ("Sony A50 1.02", "i2f dseeai heq dynamicnormalizer attn eq6band vpt clearphase alc dcphaselinear vinylizer f2i"),
        ("Walkman One 3.02", "i2f dseehxcustom heq dynamicnormalizer attn eq10band eqtone alc dcphaselinear vinylizer f2i"),
        ("Walkman One + Cinder's patch", "i2f dseehxcustom heq dynamicnormalizer attn eq10band eqtone alc eq6band vinylizer f2i"),
    ]
    w, body, y = 980, "", 84
    for label, names in rows:
        body += f'<text x="24" y="{y}" font-size="14" font-weight="600" fill="{INK}">{label}</text>'
        x = 24
        for n in names.split():
            bw = 12 + 7.2 * len(n)
            hot = n == "eq6band"
            body += (f'<rect x="{x}" y="{y + 10}" width="{bw}" height="28" rx="4" fill="{BLUE if hot else SURF}" stroke="{BLUE if hot else QUIET}"/>'
                     f'<text x="{x + bw / 2}" y="{y + 29}" font-size="12" text-anchor="middle" fill="{SURF if hot else INK}">{n}</text>')
            x += bw + 6
        y += 76
    body += f'<text x="24" y="{y + 4}" font-size="12" fill="{DIM}">eq6band is the six-band equalizer whose first band is Clear Bass. Audio runs left to right.</text>'
    save(name, page(w, y + 24, "Sony's effect chain, by firmware", "The filters the sound service builds at start, in order. Read from each library and from the service's own log.", body))


def stack(name):
    """What Cinder replaces and what it leaves. Two columns, same layers."""
    layers = [("Home app", "what you see and touch", "Sony's player app", "Cinder"),
              ("28 Sony services", "playback, sound, Bluetooth, USB, power", "Sony's", "Sony's, unchanged"),
              ("Sound library on Walkman One", "", "Walkman One's", "one byte changed, if you choose Clear Bass"),
              ("Kernel, drivers, boot loader", "", "Sony's", "Sony's, unchanged")]
    w, body, y = 860, "", 78
    body += f'<text x="300" y="{y}" font-size="13" fill="{DIM}">Sony or Walkman One firmware</text><text x="580" y="{y}" font-size="13" fill="{DIM}">with Cinder installed</text>'
    y += 12
    for what, more, sony, cinder in layers:
        changed = cinder != f"{sony}, unchanged"
        body += (f'<text x="24" y="{y + (24 if more else 31)}" font-size="13" fill="{INK}">{what}</text>'
                 f'<text x="24" y="{y + 41}" font-size="12" fill="{DIM}">{more}</text>'
                 f'<rect x="300" y="{y}" width="260" height="52" rx="4" fill="{SURF}" stroke="{QUIET}"/>'
                 f'<text x="430" y="{y + 31}" font-size="13" text-anchor="middle" fill="{INK}">{sony}</text>'
                 f'<rect x="580" y="{y}" width="260" height="52" rx="4" fill="{BLUE if changed else SURF}" stroke="{BLUE if changed else QUIET}"/>'
                 f'<text x="710" y="{y + 31}" font-size="{12 if len(cinder) > 28 else 13}" text-anchor="middle" fill="{SURF if changed else INK}">{cinder}</text>')
        y += 60
    save(name, page(w, y + 12, "What Cinder replaces", "Cinder is the Home app. The services that decode and process sound are Sony's own in both columns.", body))


def save(name, svg):
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, name), "w") as f:
        f.write(svg + "\n")
    print("wrote docs/compare/" + name)


if __name__ == "__main__":
    hbars("sound_cpu.svg", "Processor use of Sony's sound service while a track plays",
          "Per cent of one processor core. Walkman One player, 3.5 mm jack, 16-bit FLAC, 2026-10-11.",
          [("Direct mode", 0.6), ("Custom: 10-band EQ + normalizer", 10.3)], " %",
          "Busiest thread of the service over 15 s. About half of the Custom figure is the 10-band equalizer.")
    hbars("deep_idle.svg", "Times the processor reached deep idle in 3 minutes of playback",
          "Music on the 3.5 mm jack, screen off, off the cable. Same track, same player, 2026-10-11.",
          [("Bluetooth on, nothing linked", 0), ("Bluetooth off", 291)], "",
          "With the radio on, the kernel refused deep idle 76 times a second. Battery hours are not measured.")
    columns("clear_bass.svg", "What Clear Bass +10 does at the headphone jack",
            "Change against Clear Bass off, pink noise, by frequency band. Walkman One with Cinder's patch, 2026-10-11.",
            [("35 to 56 Hz", 8.7), ("56 to 90 Hz", 7.9), ("90 to 140 Hz", 5.3), ("140 to 300 Hz", -2.3), ("300 Hz to 3 kHz", -2.1)],
            "Recorded through a PC microphone input; one player.")
    chain("effect_chain.svg")
    stack("what_cinder_replaces.svg")
