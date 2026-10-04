// The one modal dialog (ui/dialog.slint): messages, yes/no confirmations and
// PIN entry.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use crate::{AppWindow, DialogKind, DialogState};

type Handler = Rc<dyn Fn(&AppWindow, String)>;

thread_local! {
    static HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
    // Run when the dialog is cancelled (only set by `choose`).
    static CANCEL_HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
}

pub fn setup(window: &AppWindow) {
    let state = window.global::<DialogState>();

    let weak = window.as_weak();
    state.on_confirmed(move || {
        let Some(window) = weak.upgrade() else { return };
        let state = window.global::<DialogState>();
        let Some(handler) = HANDLER.with_borrow(|h| h.clone()) else {
            return;
        };
        if state.get_kind() == DialogKind::Pin {
            // The handler checks the PIN and closes the dialog itself if
            // it's right (see pin_rejected).
            handler(&window, state.get_pin().into());
        } else if state.get_kind() == DialogKind::Error {
            // Copy, and leave the dialog open.
            handler(&window, state.get_details().into());
        } else {
            close(&window);
            handler(&window, String::new());
        }
    });

    let weak = window.as_weak();
    state.on_cancelled(move || {
        if let Some(window) = weak.upgrade() {
            let cancel = CANCEL_HANDLER.with_borrow(|h| h.clone());
            close(&window);
            if let Some(cancel) = cancel {
                cancel(&window, String::new());
            }
        }
    });
}

pub fn confirm(
    window: &AppWindow,
    title: &str,
    message: &str,
    confirm_label: &str,
    on_confirm: impl Fn(&AppWindow) + 'static,
) {
    open(window, DialogKind::Confirm, title, message, confirm_label);
    HANDLER.set(Some(Rc::new(move |window, _| on_confirm(window))));
}

/// A message with just an OK button.
pub fn inform(window: &AppWindow, title: &str, message: &str) {
    open(window, DialogKind::Confirm, title, message, "OK");
    window.global::<DialogState>().set_cancel_label("".into());
    HANDLER.set(Some(Rc::new(|_, _| {})));
}

/// Two choices, both of which carry on (Escape counts as the second).
pub fn choose(
    window: &AppWindow,
    title: &str,
    message: &str,
    first_label: &str,
    second_label: &str,
    on_first: impl Fn(&AppWindow) + 'static,
    on_second: impl Fn(&AppWindow) + 'static,
) {
    open(window, DialogKind::Confirm, title, message, first_label);
    window
        .global::<DialogState>()
        .set_cancel_label(second_label.into());
    HANDLER.set(Some(Rc::new(move |window, _| on_first(window))));
    CANCEL_HANDLER.set(Some(Rc::new(move |window, _| on_second(window))));
}

#[cfg(target_os = "linux")]
const LOGS: &str =
    "~/.cache/iptelly/logs (or ~/.var/app/dev.iptelly.iptelly/cache/iptelly/logs for the Flatpak)";
#[cfg(target_os = "windows")]
const LOGS: &str = "%localappdata%\\iptelly\\iptelly\\cache\\logs";
#[cfg(target_os = "macos")]
const LOGS: &str = "~/Library/Caches/dev.iptelly.iptelly/logs";
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
const LOGS: &str = "the IPTelly cache folder";

/// The full text of an error, which can be copied for a bug report.
pub fn show_error(window: &AppWindow, details: &str) {
    open(
        window,
        DialogKind::Error,
        "Error",
        &format!(
            "Please include this error, and the logs in {LOGS}, when reporting a problem at \
             https://github.com/iptelly/iptelly/issues"
        ),
        "Copy",
    );
    let state = window.global::<DialogState>();
    state.set_details(details.into());
    state.set_cancel_label("Close".into());
    HANDLER.set(Some(Rc::new(
        |window, details| match crate::copy_to_clipboard(details) {
            Ok(()) => crate::show_toast(window, "Copied error"),
            Err(e) => crate::show_toast(window, &format!("Couldn't copy: {e}")),
        },
    )));
}

/// Asks for the adult-content PIN. `on_pin` gets what was typed; it calls
/// `close` once the PIN is accepted or `pin_rejected` to ask again.
pub fn ask_pin(
    window: &AppWindow,
    title: &str,
    message: &str,
    on_pin: impl Fn(&AppWindow, String) + 'static,
) {
    open(window, DialogKind::Pin, title, message, "Unlock");
    HANDLER.set(Some(Rc::new(on_pin)));
}

pub fn pin_rejected(window: &AppWindow) {
    let state = window.global::<DialogState>();
    state.set_pin("".into());
    state.set_pin_error("Incorrect PIN.".into());
}

pub fn close(window: &AppWindow) {
    window.global::<DialogState>().set_kind(DialogKind::None);
    HANDLER.set(None);
    CANCEL_HANDLER.set(None);
}

fn open(window: &AppWindow, kind: DialogKind, title: &str, message: &str, confirm_label: &str) {
    let state = window.global::<DialogState>();
    state.set_title(title.into());
    state.set_message(message.into());
    state.set_confirm_label(confirm_label.into());
    state.set_cancel_label("Cancel".into());
    CANCEL_HANDLER.set(None);
    state.set_pin("".into());
    state.set_pin_error("".into());
    state.set_kind(kind);
}
