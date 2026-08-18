#!/usr/bin/env python3
"""Measure the Walkman's analogue output through the PC's recording input.

WHY THIS EXISTS. Several things on this device accept a write, report success, and do nothing —
high gain was the first (the codec took `high`, read it back, and sounded identical), and
`dacdat ovt` is the current suspect: it returns rc=0, logs nothing to dmesg, and there is no
read-back anywhere for what the volume table actually did. `master volume` and `master gain` are
INPUTS, not the table's output. So the only instrument left is the analogue signal itself.

Wire the Walkman's headphone out to the PC's line/mic in, play a known tone or track, and run this.
It reports level and spectrum.

CAPTURE BACKEND. Two exist, and the default is the one that actually works here:

  * `--backend ffmpeg` (DEFAULT) drives Windows' own ffmpeg.exe over WSL interop against a
    DirectShow device. MEASURED 2026-08-18: music through the cable read RMS -35.6 dBFS, peak
    -23.5 dBFS — real signal with headroom.
  * `--backend pulse` uses WSLg's PulseAudio bridge (`RDPSource`). On this machine that bridge
    returns a DEAD stream: RMS -93.66 dBFS, bit-identical at every Walkman volume from 1 to 70,
    i.e. not quiet audio but no audio. Kept because it needs no Windows interop, but do not trust
    a silent result from it — cross-check with ffmpeg before concluding the device is silent.

    python3 tools/measure_output.py --seconds 5 --label "A50 curve, vol 100"
    python3 tools/measure_output.py --seconds 5 --label "WM1A curve, vol 100" --compare out/A50*.json

READ THE CAVEATS BEFORE TRUSTING A NUMBER:

  * The bridge is MONO, 44.1 kHz, s16le. Channel balance cannot be measured this way; a stereo
    interface is the only honest way to check L/R.
  * A PC mic input is not a measurement instrument. It has its own gain, its own noise floor
    (~-94 dBFS was measured on this machine with nothing connected), and often a DC-blocking
    filter that rolls off the bottom octave. Treat ABSOLUTE numbers as meaningless.
  * RELATIVE numbers are the point. Two captures at the same volume step, same track, same input
    gain, differing only in which table is loaded — that difference is real. Change one thing.
  * Windows may apply "audio enhancements" or AGC to the recording device. AGC will silently
    flatten exactly the level differences being measured. Turn it off in Sound Settings first.
"""

import argparse
import json
import math
import os
import struct
import subprocess
import sys
import time

RATE = 44100
PREFIX = os.path.expanduser("~/.local/cinder-pa")
# The DirectShow name as ffmpeg reports it (`-list_devices true -f dshow -i dummy`).
DSHOW_DEFAULT = "Microphone (Realtek(R) Audio)"


def tool(name: str) -> list:
    """Prefer a system binary; fall back to the locally unpacked prefix."""
    local = os.path.join(PREFIX, "usr/bin", name)
    if os.path.exists(local):
        return [local]
    from shutil import which
    p = which(name)
    if p:
        return [p]
    sys.exit(f"{name} not found. Either `apt install pulseaudio-utils`, or unpack it locally:\n"
             f"  apt-get download pulseaudio-utils libpulse0 libsndfile1 libasyncns0 libflac12t64\n"
             f"  for d in *.deb; do dpkg-deb -x $d {PREFIX}; done")


def env() -> dict:
    e = dict(os.environ)
    libs = [os.path.join(PREFIX, "usr/lib/x86_64-linux-gnu"),
            os.path.join(PREFIX, "usr/lib/x86_64-linux-gnu/pulseaudio")]
    e["LD_LIBRARY_PATH"] = ":".join(libs + [e.get("LD_LIBRARY_PATH", "")])
    return e


def win_path(p: str) -> str:
    """/mnt/c/... -> C:\\... so a Windows binary can be handed the same file."""
    if p.startswith("/mnt/") and len(p) > 6:
        return p[5].upper() + ":" + p[6:].replace("/", "\\")
    return p


def read_wav(path: str) -> list:
    """Mono samples from a WAV, averaging channels if it is stereo."""
    import wave
    with wave.open(path, "rb") as w:
        if w.getsampwidth() != 2:
            sys.exit(f"{path}: expected 16-bit, got {w.getsampwidth() * 8}-bit")
        ch = w.getnchannels()
        raw = w.readframes(w.getnframes())
    vals = list(struct.unpack(f"<{len(raw) // 2}h", raw))
    if ch == 1:
        return vals
    # Average rather than take one side: the bridge is often a mono jack fed from a stereo out,
    # and picking channel 0 alone would under-read anything panned right.
    return [(vals[i] + vals[i + 1]) // 2 for i in range(0, len(vals) - 1, 2)]


def capture_ffmpeg(seconds: float, device: str) -> list:
    """Record via Windows' ffmpeg.exe. The WAV lands on the Windows side so both can reach it."""
    out_win = r"C:\Users\walkman\Downloads\cinder_measure.wav"
    out_wsl = "/mnt/c/Users/walkman/Downloads/cinder_measure.wav"
    ff = subprocess.run(["powershell.exe", "-NoProfile", "-Command",
                         "(Get-Command ffmpeg).Source"],
                        capture_output=True, text=True).stdout.strip()
    if not ff:
        sys.exit("ffmpeg.exe not found on the Windows side — install it, or use --backend pulse")
    cmd = (f"& '{ff}' -hide_banner -loglevel error -f dshow "
           f"-i audio='{device}' -t {seconds} -y '{out_win}'")
    r = subprocess.run(["powershell.exe", "-NoProfile", "-Command", cmd],
                       capture_output=True, text=True, timeout=seconds + 60)
    if not os.path.exists(out_wsl):
        sys.exit(f"ffmpeg captured nothing.\n{r.stderr.strip()[:400]}")
    return read_wav(out_wsl)


def capture(seconds: float, source: str) -> list:
    cmd = tool("parecord") + [f"--device={source}", "--format=s16le", f"--rate={RATE}",
                              "--channels=1", "--raw"]
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=env())
    want = int(RATE * seconds) * 2
    buf = b""
    deadline = time.time() + seconds + 5
    while len(buf) < want and time.time() < deadline:
        chunk = p.stdout.read(want - len(buf))
        if not chunk:
            break
        buf += chunk
    p.terminate()
    p.wait()
    n = len(buf) // 2
    if n == 0:
        sys.exit("captured nothing — is the recording device selected in Windows?")
    return list(struct.unpack(f"<{n}h", buf[:n * 2]))


