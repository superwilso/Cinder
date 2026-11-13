//! `cinder.local`: answers mDNS questions for that name with the player's Wi-Fi address, so a
//! phone or a PC can open the phone remote (`http://cinder.local:8080`) or the drive share by name
//! instead of by number. Runs only while one of those is serving (see `mdns_tick` in lib.rs).
//!
//! Only the A record, only for this one name: no service records, no probing for a clash.
//! ponytail: two Cinder players on one network both answer `cinder.local`; probe and pick
//! `cinder-2.local` if that ever matters.

use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::os::unix::io::FromRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub const NAME: &str = "cinder.local";
const GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const PORT: u16 = 5353;
const TTL_S: u32 = 120;

static RUN: AtomicBool = AtomicBool::new(false);

pub fn running() -> bool {
    RUN.load(Ordering::SeqCst)
}

pub fn stop() {
    RUN.store(false, Ordering::SeqCst);
}

/// Read one name at `at` (following compression pointers), lower-cased, dot-joined, and the
/// offset just past it in the packet.
fn read_name(p: &[u8], mut at: usize) -> Option<(String, usize)> {
    let mut labels = Vec::new();
    let mut end = None;
    for _ in 0..32 {
        let len = *p.get(at)? as usize;
        if len == 0 {
            return Some((labels.join("."), end.unwrap_or(at + 1)));
        }
        if len & 0xC0 == 0xC0 {
            end.get_or_insert(at + 2);
            at = ((len & 0x3F) << 8) | *p.get(at + 1)? as usize;
            continue;
        }
        let l = p.get(at + 1..at + 1 + len)?;
        labels.push(String::from_utf8_lossy(l).to_ascii_lowercase());
        at += 1 + len;
    }
    None
}

/// The reply to query `q` when it asks for `NAME`'s address (type A or ANY), and whether it must
/// go back to the asker alone. `legacy`: the query came from a port other than 5353 (a plain DNS
/// resolver); RFC 6762 §6.7 wants its id and question echoed and no cache-flush bit.
pub fn answer(q: &[u8], ip: Ipv4Addr, legacy: bool) -> Option<(Vec<u8>, bool)> {
    if q.len() < 12 || q[2] & 0x80 != 0 {
        return None; // short, or a response
    }
    let qd = u16::from_be_bytes([q[4], q[5]]);
    let mut at = 12;
    for _ in 0..qd {
        let (name, next) = read_name(q, at)?;
        let qtype = u16::from_be_bytes([*q.get(next)?, *q.get(next + 1)?]);
        let qclass = u16::from_be_bytes([*q.get(next + 2)?, *q.get(next + 3)?]);
        let question = &q[at..next + 4];
        at = next + 4;
        if name != NAME || !(qtype == 1 || qtype == 255) || qclass & 0x7FFF != 1 {
            continue;
        }
        let unicast = legacy || qclass & 0x8000 != 0;
        let mut r = Vec::with_capacity(64);
        r.extend_from_slice(if legacy { &q[0..2] } else { &[0, 0] });
        r.extend_from_slice(&[0x84, 0x00]); // response, authoritative
        r.extend_from_slice(&[0, legacy as u8, 0, 1, 0, 0, 0, 0]);
        if legacy {
            // The question, with its name written out (it may have pointed into the query).
            for l in NAME.split('.') {
                r.push(l.len() as u8);
                r.extend_from_slice(l.as_bytes());
            }
            r.push(0);
            r.extend_from_slice(&question[question.len() - 4..]);
        }
        for l in NAME.split('.') {
            r.push(l.len() as u8);
            r.extend_from_slice(l.as_bytes());
        }
        r.push(0);
        r.extend_from_slice(&[0, 1]); // A
        r.extend_from_slice(if legacy { &[0x00, 0x01] } else { &[0x80, 0x01] }); // IN, cache flush
        r.extend_from_slice(&(if legacy { 10 } else { TTL_S }).to_be_bytes());
        r.extend_from_slice(&[0, 4]);
        r.extend_from_slice(&ip.octets());
        return Some((r, unicast));
    }
    None
}

/// Port 5353 shared with anything else listening there (AirPlay's responder), and the mDNS group
/// joined on the Wi-Fi interface.
fn socket(ip: Ipv4Addr) -> Result<UdpSocket, String> {
    // SAFETY: a fresh socket, owned by the UdpSocket from here on; the options are plain ints.
    let s = unsafe {
        let fd = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
        if fd < 0 {
            return Err("socket".into());
        }
        let one: libc::c_int = 1;
        for opt in [libc::SO_REUSEADDR, libc::SO_REUSEPORT] {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                opt,
                &one as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }
        UdpSocket::from_raw_fd(fd)
    };
    let sa = any_addr(PORT);
    // SAFETY: binding the socket above to 0.0.0.0:5353.
    let rc = unsafe {
        libc::bind(
            std::os::unix::io::AsRawFd::as_raw_fd(&s),
            &sa as *const libc::sockaddr_in as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
        )
    };
    if rc != 0 {
        return Err(format!("bind 5353: {}", std::io::Error::last_os_error()));
    }
    s.join_multicast_v4(&GROUP, &ip)
        .map_err(|e| e.to_string())?;
    let _ = s.set_multicast_ttl_v4(255);
    let _ = s.set_read_timeout(Some(Duration::from_millis(250)));
    Ok(s)
}

