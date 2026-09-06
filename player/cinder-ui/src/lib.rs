//! Cinder UI rendering core for the NW-A50 replacement player.
//!
//! Renders into a 480x800 XRGB8888 buffer (the device panel format:
//! `/dev/graphics/fb0`, mtkfb, 0x00RRGGBB). `embedded-graphics` draws the
//! primitives; `fontdue` rasterises the Cinder type scale (proportional Hanken
//! Grotesk + mono JetBrains Mono). Backends (host PNG / device framebuffer)
//! live in their own crates and just hand us a `Canvas`.

pub mod art;
pub mod canvas;
pub mod chrome;
pub mod confirm;
pub mod data;
pub mod icons;
pub mod model;
pub mod overlay;
pub mod theme;
pub mod text;
pub mod widgets;
pub mod lock;
pub mod menu;
pub mod now_playing;
pub mod shelf;
pub mod up_next;
pub mod keyboard;
pub mod library;
pub mod playlist_pick;
pub mod viz;
pub mod vizcfg;
pub mod vizset;
pub mod eq;
pub mod sound;
pub mod advanced;
pub mod tone;
pub mod device;
pub mod settings;
pub mod bluetooth;
pub mod pairing;
pub mod receiver;
pub mod fm;
pub mod folders;
pub mod track_info;
pub mod clockset;
pub mod usbdac;
pub mod usb_storage;
pub mod onboarding;
pub mod nav;

pub use canvas::{Canvas, H, W};
pub use model::Library;
pub use text::{Family, FontSet, TextStyle, Weight};
pub use theme::{Accent, Theme};
