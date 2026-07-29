//! Show what `Db::playlists()` / `Db::playlist_tracks()` return for a real device DB.
//! `cargo run -p cinder-db --example playlists -- <MTPDB.dat>`

use cinder_db::Db;

fn main() {
    let path = std::env::args().nth(1).expect("usage: playlists <MTPDB.dat>");
    let db = Db::open(&path).expect("open");
    let pls = db.playlists().expect("playlists");
    println!("{} playlist(s)", pls.len());
    for p in &pls {
        println!("\n== {} (id {}, {} tracks)", p.name, p.id, p.track_count);
        for (i, t) in db.playlist_tracks(p.id).expect("tracks").iter().enumerate().take(5) {
            println!("   {:>2}. {} — {}", i + 1, t.title, t.artist);
        }
    }
}
