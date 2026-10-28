//! Linux web view: GPUI draws straight to Wayland/X11 with no GTK widget tree to
//! host WebKitGTK in, so the page lives in a helper process (`webview.py`,
//! WebKitGTK 6 through PyGObject) with the same API as the macOS `Browser`:
//! inspect, picks, snapshots and edit commands all work. On Hyprland its window
//! floats exactly over the Browser pane; elsewhere it opens beside Qur. Without
//! WebKitGTK, pages open in the default browser instead.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::channel::mpsc::{UnboundedSender, unbounded};
use serde_json::{Value, json};

use crate::event::parse_message;
use crate::{BrowserEvent, EditCommand, Events, normalize_url};

const HELPER: &str = include_str!("webview.py");
const SCRIPT: &str = include_str!("inspect.js");

/// Shown in the pane when WebKitGTK isn't installed.
const NO_EMBED: &str = "Install webkitgtk-6.0 and python-gobject for the built-in browser; until then pages open in your default browser.";

#[derive(Default)]
struct State {
    ready: bool,
    url: Option<String>,
    title: Option<String>,
    back: bool,
    fwd: bool,
    snaps: HashMap<u64, Sender<Option<Vec<u8>>>>,
}

pub struct Browser {
    tx: UnboundedSender<BrowserEvent>,
    child: RefCell<Option<(Child, ChildStdin)>>,
    state: Arc<Mutex<State>>,
    frame: Cell<[i64; 4]>,
    visible: Cell<bool>,
    next_snap: Cell<u64>,
}

/// Write the helper and the page script where the helper can read them.
fn helper_files() -> std::io::Result<(std::path::PathBuf, std::path::PathBuf)> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir).join("qur");
    std::fs::create_dir_all(&dir)?;
    let (py, js) = (dir.join(format!("webview-{}.py", std::process::id())), dir.join(format!("inspect-{}.js", std::process::id())));
    std::fs::write(&py, HELPER)?;
    std::fs::write(&js, SCRIPT)?;
    Ok((py, js))
}

fn spawn(tx: UnboundedSender<BrowserEvent>, state: Arc<Mutex<State>>) -> std::io::Result<(Child, ChildStdin)> {
    let (py, js) = helper_files()?;
    // -I: don't import from the current directory or user site.
    let mut child = Command::new("python3")
        .arg("-I")
        .arg(&py)
        .env("QUR_WEB_SCRIPT", &js)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdin = child.stdin.take().expect("piped");
    let stdout = child.stdout.take().expect("piped");
    std::thread::Builder::new().name("qur-webview".into()).spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                on_line(&v, &tx, &state);
            }
        }
        let mut st = state.lock().unwrap();
        st.snaps.drain().for_each(|(_, s)| drop(s.send(None)));
        if !st.ready {
            // The helper never came up (no python3/WebKitGTK): fall back to the default browser.
            let url = st.url.clone();
            drop(st);
            let _ = tx.unbounded_send(BrowserEvent::Error(NO_EMBED.into()));
            if let Some(u) = url {
                let _ = Command::new("xdg-open").arg(u).spawn();
            }
        }
    })?;
    Ok((child, stdin))
}

