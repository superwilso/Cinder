#!/usr/bin/env python3
"""Decode the HCI trace Sony's own stack writes.

`BtCommonServiceClient::SetHciLogEnabled(true)` (client vtable slot 26, or
`cinder-probe --btlink hci on`) makes mtkbt open /tmp/hci_sniffer_log_<stamp>.cfa. Despite the
extension that file is a **plain btsnoop** capture (magic "btsnoop\\0", version 1, datalink 1002 =
HCI UART H4), so Wireshark opens it directly — this script is for when you want the answer in a
terminal instead.

    adb pull /tmp/hci_sniffer_log_20260819091929.cfa
    python3 analysis/tools/btsnoop_decode.py hci_sniffer_log_20260819091929.cfa

Reading a reconnect: `Create Connection` -> `Connection Complete status=0x02` is a PAGE TIMEOUT,
i.e. the peer never answered (switched off, out of range). `Write Scan Enable 0x02` = page scan on,
inquiry scan off — connectable but not discoverable; 0x00 disables both, which the stack does for
the duration of each outgoing connection attempt.
"""
import struct
import sys

EVENTS = {
    0x03: "Connection Complete", 0x04: "Connection Request", 0x05: "Disconnection Complete",
    0x06: "Authentication Complete", 0x07: "Remote Name Request Complete",
    0x08: "Encryption Change", 0x0b: "Read Remote Supported Features",
    0x0c: "Read Remote Version Complete", 0x0e: "Command Complete", 0x0f: "Command Status",
    0x13: "Number of Completed Packets", 0x17: "Link Key Request", 0x18: "Link Key Notification",
    0x1a: "Mode Change", 0x1b: "Max Slots Change", 0x2c: "Synchronous Connection Complete",
    0x2f: "Extended Inquiry Result", 0x30: "Encryption Key Refresh", 0x38: "Link Supervision Timeout Changed",
}
COMMANDS = {
    0x0401: "Inquiry", 0x0402: "Inquiry Cancel", 0x0405: "Create Connection", 0x0406: "Disconnect",
    0x0408: "Create Connection Cancel", 0x0409: "Accept Connection Request",
    0x040b: "Authentication Requested", 0x0419: "Remote Name Request", 0x041d: "Read Remote Version",
    0x0c05: "Set Event Filter", 0x0c13: "Write Local Name", 0x0c1a: "Write Scan Enable",
    0x0c2d: "Read Transmit Power Level", 0x0c45: "Write Inquiry Mode", 0x0c52: "Write EIR",
    0x0c56: "Write Simple Pairing Mode", 0x1403: "Read Link Quality", 0x1405: "Read RSSI",
}
# HCI error codes worth naming; everything else prints as a number.
STATUS = {
    0x00: "success", 0x02: "PAGE TIMEOUT (peer did not answer)", 0x04: "page timeout / conn timeout",
    0x05: "authentication failure", 0x06: "PIN or key missing", 0x08: "connection timeout",
    0x0c: "command disallowed", 0x13: "remote user terminated", 0x16: "connection terminated by host",
    0x1f: "unspecified error", 0x3c: "advertising timeout",
}


def bdaddr(b):
    return ":".join("%02X" % x for x in reversed(b[:6]))


def main(path):
    d = open(path, "rb").read()
    if d[:8] != b"btsnoop\0":
        sys.exit("not a btsnoop file: %r" % d[:8])
    ver, link = struct.unpack(">II", d[8:16])
    print("btsnoop v%d datalink %d%s  %d bytes" % (ver, link, " (H4)" if link == 1002 else "", len(d)))
    off, t0 = 16, None
    while off + 24 <= len(d):
        _olen, ilen, _flags, _drops = struct.unpack(">IIII", d[off:off + 16])
        ts, = struct.unpack(">q", d[off + 16:off + 24])
        pkt = d[off + 24:off + 24 + ilen]
        off += 24 + ilen
        if not pkt:
            continue
        if t0 is None:
            t0 = ts
        rel = (ts - t0) / 1e6
        kind = pkt[0]
        if kind == 1 and len(pkt) >= 4:                      # HCI command
            op = struct.unpack("<H", pkt[1:3])[0]
            extra = ""
            if op == 0x0405 and len(pkt) >= 10:
                extra = "-> " + bdaddr(pkt[4:10])
            elif op == 0x0c1a and len(pkt) >= 5:
                se = pkt[4]
                extra = "0x%02x (%s)" % (se, {0: "no scans", 1: "inquiry scan", 2: "page scan",
                                              3: "both"}.get(se, "?"))
            print("%9.3f CMD 0x%04x %-26s %s" % (rel, op, COMMANDS.get(op, ""), extra))
        elif kind == 4 and len(pkt) >= 3:                    # HCI event
            code, extra = pkt[1], ""
            if code == 0x0e and len(pkt) >= 7:
                op = struct.unpack("<H", pkt[4:6])[0]
                extra = "%s status=%s" % (COMMANDS.get(op, "0x%04x" % op),
                                          STATUS.get(pkt[6], "0x%02x" % pkt[6]))
            elif code == 0x0f and len(pkt) >= 7:
                op = struct.unpack("<H", pkt[5:7])[0]
                extra = "%s status=%s" % (COMMANDS.get(op, "0x%04x" % op),
                                          STATUS.get(pkt[3], "0x%02x" % pkt[3]))
            elif code in (0x03, 0x05) and len(pkt) >= 10:
                extra = "status=%s %s" % (STATUS.get(pkt[3], "0x%02x" % pkt[3]),
                                          bdaddr(pkt[6:12]) if code == 0x03 else "")
            print("%9.3f EVT 0x%02x   %-26s %s" % (rel, code, EVENTS.get(code, ""), extra))


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else sys.exit(__doc__))
