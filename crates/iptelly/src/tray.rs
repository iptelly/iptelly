// The tray icon on Windows and macOS. While it's on, closing the window
// hides it to the tray; the tray's menu has Show and Quit, and a left click
// shows the window. Linux has no tray icon: the core always reports it off
// there.

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod imp {
    use std::cell::{Cell, RefCell};
    use std::time::Duration;

    use iptelly_core::log;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{
        Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    };

    const ICON: &[u8] = include_bytes!("../icons/256x256.png");
    const SHOW: &str = "show";
    const QUIT: &str = "quit";

    thread_local! {
        static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
        static ENABLED: Cell<bool> = const { Cell::new(false) };
    }

    pub fn enabled() -> bool {
        ENABLED.get()
    }

    pub fn set(enabled: bool) {
        ENABLED.set(enabled);
        // macOS only allows the icon once the event loop (and with it
        // NSApplication) is running.
        slint::Timer::single_shot(Duration::ZERO, move || {
            if !enabled {
                TRAY.set(None);
            } else if TRAY.with_borrow(Option::is_none) {
                match build() {
                    Ok(tray) => TRAY.set(Some(tray)),
                    Err(e) => log::log(format!("Failed to create the tray icon: {e:?}")),
                }
            }
        });
    }

    fn build() -> anyhow::Result<TrayIcon> {
        let image = image::load_from_memory(ICON)?.into_rgba8();
        let (width, height) = image.dimensions();
        let icon = Icon::from_rgba(image.into_raw(), width, height)?;
        let menu = Menu::with_items(&[
            &MenuItem::with_id(SHOW, "Show", true, None),
            &MenuItem::with_id(QUIT, "Quit", true, None),
        ])?;

        // Both handlers may be called off the UI thread.
        MenuEvent::set_event_handler(Some(|event: MenuEvent| {
            let id = event.id.0;
            let _ = slint::invoke_from_event_loop(move || match id.as_str() {
                SHOW => crate::instance::show_window(),
                QUIT => {
                    let _ = slint::quit_event_loop();
                }
                _ => {}
            });
        }));
        TrayIconEvent::set_event_handler(Some(|event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = slint::invoke_from_event_loop(crate::instance::show_window);
            }
        }));

        Ok(TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("IPTelly")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()?)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    pub fn enabled() -> bool {
        false
    }

    pub fn set(_enabled: bool) {}
}

/// Whether the tray icon is on, so closing the window hides it to the tray.
pub use imp::enabled;
/// Shows or removes the tray icon, from the "Enable tray icon" setting.
pub use imp::set;
