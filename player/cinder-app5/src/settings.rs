//! v0.4's settings file, read once, for the getters the shell APPLIES to the device at boot.
//!
//! Until v0.5 owns its settings (P4), these must not answer the stubs' 0: the first player run
//! showed what that does. `cinder_get_brightness() == 0` turned the backlight off, and the same 0
//! means Bluetooth off, the balance hard left and silence. Each getter here returns exactly what
//! v0.4 writes under its key (`cinder-ffi/src/settings_io.rs`, `settings_body`), or v0.4's default.
// ponytail: read-only and fixed at boot; a change made on the player is not seen until P4.

use std::collections::HashMap;
use std::ffi::{c_char, CStr};
use std::sync::OnceLock;

static FILE: OnceLock<HashMap<String, String>> = OnceLock::new();

fn parse(body: &str) -> HashMap<String, String> {
    body.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

fn get(key: &str, default: i32, lo: i32, hi: i32) -> libc::c_int {
    FILE.get()
        .and_then(|m| m.get(key))
        .and_then(|v| v.parse::<i32>().ok())
        .map_or(default, |v| v.clamp(lo, hi))
}

/// Read the file. Returns 0, "no settings applied", so the shell neither re-applies EQ and sound
/// (v0.5 does not own them yet) nor pushes a volume from here.
///
/// # Safety
/// `path` is NULL or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn cinder_settings_load(path: *const c_char) -> libc::c_int {
    if !path.is_null() {
        let p = CStr::from_ptr(path).to_string_lossy().into_owned();
        if let Ok(body) = std::fs::read_to_string(&p) {
            let _ = FILE.set(parse(&body));
        }
    }
    0
}

macro_rules! getter {
    ($name:ident, $key:literal, $default:expr, $lo:expr, $hi:expr) => {
        #[no_mangle]
        pub extern "C" fn $name() -> libc::c_int {
            get($key, $default, $lo, $hi)
        }
    };
}

getter!(cinder_get_brightness, "brightness", 3, 1, 5);
getter!(cinder_get_bt_on, "bt_on", 1, 0, 1);
getter!(cinder_get_volume, "volume", 0, 0, 120);
getter!(cinder_get_bt_volume, "bt_volume127", 0, 0, 127);
getter!(cinder_get_balance, "balance100", 50, 0, 100);
getter!(cinder_get_screen_off_s, "screen_off", 30, 0, 86_400);
getter!(cinder_get_auto_off_min, "auto_off", 0, 0, 1_440);
getter!(cinder_get_bt_codec, "bt_codec", 0, 0, 16);
getter!(cinder_get_bt_ldac_quality, "bt_ldac_quality", 0xFF, 0, 0xFF);
getter!(cinder_get_mono, "mono", 0, 0, 1);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_getters_answer_the_file_or_a_safe_default() {
        let _ = FILE.set(parse(
            "brightness=4\nbalance100=61\nbt_on=0\nscreen_off=junk\n",
        ));
        assert_eq!(cinder_get_brightness(), 4);
        assert_eq!(cinder_get_balance(), 61);
        assert_eq!(cinder_get_bt_on(), 0);
        assert_eq!(cinder_get_screen_off_s(), 30, "garbage is the default");
        assert_eq!(
            cinder_get_volume(),
            0,
            "missing volume is silence, never loud"
        );
    }
}
