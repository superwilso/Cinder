//! Services shared by both Cinder apps: v0.4's `cinder-ffi` and v0.5's `cinder-app5`
//! (docs/PLAN_v0.5.md). Moved out of `cinder-ffi` unchanged; nothing here exports a C symbol or
//! draws, except `flac`: the decoder the audio engine calls by C name, which both apps link once
//! through this crate.

pub mod cue;
pub mod display;
pub mod flac;
pub mod likes;
pub mod mdns;
pub mod ovl;
pub mod pcm_tap;
pub mod present;
pub mod scrobble;
pub mod series;
pub mod shared;
