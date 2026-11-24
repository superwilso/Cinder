//! The backend thread: it owns the state and applies commands, and after each change it publishes
//! an immutable snapshot. Nothing here draws, and the UI never waits for it. A shell call only
//! sends a command (`send`), and reading state (`snapshot`) is a pointer clone.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// What the backend is asked to do. Each shell entry point that changes state becomes one of these.
pub enum Cmd {
    NowPlaying {
        uri: String,
        progress: f32,
        playing: bool,
        battery: u8,
    },
    /// Tests only: a slow command, standing in for a library reload or a file write.
    #[cfg(test)]
    Sleep(u64),
}

/// Everything the UI may read, as of one moment. Cheap to share: the UI holds an `Arc`.
#[derive(Clone, Default)]
pub struct Snapshot {
    /// Bumped on every publish, so the UI repaints only when something changed.
    pub version: u64,
    pub title: String,
    pub progress: f32,
    pub playing: bool,
    pub battery: u8,
}

pub struct Backend {
    tx: Mutex<Sender<Cmd>>,
    snap: Arc<Mutex<Arc<Snapshot>>>,
}

impl Backend {
    pub fn start() -> Backend {
        let (tx, rx) = channel();
        let snap = Arc::new(Mutex::new(Arc::new(Snapshot::default())));
        let out = snap.clone();
        std::thread::Builder::new()
            .name("cinder-backend".into())
            .spawn(move || run(rx, out))
            .expect("backend thread");
        Backend {
            tx: Mutex::new(tx),
            snap,
        }
    }

    /// Queue a command. Never blocks on the backend's work.
    pub fn send(&self, c: Cmd) {
        let _ = self.tx.lock().unwrap().send(c);
    }

    /// The latest published state. The lock is held only to clone a pointer.
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snap.lock().unwrap().clone()
    }
}

fn run(rx: Receiver<Cmd>, out: Arc<Mutex<Arc<Snapshot>>>) {
    let mut s = Snapshot::default();
    while let Ok(c) = rx.recv() {
        apply(&mut s, c);
        // Drain whatever else is queued before publishing, so a burst costs one snapshot.
        while let Ok(c) = rx.try_recv() {
            apply(&mut s, c);
        }
        s.version += 1;
        *out.lock().unwrap() = Arc::new(s.clone());
    }
}

fn apply(s: &mut Snapshot, c: Cmd) {
    match c {
        Cmd::NowPlaying {
            uri,
            progress,
            playing,
            battery,
        } => {
            // ponytail: title from the file name until the library moves into the backend (P2).
            let name = uri.rsplit('/').next().unwrap_or(&uri);
            s.title = name
                .rsplit_once('.')
                .map_or(name, |(stem, _)| stem)
                .to_string();
            s.progress = progress;
            s.playing = playing;
            s.battery = battery;
        }
        #[cfg(test)]
        Cmd::Sleep(ms) => std::thread::sleep(std::time::Duration::from_millis(ms)),
    }
}
