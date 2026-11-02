#!/usr/bin/env python3
"""How evenly the player sends Bluetooth audio, from Sony's own HCI trace (#44).

    tools/devctl.sh hci on        # mtkbt starts /tmp/hci_sniffer_log_<stamp>.cfa (btsnoop)
    ... play ...
    tools/devctl.sh hci off && tools/devctl.sh hci pull DIR
    python3 analysis/tools/a2dp_timing.py DIR/hci_sniffer_log_*.cfa [--uptime-offset S]

Every A2DP media packet carries an RTP timestamp (audio time). The trace says when mtkbt handed it
to the radio (wall time). Their difference, "lead", is how far ahead of real time the audio has
been sent: flat = even pacing; a drop = a stall (the headphones' buffer drains); a rise = a burst
(it refills faster than real time). Headphones that keep their buffer level by changing playback
speed turn either into an audible speed change. "queued" is ACL packets handed to the controller
and not yet reported sent (Number of Completed Packets): a rise there is the radio not getting
packets out, not the player not producing them.

--uptime-offset S prints each event also as /proc/uptime seconds (S = wall epoch - uptime), so it
can be matched to cinderhome.log.
"""
import struct
import sys
from collections import defaultdict

EPOCH_US = 0x00dcddb30f2f8000   # btsnoop time base (0 AD) to the Unix epoch, in microseconds


def records(path):
    d = open(path, "rb").read()
    if d[:8] != b"btsnoop\0":
        sys.exit("not a btsnoop file: %r" % d[:8])
    off = 16
    while off + 24 <= len(d):
        _o, ilen, flags, _dr, ts = struct.unpack(">IIIIq", d[off:off + 24])
        yield (ts - EPOCH_US) / 1e6, flags & 1, d[off + 24:off + 24 + ilen]
        off += 24 + ilen


def main(argv):
    path = argv[1] if len(argv) > 1 else sys.exit(__doc__)
    up_off = float(argv[argv.index("--uptime-offset") + 1]) if "--uptime-offset" in argv else None
    media = defaultdict(list)          # (handle, cid) -> [(t, seq, rtp_ts, bytes)]
    sent = defaultdict(int)            # handle -> ACL fragments handed to the controller
    done = defaultdict(int)            # handle -> reported completed
    queued = []                        # (t, handle, outstanding)
    for t, rx, p in records(path):
        if not p:
            continue
        if p[0] == 2 and len(p) >= 5 and not rx:
            h = struct.unpack("<H", p[1:3])[0]
            handle, pb = h & 0x0fff, (h >> 12) & 3
            sent[handle] += 1
            queued.append((t, handle, sent[handle] - done[handle]))
            if pb in (0, 2) and len(p) >= 9 + 12:
                cid = struct.unpack("<H", p[7:9])[0]
                r = p[9:]
                if r[0] & 0xc0 == 0x80 and cid >= 0x40:
                    seq, rts = struct.unpack(">HI", r[2:8])
                    media[(handle, cid)].append((t, seq, rts, len(r)))
        elif p[0] == 4 and len(p) >= 4 and p[1] == 0x13:
            for i in range(p[3]):
                handle, n = struct.unpack("<HH", p[4 + 4 * i:8 + 4 * i])
                done[handle & 0x0fff] += n
                queued.append((t, handle & 0x0fff, sent[handle & 0x0fff] - done[handle & 0x0fff]))
    if not media:
        sys.exit("no A2DP media packets in the trace (was anything playing?)")
    key = max(media, key=lambda k: len(media[k]))
    pk = media[key]
    t0, ts0 = pk[0][0], pk[0][2]
    span_t, span_a = pk[-1][0] - t0, (pk[-1][2] - ts0) & 0xffffffff
    # mtkbt's SBC timestamps count PCM bytes (4 a stereo sample: 176400 at 44.1 kHz), not samples.
    rate = min((44100, 48000, 88200, 96000, 176400, 192000),
               key=lambda r: abs(r - span_a / span_t)) if span_t else 44100

    def when(t):
        return "%8.3f s" % (t - t0) + ("  (uptime %.3f)" % (t - up_off) if up_off is not None else "")

    print("media channel: handle 0x%03x cid 0x%04x, %d packets over %.1f s, %d bytes avg, "
          "RTP clock ~%d Hz (measured %.0f)" % (key[0], key[1], len(pk), span_t,
                                                sum(x[3] for x in pk) // len(pk), rate,
                                                span_a / span_t if span_t else 0))
    lost = sum(((b[1] - a[1]) & 0xffff) - 1 for a, b in zip(pk, pk[1:]) if (b[1] - a[1]) & 0xffff != 1)
    print("RTP sequence gaps (packets the stack skipped): %d" % lost)

    lead = [((x[2] - ts0) & 0xffffffff) / rate - (x[0] - t0) for x in pk]
    base = lead[0]
    lead = [(v - base) * 1000 for v in lead]   # ms, relative to the first packet
    # Long-run drift of the sender's audio clock against wall time, in parts per million.
    print("sender clock vs wall: %+.0f ppm over the capture (audio sent %+.1f ms vs real time)"
          % ((span_a / rate - span_t) / span_t * 1e6 if span_t else 0, lead[-1]))

    print("\nper 2 s: packets, audio sent / wall time, lead min..max (ms), worst gap (ms), max queued")
    qi, w, start = 0, 2.0, t0
    i = 0
    while i < len(pk):
        j = i
        while j < len(pk) and pk[j][0] < start + w:
            j += 1
        seg = pk[i:j]
        if seg:
            a = (((seg[-1][2] - seg[0][2]) & 0xffffffff) / rate) if len(seg) > 1 else 0
            gaps = [(b[0] - c[0]) * 1000 for c, b in zip(seg, seg[1:])] or [0]
            if i:
                gaps.append((seg[0][0] - pk[i - 1][0]) * 1000)
            mq = 0
            while qi < len(queued) and queued[qi][0] < start + w:
                if queued[qi][1] == key[0]:
                    mq = max(mq, queued[qi][2])
                qi += 1
            ld = lead[i:j]
            flag = "  <-- stall" if max(gaps) > 150 else ("  <-- burst" if a / w > 1.15 else "")
            print("%s %4d  %5.2f  %7.1f..%7.1f  %6.1f  %3d%s" % (when(start), len(seg), a / w, min(ld),
                                                               max(ld), max(gaps), mq, flag))
        i, start = j, start + w

    print("\nevents (a gap over 100 ms between media packets):")
    k = 0
    for n in range(1, len(pk)):
        g = (pk[n][0] - pk[n - 1][0]) * 1000
        if g > 100:
            print("  %s  gap %.0f ms, lead %.0f -> %.0f ms" % (when(pk[n][0]), g, lead[n - 1], lead[n]))
            k += 1
    if not k:
        print("  none")


if __name__ == "__main__":
    main(sys.argv)
