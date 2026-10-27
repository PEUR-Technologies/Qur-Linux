//! Embedded system WKWebView for Qur's Browser view, with an element-inspect mode.
//!
//! macOS embeds WKWebView. On Linux, `Browser` has the same API but opens pages in the
//! default browser (see `linux.rs`); elsewhere only the event types and helpers exist.
//!
//! Everything here is main-thread only (AppKit). Events leave through an unbounded
//! `futures` channel; there is no async runtime.

mod event;
mod url;

pub use event::{BrowserEvent, EditCommand, Picked};
pub use url::normalize_url;

/// Receiving end of a [`Browser`]'s event stream.
pub type Events = futures::channel::mpsc::UnboundedReceiver<BrowserEvent>;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
pub use mac::Browser;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{Browser, NO_EMBED};
