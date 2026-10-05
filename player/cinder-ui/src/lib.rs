//! Cinder UI rendering core for the NW-A50 replacement player.
//!
//! Renders into a 480x800 XRGB8888 buffer (the device panel format:
//! `/dev/graphics/fb0`, mtkfb, 0x00RRGGBB). `embedded-graphics` draws the
//! primitives; `fontdue` rasterises the Cinder type scale (proportional Hanken
//! Grotesk + mono JetBrains Mono). Backends (host PNG / device framebuffer)
//! live in their own crates and just hand us a `Canvas`.

pub mod advanced;
pub mod art;
pub mod bluetooth;
pub mod canvas;
pub mod chrome;
pub mod clockset;
pub mod collate;
pub mod confirm;
pub mod dac_eq;
pub mod data;
pub mod device;
pub mod display;
pub mod eq;
pub mod fm;
pub mod folders;
pub mod help;
pub mod icons;
pub mod keyboard;
pub mod kit;
pub mod library;
pub mod lock;
pub mod lyrics;
pub mod menu;
pub mod model;
pub mod nav;
pub mod now_playing;
pub mod np_styles;
pub mod onboarding;
pub mod overlay;
pub mod pairing;
pub mod palette;
pub mod palette_list;
pub mod playlist_edit;
pub mod playlist_pick;
pub mod profile;
pub mod quick;
pub mod receiver;
pub mod scale;
pub mod search;
pub mod sensme;
pub mod settings;
pub mod shelf;
pub mod shuffle;
pub mod sound;
pub mod soundscape;
pub mod style;
pub mod text;
pub mod theme;
pub mod tone;
pub mod track_info;
pub mod up_next;
pub mod usb_storage;
pub mod usbdac;
pub mod view_edit;
pub mod views;
pub mod viz;
pub mod vizcfg;
pub mod vizset;
pub mod widgets;

pub use canvas::{Canvas, H, W};
pub use model::Library;
pub use text::{Family, FontSet, TextStyle, Weight};
pub use theme::{Accent, Theme};
