//! Linux stand-in for the embedded WKWebView: GPUI draws straight to
//! Wayland/X11 with no GTK widget tree to host WebKitGTK in, so pages open in
//! the default browser (`xdg-open`) instead. Same API as the macOS `Browser`,
//! so the app compiles unchanged; Inspect, snapshots and in-page edits are
//! unavailable and say so.

use std::cell::RefCell;
use std::ffi::c_void;

use futures::channel::mpsc::{UnboundedSender, unbounded};

use crate::{BrowserEvent, EditCommand, Events};

/// Shown in the pane instead of a page.
pub const NO_EMBED: &str = "On Linux the page opens in your default browser. Inspect isn't available yet.";

pub struct Browser {
    tx: UnboundedSender<BrowserEvent>,
    history: RefCell<(Vec<String>, usize)>,
}

impl Browser {
    /// `parent` is ignored (no native view is embedded).
    ///
    /// # Safety
    /// Always safe on Linux; `unsafe` only to match the macOS signature.
    pub unsafe fn new(_parent: *mut c_void) -> (Browser, Events) {
        let (tx, rx) = unbounded();
        (Browser { tx, history: RefCell::new((Vec::new(), 0)) }, rx)
    }

    fn send(&self, ev: BrowserEvent) {
        let _ = self.tx.unbounded_send(ev);
    }

    fn open(&self, url: &str) {
        if let Err(e) = std::process::Command::new("xdg-open").arg(url).spawn() {
            self.send(BrowserEvent::Error(format!("Couldn't open {url}: {e}")));
            return;
        }
        self.send(BrowserEvent::Navigated { url: url.to_string(), title: url.to_string() });
        self.send(BrowserEvent::Error(format!("Opened {url} in your browser. {}", NO_EMBED.split(". ").nth(1).unwrap_or(""))));
    }

    pub fn set_frame(&self, _x: f64, _y: f64, _w: f64, _h: f64) {}

    pub fn set_visible(&self, _on: bool) {}

    pub fn load(&self, url: &str) {
        let mut h = self.history.borrow_mut();
        let at = h.1;
        h.0.truncate(at);
        h.0.push(url.to_string());
        h.1 = h.0.len();
        drop(h);
        self.open(url);
    }

    pub fn reload(&self) {
        if let Some(u) = self.url() {
            self.open(&u);
        }
    }

    pub fn go_back(&self) {
        let mut h = self.history.borrow_mut();
        if h.1 > 1 {
            h.1 -= 1;
        }
    }

    pub fn go_forward(&self) {
        let mut h = self.history.borrow_mut();
        if h.1 < h.0.len() {
            h.1 += 1;
        }
    }

    pub fn stop(&self) {}

    pub fn url(&self) -> Option<String> {
        let h = self.history.borrow();
        h.1.checked_sub(1).and_then(|i| h.0.get(i).cloned())
    }

    pub fn title(&self) -> Option<String> {
        self.url()
    }

    pub fn can_go_back(&self) -> bool {
        self.history.borrow().1 > 1
    }

    pub fn can_go_forward(&self) -> bool {
        let h = self.history.borrow();
        h.1 < h.0.len()
    }

    pub fn set_inspect(&self, on: bool, _accent_css: &str) {
        if on {
            self.send(BrowserEvent::Error("Inspect needs the embedded browser, which isn't available on Linux yet.".into()));
            self.send(BrowserEvent::InspectEnded);
        }
    }

    pub fn evaluate_js(&self, _js: &str) {}

    pub fn snapshot(&self, _rect: Option<[f64; 4]>, done: impl FnOnce(Option<Vec<u8>>) + 'static) {
        done(None);
    }

    pub fn release_focus(&self) {}

    pub fn edit(&self, _cmd: EditCommand) {}

    pub fn zoom(&self, _delta: i32) {}

    pub fn focus(&self) {}
}
