// preload_glue.h — what cinder-home writes and reads for the libraries it preloads into Sony's
// services: the flags libcinder_mono.so polls (mono, the long low-power ring), its loaded marker,
// and the proof files the boot guard reads before it keeps the hagodaemon wrapper.
// Moved out of main.cpp unchanged (2026-11-23).
#pragma once

void mono_flag_apply();
void long_ring_flag_apply();
void mono_shim_poll();
int mono_shim_seen();   // -1 not looked yet, 0 not loaded, 1 loaded in SoundServiceFw
void hago_note_healthy();
void hago_note_transport(bool playing);
void hago_tick(bool running);
