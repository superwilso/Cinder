//! Which output is live (the jack, Bluetooth, USB-DAC), and the A/B compare's letters.
//!
//! Until 2026-10-18 this was "sound profiles per output": each output remembered A or B and a
//! route change switched between them. The owner's call that day: A/B is a compare, and saved
//! sounds are presets (`presets.rs`). What is left here is the route, which the jack's DAC EQ
//! still needs (`nav::App::eq_on_dac`).

/// Where the audio is going. The order is the persisted order of [`crate::nav::App::profile_map`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// The 3.5 mm jack (also the output while nothing else is).
    Jack,
    /// A Bluetooth sink is connected and the player is transmitting to it.
    Bluetooth,
    /// USB-DAC mode: a PC is the source, and the player renders it to the jack or bridges it to a
    /// connected Bluetooth sink. One output for both, because the SOURCE is what makes it different.
    UsbDac,
}

impl Output {
    pub const ALL: [Output; 3] = [Output::Jack, Output::Bluetooth, Output::UsbDac];

    pub fn idx(self) -> usize {
        match self {
            Output::Jack => 0,
            Output::Bluetooth => 1,
            Output::UsbDac => 2,
        }
    }

    /// Row title (unused since the profiles were retired).
    pub fn label(self) -> &'static str {
        match self {
            Output::Jack => crate::i18n::text("Headphone jack"),
            Output::Bluetooth => crate::i18n::text("Bluetooth"),
            Output::UsbDac => "USB-DAC",
        }
    }

    /// Mono caption form, for strips and toasts.
    pub fn short(self) -> &'static str {
        match self {
            Output::Jack => "3.5 MM",
            Output::Bluetooth => crate::i18n::text("BLUETOOTH"),
            Output::UsbDac => "USB-DAC",
        }
    }

    /// The settings-file key holding this output's profile letter. Part of the on-disk format.
    pub fn key(self) -> &'static str {
        match self {
            Output::Jack => "profile_jack",
            Output::Bluetooth => "profile_bt",
            Output::UsbDac => "profile_usb",
        }
    }
}

/// Which output is live. USB-DAC wins over Bluetooth: in DAC mode the PC is the source even when
/// the bridge is sending it to headphones, and that is the case a separate profile exists for.
pub fn output_for(usb_dac: bool, bt_route: bool) -> Output {
    if usb_dac {
        Output::UsbDac
    } else if bt_route {
        Output::Bluetooth
    } else {
        Output::Jack
    }
}

/// Profile letters, indexed by setup index (0 = A, 1 = B).
pub const LETTERS: [&str; 2] = ["A", "B"];

pub fn letter(idx: usize) -> &'static str {
    LETTERS[idx & 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_dac_outranks_bluetooth_outranks_the_jack() {
        assert_eq!(output_for(false, false), Output::Jack);
        assert_eq!(output_for(false, true), Output::Bluetooth);
        assert_eq!(output_for(true, false), Output::UsbDac);
        assert_eq!(
            output_for(true, true),
            Output::UsbDac,
            "DAC mode bridging to BT is still DAC mode"
        );
    }
}