fn any_addr(port: u16) -> libc::sockaddr_in {
    // SAFETY: an all-zero sockaddr_in is valid; the fields that matter are set below.
    let mut sa: libc::sockaddr_in = unsafe { std::mem::zeroed() };
    sa.sin_family = libc::AF_INET as libc::sa_family_t;
    sa.sin_port = port.to_be();
    sa
}

/// Answer for `ip` until `stop`.
pub fn start(ip: Ipv4Addr) -> Result<(), String> {
    let s = socket(ip)?;
    RUN.store(true, Ordering::SeqCst);
    // Tell the network once, so a cache holding an old address for the name drops it.
    let hello = [0u8, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 6]
        .iter()
        .copied()
        .chain(*b"cinder")
        .chain([5])
        .chain(*b"local")
        .chain([0, 0, 1, 0, 1])
        .collect::<Vec<u8>>();
    if let Some((r, _)) = answer(&hello, ip, false) {
        let _ = s.send_to(&r, SocketAddrV4::new(GROUP, PORT));
    }
    std::thread::Builder::new()
        .name("cinder-mdns".into())
        .stack_size(64 * 1024)
        .spawn(move || {
            let mut buf = [0u8; 1500];
            while RUN.load(Ordering::SeqCst) {
                let Ok((n, std::net::SocketAddr::V4(from))) = s.recv_from(&mut buf) else {
                    continue;
                };
                let legacy = from.port() != PORT;
                if let Some((r, unicast)) = answer(&buf[..n], ip, legacy) {
                    let to = if unicast {
                        from
                    } else {
                        SocketAddrV4::new(GROUP, PORT)
                    };
                    let _ = s.send_to(&r, to);
                }
            }
        })
        .map_err(|e| {
            RUN.store(false, Ordering::SeqCst);
            e.to_string()
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(name: &str, qtype: u16, qclass: u16, id: u16) -> Vec<u8> {
        let mut q = id.to_be_bytes().to_vec();
        q.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
        for l in name.split('.') {
            q.push(l.len() as u8);
            q.extend_from_slice(l.as_bytes());
        }
        q.push(0);
        q.extend_from_slice(&qtype.to_be_bytes());
        q.extend_from_slice(&qclass.to_be_bytes());
        q
    }

    #[test]
    fn answers_its_name_and_nothing_else() {
        let ip = Ipv4Addr::new(192, 168, 1, 23);
        let (r, unicast) = answer(&query("Cinder.local", 1, 1, 0x1234), ip, false).unwrap();
        assert!(!unicast);
        assert_eq!(&r[0..2], &[0, 0], "multicast replies carry id 0");
        assert_eq!(&r[2..4], &[0x84, 0x00]);
        assert_eq!(&r[6..8], &[0, 1], "one answer");
        assert_eq!(&r[r.len() - 4..], &[192, 168, 1, 23]);
        assert_eq!(
            &r[r.len() - 14..r.len() - 6],
            &[0, 1, 0x80, 0x01, 0, 0, 0, 120]
        );
        assert!(
            answer(&query("cinder.local", 28, 1, 0), ip, false).is_none(),
            "AAAA"
        );
        assert!(answer(&query("other.local", 1, 1, 0), ip, false).is_none());
        let (_, unicast) = answer(&query("cinder.local", 255, 0x8001, 0), ip, false).unwrap();
        assert!(unicast, "the QU bit asks for a direct reply");
        let mut resp = query("cinder.local", 1, 1, 0);
        resp[2] = 0x84;
        assert!(
            answer(&resp, ip, false).is_none(),
            "answers are not questions"
        );
    }

    #[test]
    fn a_plain_resolver_gets_its_id_and_question_back() {
        let ip = Ipv4Addr::new(10, 0, 0, 2);
        let q = query("cinder.local", 1, 1, 0xBEEF);
        let (r, unicast) = answer(&q, ip, true).unwrap();
        assert!(unicast);
        assert_eq!(&r[0..2], &[0xBE, 0xEF]);
        assert_eq!(&r[4..8], &[0, 1, 0, 1], "question echoed, one answer");
        assert_eq!(&r[12..12 + q.len() - 12], &q[12..], "the same question");
        assert_eq!(&r[r.len() - 14..r.len() - 6], &[0, 1, 0, 1, 0, 0, 0, 10]);
    }

    #[test]
    fn a_compressed_second_question_is_read() {
        // Question 1: other.local; question 2: "cinder" + pointer to "local" in question 1.
        let mut q = vec![0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0];
        q.extend_from_slice(&[
            5, b'o', b't', b'h', b'e', b'r', 5, b'l', b'o', b'c', b'a', b'l', 0,
        ]);
        q.extend_from_slice(&[0, 1, 0, 1]);
        q.extend_from_slice(&[6, b'c', b'i', b'n', b'd', b'e', b'r', 0xC0, 18]);
        q.extend_from_slice(&[0, 1, 0, 1]);
        let (r, _) = answer(&q, Ipv4Addr::new(1, 2, 3, 4), false).unwrap();
        assert_eq!(&r[r.len() - 4..], &[1, 2, 3, 4]);
        assert!(
            answer(&q[..q.len() - 3], Ipv4Addr::LOCALHOST, false).is_none(),
            "truncated"
        );
    }
}
