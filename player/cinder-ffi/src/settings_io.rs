//! The settings file: what is saved (`settings_body`), where (`save_settings`, the mirror copy on
//! /contents), and `cinder_settings_load`, which reads it back at boot.
//!
//! Moved out of lib.rs (2026-11-24); behaviour unchanged.

use super::*;

pub(crate) fn settings_body(r: &Render) -> String {
    let eq: Vec<String> = r.app.eq_bands().iter().map(|b| b.to_string()).collect();
    let mut body = format!(
        "night={}\naccent={}\nviz_kind={}\nviz_size={}\nnp_page={}\nshuffle={}\nrepeat={}\neq={}\nsound={}\nonboarding={}\nbt_codec={}\nbt_ldac_quality={}\nbt_enhanced={}\nbt_on={}\nvolume={}\nbt_volume127={}\nbrightness={}\nscreen_off={}\nauto_off={}\nbalance100={}\nmono={}\nvpt_mode={}\ndc_type={}\nadv={}\ndsee_mode={}\nvinyl_type={}\ntone={}\nbig_view={}\nui_scale={}\nsetup={}\nclear_bass={}\n",
        r.app.night as u8,
        r.app.accent(),
        r.app.viz_kind(),
        r.app.viz_size(),
        r.app.np_page(),
        r.np.shuffle as u8,
        r.np.repeat,
        eq.join(","),
        r.app.sound_flags(),
        r.app.onboarding_seen() as u8,
        r.app.bt_codec(),
        r.app.bt_ldac_quality(),
        r.app.bt_enhanced() as u8,
        r.app.bt_on() as u8,
        r.app.volume_level(),
        r.app.bt_volume_level(),
        r.app.brightness_restore(), // never 0: backlight-off is transient, not a setting
        r.app.screen_off_s(),
        r.app.auto_off_min(),
        r.app.balance(),
        u8::from(r.app.mono()),
        r.app.vpt_mode(),
        r.app.dc_type(),
        r.app.adv_flags(),
        r.app.dsee_mode(),
        r.app.vinyl_type(),
        r.app.tone_bands().iter().map(|b| b.to_string()).collect::<Vec<_>>().join(","),
        // Before ui_scale on purpose: loading big_view=0 puts a 130 % text size back to 100 %, so
        // the saved size has to be read after it.
        u8::from(r.app.big_view()),
        r.app.ui_scale_pct(),
        r.app.setup_idx(),
        r.app.clear_bass_step(),
    );
    body.push_str(&format!("rx_codec={}\n", r.app.rx_codec_pref()));
    body.push_str(&format!("language={}\n", r.app.language().token()));
    body.push_str(&setup_body(&r.app.setup_inactive()));
    // Saved presets (Sound ▸ Presets): the spare's keys, numbered — `preset1_eq=` and so on.
    body.push_str(&presets_body(r.app.presets(), r.app.preset_names()));
    // The sound an album's preset replaced, so a restart mid-album still puts it back after.
    if let Some(prev) = r.app.album_sound_prev() {
        body.push_str(&setup_body(prev).replace("bank_", "album_prev_"));
    }
    for (key, n) in r.app.album_sounds() {
        body.push_str(&format!("album_sound={n}\t{}\n", key.replace('\n', " ")));
    }
    body.push_str(&format!(
        "jack_eq={}\n",
        if r.app.jack_eq_dac() { "dac" } else { "10band" }
    ));
    // Noise cancelling: indices into cinder_ui::nc::MODES / MODELS.
    body.push_str(&format!(
        "nc_mode={}\nnc_model={}\n",
        r.app.nc_mode(),
        r.app.nc_model()
    ));
    // Which of mono (1), linear amp (2) and DAC EQ (4) travel with a sound. 0 = none.
    body.push_str(&format!("profile_follow={}\n", r.app.profile_follow()));
    // The visualiser's signal settings. One line each rather than a packed field, because these
    // are exactly the lines someone tuning the display over adb will want to edit by hand — and
    // every one is an INDEX into a table owned by `cinder_ui::vizcfg`, so an out-of-range value
    // from a hand-edited file is wrapped by the setter rather than accepted.
    // The Bluetooth fine-volume SPAN, not the trim: the trim is a live attenuation the user cannot
    // see in any menu, and restoring one at boot would be a device that plays quiet for reasons
    // nothing on screen explains.
    body.push_str(&format!("bt_fine={}\n", r.app.bt_fine_span()));
    body.push_str(&format!("bt_idle_off={}\n", r.app.bt_idle_off() as u8));
    body.push_str(&format!("volume_limit={}\n", r.app.volume_limit() as u8));
    body.push_str(&format!("ignore_the={}\n", r.app.ignore_the() as u8));
    let (on, min) = r.app.alarm_saved();
    body.push_str(&format!(
        "alarm={}\nalarm_at={}\n",
        on as u8,
        cinder_ui::alarm::hhmm(min)
    ));
    // The opt-in switches (pull-down panel, Folders tab, the Wi-Fi options, …), OFF unless on.
    for o in Opt::ALL {
        if o == Opt::LineOut {
            continue; // never kept across a restart (opt.rs)
        }
        body.push_str(&format!("{}={}\n", o.key(), r.app.opt(o) as u8));
    }
    body.push_str(&format!("wifi_drive_code={}\n", r.app.drive_code()));
    // Settings ▸ Swipe up / Swipe down: what each opens on Now Playing (#51), as words.
    let (up, down) = r.app.np_swipes();
    body.push_str(&format!(
        "np_swipe_up={}\nnp_swipe_down={}\n",
        up.token(),
        down.token()
    ));
    body.push_str(&format!("np_button={}\n", r.app.np_button().token()));
    // Night's schedule. Night itself is not saved (every boot starts in day, see "night" in the
    // loader); with the schedule on, the first clock tick puts it where the hour says.
    let (on, from, until) = r.app.night_schedule();
    body.push_str(&format!(
        "night_sched={}\nnight_from={from}\nnight_until={until}\n",
        on as u8
    ));
    // Big view's two extras (#51): the bigger top bar and bigger icons.
    body.push_str(&format!(
        "big_bar={}\nbig_icons={}\n",
        cinder_ui::scale::big_bar_set() as u8,
        cinder_ui::scale::big_icons_set() as u8
    ));
    // Library ▸ the header's view button: each tab's layout, Songs to Playlists, as words.
    body.push_str(&format!("lib_views={}\n", r.app.lib_views_str()));
    body.push_str(&format!("lib_place={}\n", r.app.lib_place_str()));
    body.push_str(&format!(
        "sensme_follow_time={}\n",
        r.app.sensme_follow() as u8
    ));
    // Menu ▸ Soundscapes: the sound is kept while switched off, so the switch brings it back.
    let (_, alone, music) = r.app.ambient();
    body.push_str(&format!(
        "ambient={}\nambient_on={}\nambient_levels={},{}\n",
        r.app.ambient_sound(),
        r.app.ambient_on() as u8,
        alone,
        music
    ));
    // Settings ▸ Shuffle: what the shuffle button deals, as a word.
    body.push_str(&format!("shuffle_by={}\n", r.app.shuffle_by()));
    // Sound ▸ Advanced ▸ Volume curve: Sony's step at volume 0, 20 … 120, and whether it is used.
    let (vc, vc_on) = r.app.vol_curve_points();
    body.push_str(&format!(
        "vol_curve={}\nvol_curve_on={}\n",
        vc.iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(","),
        vc_on as u8
    ));
    // Sound ▸ Advanced ▸ DAC EQ, RAW half-decibels in the helper's argument order.
    body.push_str(&format!(
        "dac_eq={}\n",
        r.app
            .dac_eq()
            .iter()
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(",")
    ));
    // Settings ▸ Display ▸ Volume: `full` or `minimal`, as a word — the handoff names the key and
    // its values, and a word survives a future third style where an index would shift.
    body.push_str(&format!("volume_hud={}\n", r.app.volume_hud()));
    body.push_str(&format!("style={}\n", r.app.style_token()));
    if !r.skin_pref.is_empty() {
        body.push_str(&format!("wsz={}\n", r.skin_pref));
    }
    if !r.hpeq_pref.is_empty() {
        body.push_str(&format!("hpeq={}\n", r.hpeq_pref));
    }
    body.push_str(&format!("wa={}\n", r.app.winamp_prefs()));
    if !r.tape_pref.is_empty() {
        body.push_str(&format!("tape={}\n", r.tape_pref));
    }
    // What the player opens on (the Menu's START ON chips), and — only when that is "last" — the
    // place to reopen, encoded like a Shelf pin.
    body.push_str(&format!("home_screen={}\n", r.app.home_screen()));
    let last = r.app.home_last_encode();
    if !last.is_empty() {
        body.push_str(&format!("home_last={last}\n"));
    }
    // The palette by id — the CHOICE, not what happens to be drawn. If the folder could not be read
    // this boot, Cinder is on screen, but the palette the user picked is still the one to keep.
    body.push_str(&format!("palette={}\n", r.app.palette_id()));
    body.push_str(&format!("palette_sort={}\n", r.app.palette_sort()));
    body.push_str(&format!(
        "viz_scale={}\nviz_range={}\nviz_response={}\nviz_interp={}\nviz_peaks={}\nviz_window={}\nviz_rate={}\nviz_bands={}\n",
        r.app.viz_scale_idx(),
        r.app.viz_range_idx(),
        r.app.viz_response_idx(),
        r.app.viz_interp_idx(),
        r.app.viz_peak_hold() as u8,
        r.app.viz_window_idx(),
        r.app.viz_rate_idx(),
        r.app.viz_bands_idx(),
    ));
    // FM: the dial position and the scanned station list. A scan is a DELIBERATE ten-second wait
    // that the user watches happen, so losing it on a reboot is the same defect as losing a shelf
    // pin — and losing the frequency drops the dial to a hardcoded 97.3 that is nobody's station.
    // Written unconditionally (the dial always has a value) and the list only when a scan has run.
    body.push_str(&format!("fm_khz={}\n", r.app.fm_khz()));
    let st = r.app.fm_stations();
    if !st.is_empty() {
        body.push_str(&format!(
            "fm_stations={}\n",
            st.iter()
                .map(|k| k.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    // Shelf pins were session-scoped, so every reboot silently wiped the user's bookmarks — the
    // one thing a "pin this place" feature must not do. One line per occupied slot.
    for i in 0..cinder_ui::shelf::SLOTS {
        let enc = r.app.shelf_pin_encode(i);
        if !enc.is_empty() {
            body.push_str(&format!("pin{i}={enc}\n"));
        }
    }
    body
}

// Write the preferences to the configured file IF they changed since the last write (cheap body
// compare → most presses don't write). Best-effort: IO errors (RO/full fs) are ignored.
pub(crate) fn save_settings(r: &mut Render) {
    if r.settings_path.is_none() {
        return;
    }
    // The saved views ride the same save points: every tap, press and swipe ends here.
    save_views(r);
    let body = settings_body(r);
    if body == r.last_saved_body {
        return;
    }
    if let Some(path) = r.settings_path.clone() {
        let _ = write_atomic(&path, &body);
        if let Some(m) = settings_mirror(&path) {
            let _ = write_atomic(m, &body);
        }
        r.last_saved_body = body;
    }
}

/// The first frame paints at onPostInitialize, about 10 s into boot, which is inside the window
/// where the USB gadget binds /contents and reads of it fail. So the settings are kept twice: the
/// copy on /data is what a boot reads when /contents cannot be read yet. Only for the real
/// /contents file, so host tests never touch /data.
pub(crate) fn settings_mirror(p: &str) -> Option<&'static str> {
    p.starts_with("/contents/")
        .then_some("/data/cinder/cinder_settings.conf")
}

/// The settings file, or its mirror when the file cannot be read. A good read refreshes the
/// mirror, so a file edited over USB is what the next early boot sees.
pub(crate) fn read_settings(p: &str, mirror: Option<&str>) -> std::io::Result<String> {
    match (std::fs::read_to_string(p), mirror) {
        (Ok(body), Some(m)) => {
            if std::fs::read_to_string(m).ok().as_deref() != Some(body.as_str()) {
                let _ = write_atomic(m, &body);
            }
            Ok(body)
        }
        (Err(e), Some(m)) => {
            let body = std::fs::read_to_string(m)?;
            eprintln!("cinder-ffi: {p} could not be read ({e}); settings from {m}");
            Ok(body)
        }
        (res, None) => res,
    }
}

/// Load persisted UI preferences (theme + visualiser + EQ + sound effects + volume) from `path`,
/// apply them, and remember the path so later changes auto-save. Call once at boot after
/// cinder_render_init. Returns a bitmask: bit0 = a settings file was read (shell should re-apply
/// EQ/sound to the DSP), bit1 = a persisted volume level was restored (shell should apply it to
/// the mixer instead of seeding the UI from hardware). 0 = no file. Best-effort: a
/// missing/garbage file is ignored (defaults stand); robust line parser.
#[no_mangle]
pub extern "C" fn cinder_settings_load(path: *const c_char) -> libc::c_int {
    let p = unsafe { cstr(path) };
    if p.is_empty() {
        return 0;
    }
    let mut loaded = 0;
    // Big view is the default (owner 11-24): on unless the file says `big_view=0`, so a first boot
    // (no file) and a file from before the switch existed both start Big; a saved choice is kept.
    let mut saw_big = false;
    if let Some(r) = cell().lock().unwrap().as_mut() {
        // Before the saved choice, which may only pick among them.
        r.app.set_languages(&installed_languages(LANGUAGES_PATH));
        if let Ok(body) = read_settings(&p, settings_mirror(&p)) {
            loaded = 1;
            // The spare A/B setup, accumulated across however many `bank_*` lines the file has and
            // installed once at the end — the keys can arrive in any order, and the live setup must
            // be in place before the spare is banked beside it.
            let mut pl = ProfileLoad::default();
            for line in body.lines() {
                let mut it = line.splitn(2, '=');
                let k = it.next().unwrap_or("").trim();
                let v = it.next().unwrap_or("").trim();
                match k {
                    "language" => r.app.set_language(cinder_ui::i18n::Language::from_token(v)),
                    // NIGHT IS NOT RESTORED. It is still WRITTEN to the settings file (one line
                    // below), because losing it would mean losing the accent/theme pair a user set
                    // — but it is deliberately not read back, so every boot starts in day.
                    //
                    // Night is now the panel's dimmest lit state, a flat raw 1 at every brightness
                    // level, and it is the thing you reach for in a dark room. A dark-room setting
                    // that survives into the next morning is a screen you cannot read outdoors,
                    // set by a decision you made hours ago and have forgotten making. The backlight
                    // already forced day on boot for exactly this reason; the theme was the half
                    // that did not, so the panel came up bright while the palette stayed dark.
                    "night" => {}
                    "accent" => {
                        // set_accent snaps an unknown index to the default, so a hand-edited or
                        // corrupt value can't leave the UI on a colour the picker can't reach.
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_accent(n);
                        }
                    }
                    // By id. A palette that is not loaded (yet) keeps the choice and draws Cinder
                    // until the folder is read — see App::set_palette_wanted.
                    "palette" => r.app.set_palette_wanted(v),
                    "palette_sort" => r.app.set_palette_sort(v),
                    "viz_kind" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_kind(n);
                        }
                    }
                    // Written by builds before the visualiser gained sizes. Map it so an upgrade
                    // keeps the user's choice instead of silently resetting it: on => FULL (the
                    // only "on" that existed), off => OFF. A `viz_size` line, if present, is read
                    // after this and wins.
                    "viz_on" => r.app.set_viz_on(v == "1"),
                    "viz_size" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_size(n);
                        }
                    }
                    // Settings ▸ Visualiser. Each setter wraps its index into range, so a file
                    // written by a newer build (or by hand) can never select an option that does
                    // not exist in this one.
                    "bt_fine" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_bt_fine_span(n);
                        }
                    }
                    "volume_limit" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_volume_limit(n != 0);
                        }
                    }
                    // Settings ▸ Ignore "The" in artists. Read before the library arrives, and
                    // `set_library` applies it to whatever library comes next.
                    "ignore_the" => r.app.set_ignore_the(v == "1"),
                    "alarm" => {
                        let (_, min) = r.app.alarm_saved();
                        r.app.set_alarm(v == "1", min);
                    }
                    "alarm_at" => {
                        if let Some(min) = cinder_ui::alarm::parse_hhmm(v) {
                            let (on, _) = r.app.alarm_saved();
                            r.app.set_alarm(on, min);
                        }
                    }
                    "wifi_drive_code" => r.app.set_drive_code(v),
                    "big_bar" => cinder_ui::scale::set_big_bar(v == "1"),
                    "big_icons" => cinder_ui::scale::set_big_icons(v == "1"),
                    "night_sched" | "night_from" | "night_until" => {
                        let (mut on, mut from, mut until) = r.app.night_schedule();
                        match k {
                            "night_sched" => on = v == "1",
                            "night_from" => from = v.parse().unwrap_or(from),
                            _ => until = v.parse().unwrap_or(until),
                        }
                        r.app.set_night_schedule(on, from, until);
                    }
                    "np_button" => r
                        .app
                        .set_np_button(cinder_ui::now_playing::NpButton::from_token(v)),
                    "np_swipe_up" | "np_swipe_down" => {
                        let (mut up, mut down) = r.app.np_swipes();
                        let t = cinder_ui::settings::SwipeTo::from_token(v);
                        if k == "np_swipe_up" {
                            up = t;
                        } else {
                            down = t;
                        }
                        r.app.set_np_swipes(up, down);
                    }
                    "lib_views" => r.app.set_lib_views_str(v),
                    "lib_place" => r.app.set_lib_place_str(v),
                    "style" => r.app.set_style_token(v),
                    "wsz" => r.skin_pref = v.to_string(),
                    "hpeq" => r.hpeq_pref = v.to_string(),
                    "tape" => r.tape_pref = v.to_string(),
                    "wa" => r.app.set_winamp_prefs(v),
                    "sensme_follow_time" => r.app.set_sensme_follow(v == "1"),
                    "shuffle_by" => r.app.set_shuffle_by(v),
                    "volume_hud" => r.app.set_volume_hud(v),
                    "home_screen" => r.app.set_home_screen(v),
                    "home_last" => r.app.set_home_last(v),
                    "viz_scale" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_scale(n);
                        }
                    }
                    "viz_range" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_range(n);
                        }
                    }
                    "viz_response" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_response(n);
                        }
                    }
                    "viz_interp" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_interp(n);
                        }
                    }
                    "viz_peaks" => r.app.set_viz_peak_hold(v == "1"),
                    "viz_window" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_window(n);
                        }
                    }
                    "viz_rate" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_rate(n);
                        }
                    }
                    "viz_bands" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_bands(n);
                        }
                    }
                    "np_page" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_np_page(n);
                        }
                    }
                    // Shuffle and repeat persist now that they do something. Losing them on every
                    // reboot would mean re-enabling shuffle each morning, and everything else on
                    // this screen already survives a restart.
                    "shuffle" => r.np.shuffle = v == "1",
                    // Clamped: only 0 and 1 exist (repeat-all has no primitive), so a stale file
                    // written by a build that cycled three states cannot restore a dead value.
                    // 0 = off, 1 = one, 2 = all. Was `v == "1"`, which silently collapsed a
                    // saved repeat-all back to off on the next boot.
                    "repeat" => r.np.repeat = v.parse::<u8>().unwrap_or(0).min(REPEAT_ALBUM),
                    "eq" => {
                        let mut arr = r.app.eq_bands();
                        for (i, part) in v.split(',').enumerate().take(10) {
                            if let Ok(g) = part.trim().parse::<i8>() {
                                // CLAMPED, because this is not the UI. Every place the EQ screen
                                // writes a band clamps to ±BAND_MAX, so the values Cinder itself
                                // produces are always in range — but this file is on /contents,
                                // which is vfat and writable by any PC the player is plugged into,
                                // and `i8` parses anything from -128 to 127.
                                //
                                // An out-of-range gain does NOT clamp inside the DSP: Sony's
                                // SetEq10BandValue ZEROES the band instead (measured; the scale is
                                // half-dB, so ±20 is ±10 dB). So a hand-edited or corrupted line
                                // silently flattens that band rather than pinning it to the
                                // maximum, and the EQ screen would draw its knob outside the field
                                // it belongs to. It would also be written straight back out on the
                                // next save, so the bad value persists.
                                arr[i] = g.clamp(-crate::EQ_BAND_MAX, crate::EQ_BAND_MAX);
                            }
                        }
                        r.app.set_eq_bands(arr);
                    }
                    "sound" => {
                        if let Ok(f) = v.parse::<u8>() {
                            r.app.set_sound_flags(f);
                        }
                    }
                    "onboarding" => r.app.set_onboarding_seen(v == "1"),
                    "bt_codec" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_bt_codec(n);
                        }
                    }
                    "bt_ldac_quality" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_bt_ldac_quality(n);
                        }
                    }
                    // Missing in older files means Auto. Malformed and out-of-range values also
                    // fall back to Auto rather than wrapping into a different receiver policy.
                    "rx_codec" => r.app.set_rx_codec_pref(v.parse::<u8>().unwrap_or(0)),
                    "bt_enhanced" => r.app.set_bt_enhanced(v == "1"),
                    "bt_idle_off" => r.app.set_bt_idle_off(v == "1"),
                    // The RADIO's own on/off. Everything else about Bluetooth persisted already —
                    // codec, LDAC quality, enhanced mode, volume — but not whether the radio was
                    // ON, so every boot came up with it off and nothing could connect until the
                    // switch was toggled by hand (measured 2026-08-26: GetBtStatus=7 on a fresh
                    // boot with three devices paired). bit2 says the file actually CARRIED this,
                    // which matters because the in-app default is ON: without that distinction the
                    // shell would power the radio up for someone who deliberately keeps it off.
                    "bt_on" => {
                        r.app.set_bt_on(v == "1");
                        loaded |= 4;
                    }
                    "volume" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_volume(n);
                            loaded |= 2; // bit1: a persisted volume level was restored
                        }
                    }
                    // LEGACY KEY, on the old 0..30 scale. Written by every build before
                    // 2026-08-11; reading it as 0..64 would halve the stored level on upgrade, so
                    // it is rescaled. New builds write `bt_volume64` instead, and when both keys
                    // are present that one wins (the file is parsed in order and it is written
                    // after this one).
                    "bt_volume" => {
                        if let Ok(n) = v.parse::<u16>() {
                            let scaled = (n * crate::VOL_BT_MAX as u16
                                / cinder_ui::overlay::BT_VOL_MAX_LEGACY as u16)
                                as u8;
                            r.app.set_bt_volume(scaled);
                        }
                    }
                    // Deliberately does NOT set bit1. That bit means "push this to the hardware",
                    // and the sink keeps its own volume across reconnects — so this is restored as
                    // the UI's BELIEF about where the headphones are, and it can be stale until
                    // the rocker moves or the sink reports its real level.
                    // LEGACY KEY, on the 0..64 scale used between 2026-08-11 and 2026-08-18.
                    // Same reasoning as `bt_volume` above, one scale later: read as 0..127 it would
                    // halve the stored level. New builds write `bt_volume127`.
                    "bt_volume64" => {
                        if let Ok(n) = v.parse::<u16>() {
                            let scaled = (n * crate::VOL_BT_MAX as u16
                                / cinder_ui::overlay::BT_VOL_MAX_LEGACY_64 as u16)
                                as u8;
                            r.app.set_bt_volume(scaled);
                        }
                    }
                    // Deliberately does NOT set bit1. That bit means "push this to the hardware",
                    // and the sink keeps its own volume across reconnects — so this is restored as
                    // the UI's BELIEF about where the headphones are, and it can be stale until
                    // the rocker moves or the sink reports its real level.
                    "bt_volume127" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_bt_volume(n);
                        }
                    }
                    // The OLD key, from when balance was 7 discrete stops (0..=6, 3 = centre).
                    // Migrated onto the 0..=100 slider so an upgrade doesn't silently slam a
                    // centred player hard left — "3" means centre in the old scale and "pan left
                    // by a third" in the new one, so the value cannot be reinterpreted in place.
                    // Written back under `balance100`, and this key is never written again.
                    "balance" => {
                        if let Ok(n) = v.parse::<usize>() {
                            let old = n.min(6);
                            r.app.set_balance(old * cinder_ui::sound::BALANCE_MAX / 6);
                        }
                    }
                    "balance100" => {
                        if let Ok(n) = v.parse::<usize>() {
                            r.app.set_balance(n);
                        }
                    }
                    // MONO (accessibility). Absent from files written by older builds, which is
                    // fine — it stays off, which is what it was before the key existed.
                    "mono" => r.app.set_mono(v == "1"),
                    // Absent from older files: 0, the three stay one value for every output.
                    "profile_follow" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_profile_follow(n);
                        }
                    }
                    // Soundscapes. Absent from older files: off, rain, the default levels.
                    "ambient" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_ambient_sound(n);
                        }
                    }
                    "ambient_on" => r.app.set_ambient_on(v == "1"),
                    "ambient_levels" => {
                        let mut it = v.split(',').map(|x| x.trim().parse::<u8>());
                        if let (Some(Ok(a)), Some(Ok(m))) = (it.next(), it.next()) {
                            r.app.set_ambient_levels(a, m);
                        }
                    }
                    // Which VPT room. Absent from files written by older builds, which is fine —
                    // it just stays at 0 (Studio), and VPT's on/off still comes from `sound=`.
                    // set_vpt_mode clamps, so a hand-edited value cannot reach the device as an
                    // out-of-range enum.
                    "vpt_mode" => {
                        if let Ok(n) = v.parse::<usize>() {
                            r.app.set_vpt_mode(n);
                        }
                    }
                    // Which DC Phase filter. Same story as vpt_mode: absent in older files means
                    // 0, and set_dc_type clamps.
                    "dc_type" => {
                        if let Ok(n) = v.parse::<usize>() {
                            r.app.set_dc_type(n);
                        }
                    }
                    // Sound ▸ Advanced. All three absent in files written by older builds, which
                    // leaves the whole screen at its defaults (everything off, mode 0) — the state
                    // the device was in before the screen existed.
                    "adv" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_adv_flags(n);
                        }
                    }
                    "dsee_mode" => {
                        if let Ok(n) = v.parse::<usize>() {
                            r.app.set_dsee_mode(n);
                        }
                    }
                    "vinyl_type" => {
                        if let Ok(n) = v.parse::<usize>() {
                            r.app.set_vinyl_type(n);
                        }
                    }
                    "clear_bass" => {
                        if let Ok(n) = v.parse::<usize>() {
                            r.app.set_clear_bass_step(n);
                        }
                    }
                    "nc_mode" => {
                        if let Ok(n) = v.parse::<usize>() {
                            r.app.set_nc(n, r.app.nc_model());
                        }
                    }
                    "nc_model" => {
                        if let Ok(n) = v.parse::<usize>() {
                            r.app.set_nc(r.app.nc_mode(), n);
                        }
                    }
                    // Tone Control bands, RAW half-decibels. Absent in older files = flat, which
                    // is what the device had before the editor existed. set_tone_bands clamps, so
                    // a hand-edited value cannot reach the service out of range — past the end it
                    // ZEROES the band rather than clamping, so an unchecked value would read as a
                    // boost in the UI and be silence in the DSP.
                    "tone" => {
                        let mut b = [0i8; cinder_ui::tone::BANDS];
                        for (i, part) in v.split(',').take(cinder_ui::tone::BANDS).enumerate() {
                            if let Ok(n) = part.trim().parse::<i8>() {
                                b[i] = n;
                            }
                        }
                        r.app.set_tone_bands(b);
                    }
                    // Volume curve points; restore_vol_curve ignores one the helper would refuse.
                    // `vol_curve_on` comes after it in every file Cinder writes.
                    "vol_curve" => {
                        let mut p = cinder_ui::vol_curve::STRAIGHT;
                        for (i, part) in v.split(',').take(p.len()).enumerate() {
                            p[i] = part.trim().parse::<u8>().unwrap_or(u8::MAX);
                        }
                        let on = r.app.vol_curve().is_some();
                        r.app.restore_vol_curve(p, on);
                    }
                    "vol_curve_on" => {
                        let (p, _) = r.app.vol_curve_points();
                        r.app.restore_vol_curve(p, v == "1");
                    }
                    // DAC EQ bands. Absent in older files = flat, which is exactly the table Sony
                    // loads. set_dac_eq clamps: the helper REFUSES an out-of-range gain, so an
                    // unchecked hand edit would leave the whole EQ unapplied.
                    "dac_eq" => {
                        let mut b = [0i8; cinder_ui::dac_eq::BANDS];
                        for (i, part) in v.split(',').take(cinder_ui::dac_eq::BANDS).enumerate() {
                            if let Ok(n) = part.trim().parse::<i8>() {
                                b[i] = n;
                            }
                        }
                        r.app.set_dac_eq(b);
                    }
                    // The OTHER sound profile, which of the two was live, and which one each
                    // output uses (`setup`, `bank_*`, `profile_*`). Accumulated in `pl` and applied
                    // once at the end, because the keys can arrive in any order and the live
                    // setup has to be in place before the spare can be installed beside it.
                    _ if pl.line(k, v) => {}
                    "auto_off" => {
                        // set_auto_off_min snaps to a known preset, so a hand-edited value cannot
                        // strand the row on a duration the cycle can never reach.
                        if let Ok(n) = v.parse::<u32>() {
                            r.app.set_auto_off_min(n);
                        }
                    }
                    "screen_off" => {
                        // set_screen_off_s snaps to a known preset, so a hand-edited value can't
                        // leave the Settings row showing something it can't cycle away from.
                        if let Ok(n) = v.parse::<u32>() {
                            r.app.set_screen_off_s(n);
                        }
                    }
                    "brightness" => {
                        // set_brightness clamps to 1..5, so a corrupt or out-of-range file can
                        // never restore a level the shell would map to an unreadable screen.
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_brightness(n);
                        }
                    }
                    "big_view" => {
                        saw_big = true;
                        r.app.set_big_view(v == "1");
                    }
                    "ui_scale" => {
                        // set_ui_scale_pct clamps into the SCALE_STEPS range.
                        if let Ok(n) = v.parse::<u32>() {
                            r.app.set_ui_scale_pct(n);
                        }
                    }
                    // FM dial position. fm_report_khz clamps into the band, so a hand-edited or
                    // truncated value cannot put the tuner somewhere it cannot go.
                    "fm_khz" => {
                        if let Ok(n) = v.parse::<i32>() {
                            if n > 0 {
                                r.app.fm_report_khz(n);
                            }
                        }
                    }
                    // Scanned stations, strongest first. Anything unparseable is dropped rather
                    // than failing the load; a bad config must never stop a boot.
                    "fm_stations" => {
                        let list: Vec<i32> = v
                            .split(',')
                            .filter_map(|s| s.trim().parse::<i32>().ok())
                            .filter(|k| {
                                (cinder_ui::fm::MIN_KHZ..=cinder_ui::fm::MAX_KHZ).contains(k)
                            })
                            .collect();
                        if !list.is_empty() {
                            r.app.fm_set_stations(&list);
                        }
                    }
                    // Shelf pins: `pin0=`…`pin2=`. A malformed record clears that slot rather
                    // than failing the whole load — a hand-edited config must never stop a boot.
                    k if k.starts_with("pin") => {
                        if let Ok(i) = k[3..].parse::<usize>() {
                            r.app.shelf_pin_decode(i, v);
                        }
                    }
                    // The opt-in switches, by their keys in `cinder_ui::opt`.
                    _ => {
                        if let Some(o) = Opt::from_key(k) {
                            r.app.set_opt(o, v == "1");
                        }
                    }
                }
            }
            pl.install(&mut r.app);
            r.night = r.app.night;
            r.dirty = true;
        }
        r.palette_dir = std::path::Path::new(&p)
            .parent()
            .map(|d| d.join(cinder_ui::palette::DIR_NAME));
        scan_palettes(r);
        r.hpeq_dir = std::path::Path::new(&p)
            .parent()
            .map(|d| d.join(cinder_ui::hp_eq::DIR));
        // Also clears a control file a crash left behind when no profile is chosen.
        let pref = r.hpeq_pref.clone();
        if let Err(e) = hpeq_play(r, &pref) {
            eprintln!("cinder-ffi: headphone EQ {pref}: {e}");
        }
        if !saw_big {
            r.app.set_big_view(true);
        }
        r.settings_path = Some(p);
        r.last_saved_body = settings_body(r);
        // First run (intro not completed / no settings file yet) → show onboarding before anything.
        // Otherwise open on the home screen (Library unless the Menu's START ON says otherwise).
        if !r.app.onboarding_seen() {
            r.app.start_onboarding();
            r.dirty = true;
        } else {
            r.app.apply_home();
            r.dirty = true;
        }
    }
    loaded
}
