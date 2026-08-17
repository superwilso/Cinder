/* cinder.h — C ABI for the Rust Cinder UI (libcinder_ffi.a, glibc armhf).
 * Include from the C++ easel shell (cinder-home). All strings are copied; NULL = empty. */
#ifndef CINDER_H
#define CINDER_H
#ifdef __cplusplus
extern "C" {
#endif

/* Open /dev/graphics/fb0 + init the renderer. 0 = ok, <0 = error. */
int  cinder_render_init(void);
/* Render the current state to the panel; call once per frame from the pump. */
void cinder_render_tick(void);
/* Frames whose presentation has COMPLETED (pixels pushed to the panel). The present runs on its
 * own thread by default, so cinder_render_tick returning no longer means a frame reached the
 * glass — gate any "first frame painted" health logic on this counter going nonzero. */
unsigned long long cinder_frames_presented(void);
/* Frame-time bench: render `frames` frames, reporting rasterize vs present cost. `scroll` != 0
   drags the library list by that many px per frame so the numbers cover scrolling, not a static
   screen. Diagnostic only — cinder-probe --bench. */
void cinder_render_bench(int frames, int scroll);
/* Diagnostic: resolve+decode album art for one object_id, reporting shape and timing. */
int  cinder_art_probe(long long object_id);
/* Force the next cinder_render_tick to repaint + blit even if nothing changed. Used to overwrite
 * anything an EXTERNAL process scribbled on the framebuffer (the boot animation's last frame
 * survives its kill — the dirty-flag renderer would otherwise never paint over it). */
void cinder_force_dirty(void);
/* Save the NEXT rendered frame as a PNG at `path`. Captures the Canvas before presentation, so it
 * is faithful on BOTH the software framebuffer and the GPU/EGL path (under EGL the Mali swapchain
 * owns the panel, so reading /dev/graphics/fb0 externally does not reliably show what's on screen).
 * Also marks the UI dirty so an idle screen still repaints. Returns 0 if the request was accepted. */
int  cinder_request_screenshot(const char *path);
/* Raise the USB mass-storage modal (idempotent). The shell calls this when it auto-detects a PC
 * host, BEFORE flipping the gadget to MSC, so the UI shows the same modal a settings-row tap would.
 * Returns 1 if the modal is now up. */
int  cinder_show_usb_storage(void);
/* Unmap + tear down. */
void cinder_render_shutdown(void);
/* 0 = day theme, non-zero = night. */
void cinder_set_theme_night(int night);
/* Load + apply persisted UI preferences (theme + visualiser + EQ + sound effects + volume) from
 * `path`, and remember it so later changes auto-save. Call once at boot after cinder_render_init.
 * Returns a bitmask: bit0 = file read (re-apply EQ/sound to the DSP), bit1 = a persisted volume
 * level was restored (apply it to the mixer instead of seeding from hardware). 0 = no file. */
int cinder_settings_load(const char *path);

/* Resume across a reboot. `seq_path` holds the playback context + user queue (object ids, written
 * only when they change); `pos_path` holds the current track + position (30 bytes, written at most
 * every 5 s). Call ONCE at boot AFTER cinder_db_open — the ids are resolved against the library.
 * Returns 1 if a sequence was restored, 0 if there was nothing to restore, -2 if not initialised.
 *
 * Restoring does NOT start playback and does not touch PlayerService: a player that starts playing
 * by itself at power-on is worse than the bug this fixes. The sequence is handed over on the first
 * transport press instead — see cinder_resume_take_pending. Calling this also ARMS the saving
 * half, so both files stay current from here on. */
int  cinder_resume_load(const char *seq_path, const char *pos_path);
/* Call on the first ▶ after a boot. 1 = the restored sequence is now in cinder_pending_play_* and
 * cinder_play_position_ms() is where to seek, so hand it to play_pending_sequence with
 * restore_position; 0 = nothing pending, carry on with an ordinary play. One-shot. */
int  cinder_resume_take_pending(void);
/* Throw a pending resume away — the user started something themselves, so the sequence the last
 * boot left behind must not overwrite it. */
void cinder_resume_cancel(void);
/* Write both resume files right now (before a deliberate power-off / reboot). */
void cinder_resume_flush(void);

/* Logical buttons (the backend maps raw evdev key codes -> these). */
typedef enum {
    CINDER_BTN_UP = 0, CINDER_BTN_DOWN = 1, CINDER_BTN_LEFT = 2, CINDER_BTN_RIGHT = 3,
    CINDER_BTN_SELECT = 4, CINDER_BTN_BACK = 5, CINDER_BTN_OPTION = 6, CINDER_BTN_PLAY = 7,
    CINDER_BTN_HOME = 8, CINDER_BTN_VOLUP = 9, CINDER_BTN_VOLDOWN = 10, CINDER_BTN_POWER = 11,
    /* The Hold/lock SWITCH (sustained state, not a press). The shell routes its code to
     * cinder_set_hold(), NOT cinder_input(); this marker just lets the keymap config identify it. */
    CINDER_BTN_HOLD = 12,
    /* Dedicated transport buttons (the NW-A55 side FF/REW keys). Unlike LEFT/RIGHT these are
     * GLOBAL: next/previous track on every screen, exactly like the stock player. */
    CINDER_BTN_NEXT = 13, CINDER_BTN_PREV = 14
} cinder_button_t;

/* Actions the shell performs (via cinder-audio etc.) in response to cinder_input(). */
typedef enum {
    CINDER_ACT_NONE = 0, CINDER_ACT_PLAYPAUSE = 1, CINDER_ACT_NEXT = 2, CINDER_ACT_PREV = 3,
    CINDER_ACT_NEXT_ALBUM = 4, CINDER_ACT_PREV_ALBUM = 5, CINDER_ACT_VOLUP = 6,
    CINDER_ACT_VOLDOWN = 7, CINDER_ACT_PLAY_INDEX = 8, CINDER_ACT_SLEEP = 10,
    CINDER_ACT_ENTER_USB_MSC = 11, CINDER_ACT_EQ_CHANGED = 12,
    CINDER_ACT_BATTERY_CARE_CHANGED = 13, CINDER_ACT_SOUND_CHANGED = 14,
    CINDER_ACT_SOUND_BYPASS = 15, CINDER_ACT_THEME_CHANGED = 16,
    /* Device-wide BT transmit codec / LDAC quality changed: read cinder_get_bt_codec +
     * cinder_get_bt_ldac_quality and apply via BtTransmitterService (feeds the LDAC bridge too). */
    CINDER_ACT_BT_CODEC_CHANGED = 17,
    /* USB-DAC toggled: read cinder_get_usb_dac() and start/stop the LDAC bridge + switch the USB
     * gadget to UAC, WITHOUT disconnecting Bluetooth (the headline USB-DAC→LDAC feature). */
    CINDER_ACT_USBDAC_LDAC = 18,
    /* User left the USB mass-storage modal (Back): remount /contents and restore the USB mode. */
    CINDER_ACT_EXIT_USB_MSC = 19,
    /* Panel brightness changed: read cinder_get_brightness() (1..5) and write the backlight. */
    CINDER_ACT_BRIGHTNESS_CHANGED = 20,
    /* Idle screen-off timeout changed: read cinder_get_screen_off_s() (seconds, 0 = off). */
    CINDER_ACT_SCREEN_OFF_CHANGED = 21,
    /* Settings ▸ Boot to stock, confirmed: arm the launcher's ONE-SHOT stock flag and restart into
     * Sony's player. One-shot by design — it is the only escape reachable with no USB cable, so it
     * must undo itself or it would strand a cable-less user on stock. */
    CINDER_ACT_BOOT_TO_STOCK = 22,
    /* Now Playing repeat toggled: read cinder_get_repeat_one() (1/0) and hand it to
     * cinder_audio_set_repeat_one(). Two states only — no repeat-ALL primitive is known. */
    CINDER_ACT_REPEAT_CHANGED = 23,
    /* Settings ▸ Restart, CONFIRMED in the modal: PowerMgrServiceClient::Reboot(). Comes back into
     * Cinder, unlike CINDER_ACT_BOOT_TO_STOCK which arms the one-shot stock flag first. */
    CINDER_ACT_RESTART = 24,
    /* Settings ▸ Power off, CONFIRMED in the modal: PowerMgrServiceClient::SetStatus(PowerOff). */
    CINDER_ACT_POWER_OFF = 25,
    /* Bluetooth toggled: drive the RADIO via BtCommonServiceClient::SetRfOnOff, and on enable also
     * ask BtTransmitterServiceClient to reconnect the last device. Until 2026-07-29 this action did
     * not exist and the toggle was dropped in cinder-ffi ("UI-only"), which is why the switch never
     * affected the hardware and paired headphones never reconnected. */
    CINDER_ACT_BT_TOGGLE = 26,
    /* Hang up on the connected device but LEAVE THE RADIO ON: call
     * BtTransmitterServiceClient::RequestDisconnection (slot 8, no args). Distinct from
     * BT_TOGGLE off, which powers the radio down and makes the device unreconnectable. */
    CINDER_ACT_BT_DISCONNECT = 27,
    /* Connect a specific PAIRED device: read the row with cinder_pending_bt_device(), look up the
     * BD address in the shell's own copy of the list, and call
     * BtTransmitterServiceClient::RequestConnection(const vector<uint8_t>&) (slot 6). */
    CINDER_ACT_BT_CONNECT_DEVICE = 28,
    /* Forget a paired device: same row channel, then
     * BtCommonServiceClient::DeleteLinkkey(const vector<uint8_t>&) (slot 15). */
    CINDER_ACT_BT_FORGET_DEVICE = 29,
    /* Re-read GetPairedDeviceInfo (slot 20) and push the list back with cinder_bt_paired_*. */
    CINDER_ACT_BT_PAIRED_REFRESH = 30,
    /* Start/stop discovery: read cinder_get_bt_scanning(), then
     * BtCommonServiceClient::SetSearchMode(const bool&, const uint16_t&) (slot 14). Results arrive on
     * BtCommonServiceListener::OnNotifySearchedDevice (listener slot 6) and are pushed back with
     * cinder_bt_found_*. */
    CINDER_ACT_BT_SCAN_TOGGLE = 31,
    /* Pair with a DISCOVERED device: row via cinder_pending_bt_device(), then
     * BtCommonServiceClient::Pairing(const vector<uint8_t>&) (slot 7). */
    CINDER_ACT_BT_PAIR_DEVICE = 32,
    /* Pairing prompt answered. CONFIRM -> SetNumericComparison(addr, true) (slot 9) or
     * RequestSspReply(addr, variant, true, value) (slot 28); CANCEL -> the same with false, or
     * CancelPairing (slot 8) for a display-only passkey. The address comes from the notification the
     * shell received, never from the UI. */
    CINDER_ACT_BT_PROMPT_CONFIRM = 33,
    CINDER_ACT_BT_PROMPT_CANCEL = 34,
    /* Sony's "Use Enhanced Mode" toggled (firmware message 230077; help text 230079 is "Select
     * this check box if you cannot change the volume"). It is the AVRCP absolute-volume switch:
     * read cinder_get_bt_enhanced() and call
     * BtTransmitterServiceClient::SetControlAbsoluteVolume(const bool&) (slot 31).
     *
     * This is not optional bookkeeping. Sony's SetCurrentVolume checks the same preference before
     * transmitting ("Not control absolute volume mode" / "Not support absolute volume" in
     * libBtTransmitterService.so), so if the shell never sets it, absolute volume silently does
     * nothing and every volume step falls back to VOLUME_UP/VOLUME_DOWN key events — which sinks
     * such as the CMF Buds answer with their own feedback beep. */
    CINDER_ACT_BT_ENHANCED_CHANGED = 35,
    /* The user queue changed while music is playing.  Rebuild the PlayerService sequence now,
     * restoring the current position, so an item marked "Playing next" really is next. */
    CINDER_ACT_QUEUE_CHANGED = 36,
    /* Settings > Reset settings, confirmed. Every preference is back to its default; re-apply the
     * whole chain from the UI's state (EQ, sound flags, balance, backlight, volume). Nothing needs
     * reading back one field at a time — the point of the reset is that all of it moved. */
    CINDER_ACT_SETTINGS_RESET = 37,
    /* Sound > Balance moved. BALANCE ONLY — deliberately not CINDER_ACT_SOUND_CHANGED, which
     * re-applies the whole DSP chain: the slider emits this on every motion event while a finger is
     * down, so it must stay cheap. Read cinder_get_balance() and write the two mixer controls;
     * skip the write when the raw pair is unchanged. A CINDER_ACT_SOUND_CHANGED still arrives when
     * the finger lifts, which is what persists the value to the settings file. */
    CINDER_ACT_BALANCE_CHANGED = 38
} cinder_action_t;

/* Deliver a button press to the navigator. Theme changes are applied internally; returns a
 * cinder_action_t for the shell to carry out (0 = nothing). */
int  cinder_input(int button);
/* Touchscreen navigation (the NW-A55 has no d-pad). cinder_tap delivers a tap at UI coordinates
 * (x:0..480, y:0..800) — returns a cinder_action_t for the shell to carry out, same as cinder_input.
 * Scrolling is PIXEL-based for smoothness: while a vertical drag is in progress the shell streams
 * cinder_touch_drag(dy_px) per pump tick (positive = show later rows); at release it hands the
 * measured velocity to cinder_touch_fling(px/s) for momentum, and on a new contact it calls
 * cinder_touch_down() to stop an in-flight fling. (The shell maps raw touch coordinates → UI and
 * classifies tap vs drag vs the left-edge Back swipe.) */
int  cinder_tap(int x, int y);
void cinder_touch_drag(int dy_px);
void cinder_touch_fling(int velocity_px_s);
void cinder_touch_down(void);
/* Pending play request, populated when CINDER_ACT_PLAY_INDEX is returned: the tapped track's
 * album context as file URIs in play order + the index to start at. The shell reads these and
 * hands PlayerService a NodeTrackSequence (cinder_audio_play_tracks). */
int  cinder_pending_play_count(void);
/* Copies URI `i` into `buf` and returns its FULL length (snprintf semantics): a return >= `cap`
 * means it was TRUNCATED and must not be used — a truncated path still looks valid and would queue
 * a file that doesn't exist. -1 = bad index/args. */
int  cinder_pending_play_uri(int i, char* buf, int cap);
int  cinder_pending_play_start(void);
/* Horizontal swipe (dir < 0 = leftward, else rightward) with the gesture's START point in UI
 * coordinates: onboarding pages through, Now Playing skips track, and a rightward swipe on a
 * Library/Album song row queues that song (start y picks the row). Returns a cinder_action_t for
 * the shell to carry out (0 = nothing). */
int  cinder_swipe(int dir, int x, int y);
/* LIVE horizontal drag on a list row, streamed per pump tick while the contact is down: `dx_px` is
 * total travel from the gesture's start point, `y` that start point. The row under `y` slides with
 * the finger and reveals what releasing will do. Returns 1 if a track row took the gesture — the
 * shell should then commit the contact to the swipe and stop weighing it as a vertical scroll.
 * Nothing is queued here; that still happens at release, in cinder_swipe. Call
 * cinder_swipe_release() when the finger lifts so the row animates back to rest. */
int  cinder_swipe_track(int dx_px, int y);
void cinder_swipe_release(void);
/* Up Next queue REORDER, the vertical counterpart of the swipe above. Asked once, when the shell
 * classifies a contact as mostly-vertical: cinder_reorder_begin() returns 1 if the finger LANDED on
 * a queue row's grab handle, in which case that row owns the contact for the rest of its life — the
 * shell streams total travel to cinder_reorder_track() instead of to cinder_touch_drag(), and calls
 * cinder_reorder_release() on lift to drop the row where it sits. Start-point ownership, same rule
 * as the scrub rail: a drag that begins elsewhere scrolls even if it wanders over the handle. */
int  cinder_reorder_begin(int x, int y);
void cinder_reorder_track(int dy_px);
void cinder_reorder_release(void);
/* SCROLLBAR drag — grab the bar at the right edge and drag it, as the Sony UI does. Same contract
 * again, and offered right AFTER the reorder so a queue row's grab handle wins where the two strips
 * would overlap. The strip is shared with the A-Z rail and split by GESTURE: a tap there is still a
 * letter jump (cinder_tap), a drag is this. The content follows the THUMB, not the finger. */
int  cinder_sbar_begin(int x, int y);
void cinder_sbar_track(int dy_px);
void cinder_sbar_release(void);
/* The Hold/lock SWITCH changed state (held = 1 locked, 0 unlocked). When locked the touchscreen is
 * ignored but the transport/volume buttons still work; ONLY the switch going off (held=0) unlocks.
 * Power is a screen on/off toggle (CINDER_ACT_SLEEP) and does NOT unlock. Call on every edge. */
void cinder_set_hold(int held);
/* The Power button has been held past the long-press threshold (~1 s): open the Power menu
 * (Power off / Restart / Cancel), which is what the stock Sony firmware does. Returns 1 if the
 * menu opened, 0 if refused (Hold engaged, or a modal is already up). A 1 means the shell must
 * NOT also toggle the screen when the button is finally released. */
int  cinder_power_held(void);
/* Is a modal dialog up? Used so the idle screen-blank timer does not blank a "Power off?" prompt
 * out from under the finger about to answer it. */
int  cinder_modal_open(void);
/* Open the library DB read-only (e.g. "/db/MTPDB.dat"). Call after cinder_render_init.
 * 0 = ok, -1 = open failed, -2 = renderer not initialised. */
int  cinder_db_open(const char *path);
/* Set now-playing from the track URI PlayerService reports (PlayStatus.uri): resolves
 * title/artist/codec/duration from the DB and derives elapsed/remaining from progress (0..1).
 * 0 = resolved, -1 = not found (falls back to filename), -2 = renderer not initialised. */
int  cinder_set_now_playing_uri(const char *uri, float progress, int playing, int battery);
/* Liked songs (the Now Playing heart). The set and its persistence live in cinder-ffi, so the
 * shell has nothing to carry out — the toggle is handled in-process when the heart is tapped.
 * cinder_toggle_liked returns the NEW state (1/0), or -1 if nothing is playing.
 * Two files are written beside the music on /contents: cinder_liked.conf (object ids, the real
 * state) and cinder_loved.tsv (artist<TAB>title). The TSV exists because this device has no WiFi,
 * so Last.fm can only ever be reached by a PC tool after a USB connection — and the AS/1.1
 * scrobble log can't carry loves (its rating column is Listened/Skipped). artist+title is exactly
 * what Last.fm's track.love takes. */
int  cinder_is_liked(void);
int  cinder_toggle_liked(void);
int  cinder_liked_count(void);
/* Drag-to-seek on the Now Playing progress rail. On finger-DOWN the shell asks cinder_scrub_hit;
 * if it returns 1 the whole contact belongs to the scrub (no tap / list-drag / swipe). Then
 * cinder_scrub_to(x) on down and on every move makes the bar follow the finger, and
 * cinder_scrub_end() at release returns the ms to hand to cinder_audio_seek_ms (-1 = nothing to
 * do). While a scrub is active, incoming position updates are ignored so the bar can't fight the
 * finger. */
/* ◁ semantics. Returns 1 when the current position is far enough into the track that ◁ should
 * REWIND to the start rather than step to the previous track (the standard 3 s convention).
 * The shell also falls back to a seek(0) when PlayController::PrevTrack() reports failure — at
 * the head of a sequence there is nothing to step back to, and ◁ used to do nothing at all. */
/* Leave the transient backlight-off state (brightness level 0) and return to the last visible
 * level. Returns 1 if it changed. The shell calls this on the next input after it applied 0, so a
 * fully dark panel is always one touch from coming back and is never persisted. */
int  cinder_brightness_wake(void);
int  cinder_prev_means_restart(void);
/* Prepare a PlayerService sequence starting at the preceding Cinder playback-history entry.
 * Returns 1 when cinder_pending_play_* is ready, 0 when there is no earlier track. */
int  cinder_prepare_previous_play(void);
/* Best current position estimate, used when rebuilding a sequence for a queue edit. */
int  cinder_play_position_ms(void);
/* Tell the UI that the SHELL seeked playback to `ms` (the ◁ rewind paths), so the progress bar
 * snaps there instead of extrapolating from the pre-seek anchor for ~1 s. */
void cinder_notify_seek_ms(int ms);
int  cinder_scrub_hit(int x, int y);
int  cinder_scrub_to(int x);
int  cinder_scrub_end(void);
/* Push the REAL position/duration from PlayerService's PlayEventListener (onPlayTimeUpdated) plus
 * the real play/pause state. Takes priority over the local play-clock estimate and is what makes
 * the progress bar follow seeks and mid-track starts. `cur_ms` < 0 = no update yet (ignored);
 * `total_ms` <= 0 keeps the DB duration. 0 = ok, -2 = renderer not initialised. */
int  cinder_set_play_position(int cur_ms, int total_ms, int playing);
/* Push the currently-playing track explicitly (progress 0..1, playing 0/1, battery 0..100). */
void cinder_set_now_playing(const char *title, const char *artist, const char *codec,
                            const char *elapsed, const char *remaining,
                            float progress, int playing, int battery);

/* Enable the built-in scrobbler: appends an Audioscrobbler/1.1 `.scrobbler.log` at `path`
 * (e.g. the storage root). `client` is the #CLIENT id. Call after cinder_db_open.
 * 0 = ok, -2 = renderer not initialised. */
int  cinder_scrobble_open(const char *path, const char *client);
/* Advance the scrobbler's play clock one second (call ~1x/sec from the pump). `playing` is
 * 0 paused / non-zero playing. No-op if the scrobbler isn't enabled. */
void cinder_scrobble_tick(int playing);
/* Refresh the status-bar/lock clock from local time (call ~1x/sec). Repaints only on minute change. */
void cinder_clock_tick(void);
/* Push battery percent (0..100) to the status bar; repaints only on change. */
void cinder_set_battery(int pct);
/* Raise the ordinary bottom toast the UI already uses for queue and Shelf feedback. For the shell
 * to say something the user must see (a low battery, an imminent shutdown) without inventing a
 * second notification surface. Fades on its own; NULL/empty is a no-op. */
void cinder_toast(const char *msg);
/* Copy the current 10-band EQ gains (dB) into `out` (>= 10 int8). Call after a
 * CINDER_ACT_EQ_CHANGED action, then apply them to the DSP via the effect shim. */
void cinder_get_eq_bands(signed char *out);
/* Battery care (Itawari charging). Push the device's real state into the UI toggle (1 on / 0 off;
 * <0 ignored) — call once at boot after reading PowerMgrServiceClient. */
void cinder_set_battery_care(int on);
/* Push the real storage usage label (e.g. "12.4 / 58 GB") for the Settings Storage row, formatted
 * from statvfs of the music mount. NULL/empty leaves the neutral placeholder. */
void cinder_set_storage(const char *label);
/* Sleep timer: returns 1 ONCE when the user's sleep timer (set in Settings) has just expired — the
 * shell then pauses playback. Poll ~1x/sec from the pump. The countdown itself is internal. */
int  cinder_sleep_should_pause(void);
/* Read the UI's desired battery-care value (1/0). Call after a CINDER_ACT_BATTERY_CARE_CHANGED
 * action, then apply it via the power shim. */
int  cinder_get_battery_care(void);
/* Read the UI's Sound-effect toggles as a bitmask (bit0 DSEE, bit1 Vinyl, bit2 VPT, bit3 DC-Phase,
 * bit4 Dynamic Normalizer, bit5 ClearAudio+). Call after a CINDER_ACT_SOUND_CHANGED action, then
 * apply each via the effect shim. */
int  cinder_get_sound_flags(void);
/* Read the current UI volume as the raw 0..120 step level (the stock scale — 1:1 with ALSA card0
 * 'master volume'). Call after a CINDER_ACT_VOLUP/VOLDOWN action and write it to the mixer. */
int  cinder_get_volume(void);
/* Seed the UI volume from the device's real level (raw 0..120 steps), no HUD pop. Call at boot
 * after restoring the saved level (or reading the mixer), so Vol± nudges from the actual level. */
void cinder_set_volume(int level);
/* Push the connected Bluetooth device's name into the UI (NULL or "" = nothing connected). Read it
 * from GetConnectInformation(vector<uint8_t>& addr, string& name). */
void cinder_set_bt_connected(const char* name);
/* Push the codec the live A2DP link NEGOTIATED — the raw BtSoundCodec from
 * BtTransmitterService::GetSoundStatus(BtSoundCodec&, BtSoundFrequency&, BtSoundChannel&, bool&),
 * vtable slot 26 on the transmitter client. Negative = nothing connected / the service wrote
 * nothing, which clears it. Returns 1 if the value changed (repaint), 0 otherwise.
 *
 * This is NOT the codec preference. cinder_get_bt_codec() is what the user asked for; this is what
 * the radio agreed to, and the two differ whenever a sink cannot do the requested codec — A2DP
 * negotiates during connection setup and falls back without saying so.
 *
 * Known enumerator: 0x02 = LDAC (measured on device 2026-08-17, WH-1000XM4, peer advertising
 * `ldac support:1` and neither aptX). It is Sony's own enum, NOT the Bluetooth assigned-numbers
 * codec ID — 0x02 there is MPEG-2/4 AAC and that is not what this is. The UI prints any other
 * value as raw hex rather than guessing. */
int  cinder_set_bt_link_codec(int raw);
/* Paired-device list for the Devices screen. Call cinder_bt_paired_clear(), then _add() once per
 * device from GetPairedDeviceInfo — IN THE SAME ORDER the shell keeps its BD addresses, because the
 * UI hands back a row index and nothing else. `kind` may be NULL. connected != 0 marks the live link.
 * cinder_pending_bt_device() DRAINS the row index that came with the last CONNECT/FORGET action and
 * returns -1 when there is none (never replay a forget against whatever later occupies that row). */
void cinder_bt_paired_clear(void);
void cinder_bt_paired_add(const char* name, const char* kind, int connected);
int  cinder_bt_paired_count(void);
int  cinder_pending_bt_device(void);
/* Discovered-device list for the Devices screen's FOUND section — same index-is-the-handle contract as
 * the paired list. Clear when a scan starts, then one _add per device the listener reports. */
void cinder_bt_found_clear(void);
void cinder_bt_found_add(const char* name, const char* kind);
int  cinder_bt_found_count(void);
/* Scan state: the shell reads it to know which way to drive SetSearchMode, and writes it when the
 * radio's own search window expires (the UI does not assume its tap stuck). */
/* Pairing prompt: kind 1 = numeric comparison (yes/no), 2 = passkey (display only), 3 = SSP request.
 * _clear() takes the panel down. _kind() reports what is showing (0 = nothing). */
void cinder_bt_prompt_set(int kind, const char* name, unsigned code);
void cinder_bt_prompt_clear(void);
int  cinder_bt_prompt_kind(void);
int  cinder_get_bt_scanning(void);
void cinder_set_bt_scanning(int on);
/* Top of the Bluetooth volume scale (must match cinder_ui::overlay::BT_VOL_MAX). A step count, not
 * a mixer value: the shell maps it onto AVRCP's 0..127. Was 30 until 2026-08-11, which made one
 * press ~4.2 AVRCP units; 64 makes it ~2. */
#define CINDER_BT_VOL_MAX 64
/* Bluetooth output volume, a SEPARATE level on its own 0..30 AVRCP-step scale. `cinder_get_volume`
 * above stays the 3.5 mm codec level even while headphones are connected, so the two routes never
 * overwrite each other's setting. */
int  cinder_get_bt_volume(void);
void cinder_set_bt_volume(int level);
/* Which output the volume rocker drives: nonzero = Bluetooth, 0 = the 3.5 mm jack. The shell owns
 * this (only it can see the radio) and pushes it whenever the connection state changes. Setting it
 * moves neither level — it selects which one the next Vol± press adjusts and which one the HUD
 * shows. */
void cinder_set_bt_route(int on);
int  cinder_get_bt_route(void);
/* Does the visualiser want the analyzer streaming?1 when it is enabled, Now Playing is showing,
 * and audio is actually playing. The shell polls this and starts/stops Sony's AudioAnalyzerService
 * to match, so the FFT only runs while its output is visible. */
int  cinder_viz_wants_analyzer(void);
/* Is night theme active? (1/0). Call after a CINDER_ACT_THEME_CHANGED action (and at boot) to set
 * the panel backlight — night = minimal light. */
int  cinder_get_night(void);
/* Device-wide BT transmit codec preference (0 LDAC, 1 aptX HD, 2 aptX, 3 SBC) + LDAC quality tier
 * (0 Auto, 1 990, 2 660, 3 330). Read after a CINDER_ACT_BT_CODEC_CHANGED action (and at boot), then
 * apply via BtTransmitterService. The same values configure the USB-DAC→LDAC bridge. */
int  cinder_get_bt_codec(void);
int  cinder_get_bt_ldac_quality(void);
/* "Use Enhanced Mode" (1/0) — AVRCP absolute volume. Read after CINDER_ACT_BT_ENHANCED_CHANGED,
 * at boot, and after every reconnect (the radio does not carry it across a link), then hand it to
 * SetControlAbsoluteVolume(const bool&) (slot 31). */
int  cinder_get_bt_enhanced(void);
/* Push back what the CONNECTED sink can actually do: IsSupportedAbsoluteVolume() (slot 33).
 * Returns 1 if the Bluetooth screen changed and needs a repaint. */
int  cinder_set_bt_enhanced_supported(int on);
/* Did a queue flush become ready at the last track boundary? Clears on read. When it returns 1,
 * drain cinder_pending_play_* and hand the result to PlayerService exactly as for a normal play
 * request: the sequence is rebuilt with the track that just started at index 0, followed by the
 * user queue. Deferred to a boundary because a SetTrackSequence mid-track RESTARTS the sequence
 * (device-measured 2026-07-28: position 9000 -> 0 and playback stopped). */
int  cinder_take_queue_flush(void);

/* Repeat-one state (1/0) for CINDER_ACT_REPEAT_CHANGED. */
int  cinder_get_repeat_one(void);
/* The UI's panel-brightness level, 1..5 (never 0). Read after a CINDER_ACT_BRIGHTNESS_CHANGED
 * action and at boot, then map it onto the backlight node. Level 1 must stay READABLE — if the
 * lowest setting blanks the panel, the screen needed to turn it back up is unusable. */
int  cinder_get_brightness(void);
/* Idle screen-off timeout in SECONDS; 0 = disabled (the default). The shell owns the countdown
 * because only it sees every input event. Read after CINDER_ACT_SCREEN_OFF_CHANGED and at boot. */
int  cinder_get_screen_off_s(void);
/* Auto power-off in MINUTES (0 = off). Poll from the ~1 Hz housekeeping: when it is non-zero and
 * nothing is playing and there has been no input for that long, shut the device down. Distinct from
 * the screen-off timer, which only blanks the panel. */
int  cinder_get_auto_off_min(void);
/* L/R balance position, 0..=100 with 50 = centre (a continuous slider). Read after a
 * CINDER_ACT_BALANCE_CHANGED or CINDER_ACT_SOUND_CHANGED action and write the codec's
 * `l balance volume` / `r balance volume` (0..88 of ATTENUATION, in HALF-decibels): panning left
 * turns the RIGHT channel down, because the mixer only offers attenuation. */
int  cinder_get_balance(void);
/* Is USB-DAC mode engaged? (1/0). Read after a CINDER_ACT_USBDAC_LDAC action to start/stop the LDAC
 * bridge + switch the USB gadget to UAC, without disconnecting Bluetooth. */
/* Is the Bluetooth switch on? (1/0). Read after a CINDER_ACT_BT_TOGGLE action. */
int  cinder_get_bt_on(void);
/* Force the Bluetooth switch to match the radio's real state (from GetBtStatus). Sets state only,
   raises no action. Call at startup so the switch cannot claim the radio is on when it is not. */
void cinder_set_bt_on(int on);
int  cinder_get_usb_dac(void);
/* Force the USB-DAC toggle to match the gadget's real mode (from sys.sony.config). Sets state
   only — raises no action, since the gadget is already there. Call at startup to stop Settings
   reporting the opposite of the hardware after a mode change made outside Cinder. */
void cinder_set_usb_dac(int on);
/* Publish the host's live USB stream format for the USB-DAC panel: rate in Hz, bit depth, channels
   (from Sony's stream_info_t, the three words GetStatus fills in). rate 0 = not streaming, which
   clears the panel back to its generic line. Sets state only, raises no action. */
void cinder_set_usb_dac_format(unsigned rate, unsigned bits, unsigned chans);
/* Publish the codec A2DP actually negotiated (raw BtSoundCodec word from GetSoundStatus; 0 = not
   known). The UI shows a neutral label until the enumerators are tied to a real headphone. */
void cinder_set_bt_negotiated_codec(int raw);
/* Read the Sound A/B compare state (1 = B/bypassed, 0 = A/active). Call after a
 * CINDER_ACT_SOUND_BYPASS action, then apply via cinder_effects_set_bypass. */
int  cinder_get_sound_bypass(void);
/* Visualiser: enable/disable the animation, select the type, and query how many types exist. */
void cinder_set_visualizer(int on);
void cinder_set_visualizer_type(int kind);
int  cinder_visualizer_count(void);
/* Feed a mono PCM window (i16) for a REAL audio-reactive visualiser (FFT'd into the bars).
 * Use only for a raw-PCM tap with no analyzer (e.g. the USB-DAC path). */
void cinder_set_pcm(const short *samples, int n);
/* PREFERRED real-data path: feed Sony's already-FFT'd spectrum bands (the int vector from
 * AudioAnalyzerService::OnSpectrumUpdate) — no FFT cost on our side. `n` source bands are
 * resampled into the visualiser bars and auto-normalised. The analyzer shim calls this. */
void cinder_set_spectrum(const int *bands, int n);

#ifdef __cplusplus
}
#endif
#endif /* CINDER_H */
