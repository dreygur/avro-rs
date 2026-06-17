//! Unix-domain-socket broadcast of preedit/suggestion/cursor state for the
//! standalone overlay app (`crates/overlay-adapter`). Fcitx5 still owns key
//! interception and text injection; this only publishes state for display.

use serde::Serialize;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

#[derive(Serialize)]
pub struct CursorRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Serialize)]
struct OverlayState<'a> {
    preedit: &'a str,
    suggestions: &'a [String],
    cursor: CursorRect,
}

type Clients = Arc<Mutex<Vec<UnixStream>>>;

fn socket_path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(dir).join("avro-overlay.sock")
}

fn clients() -> &'static Clients {
    static CLIENTS: OnceLock<Clients> = OnceLock::new();
    CLIENTS.get_or_init(|| {
        let clients: Clients = Arc::new(Mutex::new(Vec::new()));
        let path = socket_path();
        let _ = std::fs::remove_file(&path);
        if let Ok(listener) = UnixListener::bind(&path) {
            let accepted = Arc::clone(&clients);
            thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    accepted.lock().unwrap().push(stream);
                }
            });
        }
        clients
    })
}

pub fn publish(preedit: &str, suggestions: &[String], cursor: CursorRect) {
    let state = OverlayState {
        preedit,
        suggestions,
        cursor,
    };
    let Ok(mut line) = serde_json::to_string(&state) else {
        return;
    };
    line.push('\n');

    let mut guard = clients().lock().unwrap();
    guard.retain_mut(|client| client.write_all(line.as_bytes()).is_ok());
}

// ── Overlay process lifecycle ───────────────────────────────────────────────
//
// The overlay-adapter binary is a separate process (a gpui window can't live
// inside the fcitx5 daemon). One singleton instance is spawned for the whole
// addon, independent of any per-InputContext `AvroState` — started from
// `AvroPhoneticEngine`'s constructor and stopped from its destructor.

fn overlay_child() -> &'static Mutex<Option<Child>> {
    static CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();
    CHILD.get_or_init(|| Mutex::new(None))
}

/// Spawns the overlay-adapter binary if it isn't already running. Idempotent.
/// Logs and returns without effect if the binary is missing or won't start —
/// the IME itself must keep working even if the overlay can't.
pub fn spawn_overlay() {
    let mut guard = overlay_child().lock().unwrap();
    if let Some(child) = guard.as_mut() {
        match child.try_wait() {
            Ok(None) => return, // still running
            _ => *guard = None, // exited or unknown — fall through to respawn
        }
    }

    // WAYLAND_DISPLAY must be unset so gpui falls back to XWayland, which is
    // required for absolute window positioning under GNOME/Mutter (no
    // layer-shell support there).
    match Command::new(env!("OVERLAY_BIN")).env_remove("WAYLAND_DISPLAY").spawn() {
        Ok(child) => *guard = Some(child),
        Err(err) => eprintln!("avro: failed to spawn overlay-adapter: {err}"),
    }
}

/// Terminates the overlay-adapter process spawned by `spawn_overlay`, if any.
pub fn stop_overlay() {
    if let Some(mut child) = overlay_child().lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}
