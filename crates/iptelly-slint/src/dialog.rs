// The one modal dialog (ui/dialog.slint): yes/no confirmations and PIN entry.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use crate::{AppWindow, DialogKind, DialogState};

type Handler = Rc<dyn Fn(&AppWindow, String)>;

thread_local! {
    static HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
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
        } else {
            close(&window);
            handler(&window, String::new());
        }
    });

    let weak = window.as_weak();
    state.on_cancelled(move || {
        if let Some(window) = weak.upgrade() {
            close(&window);
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
}

fn open(window: &AppWindow, kind: DialogKind, title: &str, message: &str, confirm_label: &str) {
    let state = window.global::<DialogState>();
    state.set_title(title.into());
    state.set_message(message.into());
    state.set_confirm_label(confirm_label.into());
    state.set_pin("".into());
    state.set_pin_error("".into());
    state.set_kind(kind);
}