def dbfs(x: float) -> float:
    return 20 * math.log10(x / 32768.0) if x > 0 else -999.0


def dft_bands(samples: list) -> list:
    """Third-octave band energies from 31.5 Hz to 16 kHz.

    Uses a plain Goertzel per band centre rather than a full FFT: no numpy dependency, and the
    band centres are what gets compared anyway. Slower, but this runs once per capture.
    """
    centres = [31.5, 63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000]
    # Work on a window to cut spectral leakage; a plain rectangular window smears a tone badly.
    n = min(len(samples), RATE * 2)
    w = [0.5 - 0.5 * math.cos(2 * math.pi * i / (n - 1)) for i in range(n)]
    xs = [samples[i] * w[i] for i in range(n)]
    out = []
    for f in centres:
        k = 2 * math.pi * f / RATE
        cw, sw = math.cos(k), math.sin(k)
        coeff = 2 * cw
        s1 = s2 = 0.0
        for v in xs:
            s0 = v + coeff * s1 - s2
            s2, s1 = s1, s0
        re = s1 - s2 * cw
        im = s2 * sw
        mag = math.sqrt(re * re + im * im) * 2 / n
        out.append((f, dbfs(mag)))
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seconds", type=float, default=5.0)
    ap.add_argument("--backend", choices=["ffmpeg", "pulse"], default="ffmpeg")
    ap.add_argument("--device", default=DSHOW_DEFAULT, help="DirectShow capture device name")
    ap.add_argument("--wav", help="analyse an existing WAV instead of capturing")
    ap.add_argument("--source", default="RDPSource", help="pulse backend only")
    ap.add_argument("--label", default="capture")
    ap.add_argument("--out", default="artifacts/measure")
    ap.add_argument("--compare", help="an earlier .json to diff against")
    args = ap.parse_args()

    if args.wav:
        print(f"analysing {args.wav} …")
        s = read_wav(args.wav)
    elif args.backend == "ffmpeg":
        print(f"capturing {args.seconds:.1f}s from '{args.device}' via Windows ffmpeg …")
        s = capture_ffmpeg(args.seconds, args.device)
    else:
        print(f"capturing {args.seconds:.1f}s from {args.source} (WSLg pulse) …")
        s = capture(args.seconds, args.source)
    rms = math.sqrt(sum(v * v for v in s) / len(s))
    peak = max(abs(v) for v in s)
    bands = dft_bands(s)

    rec = {"label": args.label, "seconds": len(s) / RATE, "rms_dbfs": round(dbfs(rms), 2),
           "peak_dbfs": round(dbfs(peak), 2), "clipped": peak >= 32767,
           "bands": {str(f): round(d, 2) for f, d in bands}}

    print(f"  RMS  {rec['rms_dbfs']:+7.2f} dBFS")
    print(f"  peak {rec['peak_dbfs']:+7.2f} dBFS" + ("   *** CLIPPING ***" if rec["clipped"] else ""))
    if rec["rms_dbfs"] < -80:
        print("  ! that is the noise floor — nothing is reaching the input")
        if args.backend == "pulse":
            print("  ! the WSLg pulse bridge reads dead on this machine — retry with --backend ffmpeg")
    for f, d in bands:
        print(f"    {f:>7.1f} Hz  {d:+7.2f} dBFS")

    os.makedirs(args.out, exist_ok=True)
    path = os.path.join(args.out, args.label.replace(" ", "_").replace(",", "") + ".json")
    with open(path, "w") as fh:
        json.dump(rec, fh, indent=2)
    print(f"  saved {path}")

    if args.compare:
        with open(args.compare) as fh:
            old = json.load(fh)
        print(f"\n  vs {old['label']}:")
        print(f"    RMS  {rec['rms_dbfs'] - old['rms_dbfs']:+.2f} dB")
        print(f"    peak {rec['peak_dbfs'] - old['peak_dbfs']:+.2f} dB")
        for f, d in bands:
            o = old["bands"].get(str(f))
            if o is not None:
                print(f"    {f:>7.1f} Hz  {d - o:+6.2f} dB")
        print("\n  Only trust these if the ONLY thing that changed between the two captures was the\n"
              "  setting under test — same track, same position, same volume step, same input gain.")


if __name__ == "__main__":
    main()
