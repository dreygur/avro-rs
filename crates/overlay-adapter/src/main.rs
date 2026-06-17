//! Standalone floating overlay showing live Avro Phonetic preedit/suggestions.
//!
//! Purely a display: `fcitx5-adapter` still does all key interception and
//! text injection via the fcitx5 framework, and publishes state (preedit,
//! suggestions, focused text cursor rect) over a Unix socket that this binary
//! connects to as a client. See `crates/fcitx5-adapter/src/ipc.rs`.
//!
//! Forces XWayland under a native Wayland session (GNOME/Mutter has no
//! layer-shell support, so a client can't otherwise position itself at an
//! absolute screen coordinate) — run with `WAYLAND_DISPLAY` unset so gpui's
//! compositor auto-detection falls back to X11.

use gpui::{
    App, Application, Bounds, Context, Hsla, Render, SharedString, Size, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, div, point,
    prelude::*, px, rgb,
};
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Deserialize)]
struct CursorRect {
    x: i32,
    y: i32,
    #[allow(dead_code)]
    w: i32,
    h: i32,
}

#[derive(Deserialize)]
struct OverlayMsg {
    preedit: String,
    suggestions: Vec<String>,
    cursor: CursorRect,
}

fn socket_path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(dir).join("avro-overlay.sock")
}

// Blocking socket I/O runs on a plain OS thread (not gpui's executor, whose
// pool would otherwise be starved by a permanently-parked read) and forwards
// parsed messages to the gpui-side poller over a channel.
fn spawn_reader() -> mpsc::Receiver<OverlayMsg> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        loop {
            if let Ok(stream) = UnixStream::connect(socket_path()) {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    match serde_json::from_str::<OverlayMsg>(&line) {
                        Ok(msg) => {
                            if tx.send(msg).is_err() {
                                return;
                            }
                        }
                        Err(_) => continue,
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
    rx
}

struct Overlay {
    preedit: SharedString,
    suggestions: Vec<SharedString>,
}

impl Render for Overlay {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .bg(Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.12,
                a: 0.95,
            })
            .size_full()
            .p_1()
            .text_color(rgb(0xffffff))
            .child(div().child(self.preedit.clone()))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .text_sm()
                    .text_color(rgb(0xaaaaaa))
                    .children(
                        self.suggestions.iter().enumerate().map(|(i, s)| {
                            div().child(SharedString::from(format!("{}.{}", i + 1, s)))
                        }),
                    ),
            )
    }
}

fn window_options_at(x: i32, y: i32) -> WindowOptions {
    let bounds = Bounds {
        origin: point(px(x as f32), px(y as f32)),
        size: Size {
            width: px(320.),
            height: px(60.),
        },
    };
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        window_background: WindowBackgroundAppearance::Transparent,
        focus: false,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        ..Default::default()
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let rx = spawn_reader();
        let mut current: Option<WindowHandle<Overlay>> = None;

        cx.spawn(async move |cx| {
            loop {
                while let Ok(msg) = rx.try_recv() {
                    if msg.preedit.is_empty() {
                        if let Some(handle) = current.take() {
                            let _ = handle.update(cx, |_, window, _| window.remove_window());
                        }
                        continue;
                    }

                    let suggestions: Vec<SharedString> =
                        msg.suggestions.iter().cloned().map(Into::into).collect();

                    if let Some(handle) = &current {
                        let updated = handle.update(cx, |view, _, cx| {
                            view.preedit = msg.preedit.clone().into();
                            view.suggestions = suggestions.clone();
                            cx.notify();
                        });
                        if updated.is_ok() {
                            continue;
                        }
                    }

                    // Below the cursor line, since most apps render preedit/composition above
                    // their own caret line already.
                    let opts = window_options_at(msg.cursor.x, msg.cursor.y + msg.cursor.h);
                    let opened = cx.open_window(opts, |_, cx| {
                        cx.new(|_| Overlay {
                            preedit: msg.preedit.clone().into(),
                            suggestions: suggestions.clone(),
                        })
                    });
                    current = opened.ok();
                }

                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
            }
        })
        .detach();
    });
}
