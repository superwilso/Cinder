//! USB mass storage's bulk-only transport, for sending the player a SCSI command where the system
//! has no SCSI passthrough for a mounted disk (macOS). A command goes out as a 31-byte Command
//! Block Wrapper on the bulk-OUT endpoint; the data, then a 13-byte Command Status Wrapper, come
//! back on bulk-IN. Layouts from the USB Mass Storage Class Bulk-Only Transport specification 1.0,
//! §5.1 and §5.2.

/// A Command Block Wrapper for a 12-byte CDB that reads `data_len` bytes from the device (LUN 0).
pub fn cbw(tag: u32, data_len: u32, cdb: &[u8; 12]) -> [u8; 31] {
    let mut b = [0u8; 31];
    b[0..4].copy_from_slice(b"USBC");
    b[4..8].copy_from_slice(&tag.to_le_bytes());
    b[8..12].copy_from_slice(&data_len.to_le_bytes());
    b[12] = 0x80; // data-in
    b[13] = 0; // LUN
    b[14] = cdb.len() as u8;
    b[15..27].copy_from_slice(cdb);
    b
}

/// What a Command Status Wrapper says.
#[derive(Debug, PartialEq, Eq)]
pub enum Status {
    Passed,
    /// The device understood the transport and refused the command: try the other flag byte.
    Failed,
    /// The transport itself went wrong; the device needs a reset.
    PhaseError,
}

/// Read a Command Status Wrapper answering the command tagged `tag`.
pub fn csw(b: &[u8], tag: u32) -> Result<Status, String> {
    if b.len() != 13 || &b[0..4] != b"USBS" {
        return Err(format!("not a status reply ({} bytes)", b.len()));
    }
    let got = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    if got != tag {
        return Err(format!("status for command {got:#x}, not {tag:#x}"));
    }
    match b[12] {
        0 => Ok(Status::Passed),
        1 => Ok(Status::Failed),
        2 => Ok(Status::PhaseError),
        s => Err(format!("unknown status {s}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::{FW_UPGRADE_CDB, KAS_CDB};

    #[test]
    fn the_wrapper_frames_both_vendor_commands() {
        let b = cbw(0x1234_5678, 0x80, &FW_UPGRADE_CDB);
        assert_eq!(&b[0..4], b"USBC");
        assert_eq!(&b[4..8], &[0x78, 0x56, 0x34, 0x12], "tag, little-endian");
        assert_eq!(&b[8..12], &[0x80, 0, 0, 0], "transfer length");
        assert_eq!(
            (b[12], b[13], b[14]),
            (0x80, 0, 12),
            "data-in, LUN 0, 12-byte CDB"
        );
        assert_eq!(&b[15..27], &FW_UPGRADE_CDB);
        assert_eq!(&b[27..], &[0; 4], "the CDB field's unused tail");
        let k = cbw(7, 64, &KAS_CDB);
        assert_eq!(&k[15..27], &KAS_CDB);
        assert_eq!(&k[8..12], &[64, 0, 0, 0]);
    }

    #[test]
    fn the_status_reply_is_read_and_checked() {
        let reply = |tag: u32, status: u8| {
            let mut b = b"USBS".to_vec();
            b.extend_from_slice(&tag.to_le_bytes());
            b.extend_from_slice(&[0; 4]); // residue
            b.push(status);
            b
        };
        assert_eq!(csw(&reply(9, 0), 9), Ok(Status::Passed));
        assert_eq!(csw(&reply(9, 1), 9), Ok(Status::Failed));
        assert_eq!(csw(&reply(9, 2), 9), Ok(Status::PhaseError));
        assert!(csw(&reply(8, 0), 9).is_err(), "another command's status");
        assert!(csw(&reply(9, 3), 9).is_err());
        assert!(
            csw(b"USBC\0\0\0\0\0\0\0\0\0", 0).is_err(),
            "not a status reply"
        );
        assert!(csw(&reply(9, 0)[..12], 9).is_err(), "short");
    }
}