fn on_line(v: &Value, tx: &UnboundedSender<BrowserEvent>, state: &Mutex<State>) {
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let ev = match v.get("e").and_then(Value::as_str) {
        Some("ready") => {
            state.lock().unwrap().ready = true;
            return;
        }
        Some("msg") => parse_message(&s("body")),
        Some("nav") => {
            let mut st = state.lock().unwrap();
            st.url = Some(s("url"));
            st.title = Some(s("title"));
            st.back = v["back"].as_bool().unwrap_or(false);
            st.fwd = v["fwd"].as_bool().unwrap_or(false);
            Some(BrowserEvent::Navigated { url: s("url"), title: s("title") })
        }
        Some("loading") => Some(BrowserEvent::Loading(v["on"].as_bool().unwrap_or(false))),
        Some("error") => Some(BrowserEvent::Error(s("msg"))),
        Some("snap") => {
            let id = v["id"].as_u64().unwrap_or(0);
            if let Some(done) = state.lock().unwrap().snaps.remove(&id) {
                let _ = done.send(v["png"].as_str().and_then(unhex));
            }
            None
        }
        _ => None,
    };
    if let Some(ev) = ev {
        let _ = tx.unbounded_send(ev);
    }
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

impl Browser {
    /// `parent` is ignored (the page lives in its own window).
    ///
    /// # Safety
    /// Always safe on Linux; `unsafe` only to match the macOS signature.
    pub unsafe fn new(_parent: *mut c_void) -> (Browser, Events) {
        let (tx, rx) = unbounded();
        let state = Arc::new(Mutex::new(State::default()));
        let child = match spawn(tx.clone(), state.clone()) {
            Ok(c) => Some(c),
            Err(e) => {
                let _ = tx.unbounded_send(BrowserEvent::Error(format!("{NO_EMBED} ({e})")));
                None
            }
        };
        let b = Browser { tx, child: RefCell::new(child), state, frame: Cell::new([0; 4]), visible: Cell::new(false), next_snap: Cell::new(1) };
        (b, rx)
    }

    fn send(&self, cmd: Value) {
        let mut child = self.child.borrow_mut();
        if let Some((_, stdin)) = child.as_mut()
            && writeln!(stdin, "{cmd}").and_then(|_| stdin.flush()).is_err()
        {
            *child = None;
        }
    }

    pub fn set_frame(&self, x: f64, y: f64, w: f64, h: f64) {
        // Called every paint; only tell the helper about changes.
        let r = [x, y, w, h].map(|v| v.round() as i64);
        if self.frame.replace(r) != r {
            self.send(json!({"c": "frame", "r": r}));
        }
    }

    pub fn set_visible(&self, on: bool) {
        if self.visible.replace(on) != on {
            self.send(json!({"c": "visible", "on": on}));
        }
    }

    /// Load what the user typed; see [`normalize_url`] for the rules.
    pub fn load(&self, url: &str) {
        let url = normalize_url(url);
        self.state.lock().unwrap().url = Some(url.clone());
        if self.child.borrow().is_none() {
            if let Err(e) = Command::new("xdg-open").arg(&url).spawn() {
                let _ = self.tx.unbounded_send(BrowserEvent::Error(format!("Couldn't open {url}: {e}")));
            }
            return;
        }
        self.send(json!({"c": "load", "url": url}));
    }

    pub fn reload(&self) {
        self.send(json!({"c": "reload"}));
    }

    pub fn go_back(&self) {
        self.send(json!({"c": "back"}));
    }

    pub fn go_forward(&self) {
        self.send(json!({"c": "forward"}));
    }

    pub fn stop(&self) {
        self.send(json!({"c": "stop"}));
    }

    pub fn url(&self) -> Option<String> {
        self.state.lock().unwrap().url.clone()
    }

    pub fn title(&self) -> Option<String> {
        self.state.lock().unwrap().title.clone()
    }

    pub fn can_go_back(&self) -> bool {
        self.state.lock().unwrap().back
    }

    pub fn can_go_forward(&self) -> bool {
        self.state.lock().unwrap().fwd
    }

    /// Inspect mode, as on macOS: hover outlines, click picks, Esc ends it.
    pub fn set_inspect(&self, on: bool, accent_css: &str) {
        self.send(json!({"c": "inspect", "on": on, "accent": accent_css}));
    }

    pub fn evaluate_js(&self, js: &str) {
        self.send(json!({"c": "eval", "js": js}));
    }

    /// PNG of the visible page, or of `rect` (CSS px, viewport-relative).
    /// shortcut: waits up to 3 s on the main thread for the helper's reply (callers' `done` isn't Send); make it async if snapshots get frequent.
    pub fn snapshot(&self, rect: Option<[f64; 4]>, done: impl FnOnce(Option<Vec<u8>>) + 'static) {
        let id = self.next_snap.replace(self.next_snap.get() + 1);
        let (stx, srx) = channel();
        self.state.lock().unwrap().snaps.insert(id, stx);
        self.send(json!({"c": "snapshot", "id": id, "rect": rect}));
        let png = srx.recv_timeout(Duration::from_secs(3)).ok().flatten();
        self.state.lock().unwrap().snaps.remove(&id);
        done(png);
    }

    /// The page has its own window, so the compositor moves the keyboard on click.
    pub fn release_focus(&self) {}

    pub fn edit(&self, cmd: EditCommand) {
        self.send(json!({"c": "edit", "cmd": format!("{cmd:?}")}));
    }

    /// Page zoom: `delta` steps of 10 % (0 resets to 100 %).
    pub fn zoom(&self, delta: i32) {
        self.send(json!({"c": "zoom", "delta": delta}));
    }

    pub fn focus(&self) {
        self.send(json!({"c": "focus"}));
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        // Closing stdin ends the helper; reap it off the main thread.
        if let Some((mut child, stdin)) = self.child.take() {
            drop(stdin);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(2));
                let _ = child.kill();
                let _ = child.wait();
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unhex_round_trips() {
        assert_eq!(unhex("00ff10"), Some(vec![0, 255, 16]));
        assert_eq!(unhex("0"), None);
        assert_eq!(unhex("zz"), None);
    }

    #[test]
    fn lines_become_events() {
        let (tx, mut rx) = unbounded();
        let st = Mutex::new(State::default());
        on_line(&json!({"e": "nav", "url": "u", "title": "T", "back": true, "fwd": false}), &tx, &st);
        on_line(&json!({"e": "msg", "body": "{\"t\":\"esc\"}"}), &tx, &st);
        assert!(st.lock().unwrap().back);
        assert!(matches!(rx.try_recv(), Ok(BrowserEvent::Navigated { url, .. }) if url == "u"));
        assert!(matches!(rx.try_recv(), Ok(BrowserEvent::InspectEnded)));
    }
}
