//! Runs the real App renderer in both languages without a device or Sony IPC.
use cinder_ui::i18n::Language;
use cinder_ui::nav::{Action, App, Screen};
use cinder_ui::now_playing::NowPlaying;
use cinder_ui::{Canvas, FontSet};
fn np_hostile() -> NowPlaying<'static> {
    NowPlaying {
        title: "Sinfonia concertante for Violin, Viola and Orchestra in E-flat major, K. 364 — III. Presto",
        artist: "Королевский филармонический оркестр / 東京都交響楽団 feat. A Very Long Guest Artist Name",
        codec: "FLAC · 24bit / 192.0 kHz · Hi-Res Audio Wireless",
        badge: "FLAC 24/192",
        clock: "23:59",
        battery: 100,
        elapsed: "1:47:22",
        remaining: "-2:45:09",
        progress: 0.39,
        art: "kind",
        art_full: None,
        art_thumb: None,
        liked: true,
        playing: true,
        shuffle: true,
        repeat: 1,
        viz_seed: 2.0,
        viz_kind: 0,
        viz_size: 1,
        page: 0,
        viz_levels: None,
        viz_peaks: None, viz_sig: None,
        scrubbing: false, lyrics: false,
    }
}

fn main() {
    let fonts = FontSet::load();
    let mut np = np_hostile();
    np.playing = false;
    let mut app = App::unlocked();
    app.push_for_test(Screen::Settings);
    let mut english = Canvas::new();
    app.render(&mut english, &fonts, &np);
    app.set_language(Language::SimplifiedChinese);
    let mut chinese = Canvas::new();
    app.render(&mut chinese, &fonts, &np);
    assert_ne!(
        english.buf, chinese.buf,
        "language change must repaint immediately"
    );
    app.set_language(Language::English);
    let mut restored = Canvas::new();
    app.render(&mut restored, &fonts, &np);
    assert_eq!(
        english.buf, restored.buf,
        "English must return without restarting"
    );
    assert_eq!(app.language().token(), "en");
    for language in [Language::English, Language::SimplifiedChinese] {
        let mut fm = App::unlocked();
        fm.set_language(language);
        fm.push_for_test(Screen::Fm);
        let mut canvas = Canvas::new();
        fm.render(&mut canvas, &fonts, &np);
        assert_eq!(fm.tap(400, 63), vec![Action::FmPower(true)]);
        assert_eq!(fm.tap(400, 63), vec![Action::FmPower(false)]);
    }
    println!("LANGUAGE_SMOKE_PASS: immediate switch, English round trip, stable FM actions");
}
