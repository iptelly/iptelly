// The Re-stream dialog. Ported from src/app/restream-modal/.

use std::cell::{Cell, RefCell};

use iptelly_core::types::Channel;
use iptelly_core::{restream, settings};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::{AppWindow, RestreamState, STATE};

thread_local! {
    static CHANNEL: RefCell<Option<Channel>> = const { RefCell::new(None) };
    static PORT: Cell<u16> = const { Cell::new(3000) };
}

pub fn setup(window: &AppWindow) {
    let state = window.global::<RestreamState>();

    let weak = window.as_weak();
    state.on_close(move || {
        if let Some(window) = weak.upgrade() {
            window.global::<RestreamState>().set_open(false);
        }
    });

    let weak = window.as_weak();
    state.on_start(move || {
        let Some(window) = weak.upgrade() else { return };
        let Some(channel) = CHANNEL.with_borrow(|c| c.clone()) else {
            return;
        };
        window.global::<RestreamState>().set_loading(true);
        let port = PORT.get();
        // Runs until the restream stops; the core reports when it has
        // started (see events.rs).
        crate::spawn(
            restream::start_restream(port, &STATE, crate::events(), channel),
            |window, result| {
                let state = window.global::<RestreamState>();
                state.set_started(false);
                state.set_loading(false);
                if let Err(e) = result {
                    crate::show_error(window, &e.context("Re-streaming failed"));
                }
            },
        );
    });

    state.on_stop(|| {
        crate::spawn(restream::stop_restream(&STATE), |window, result| {
            if let Err(e) = result {
                crate::show_error(window, &e);
            }
        });
    });

    let weak = window.as_weak();
    state.on_watch(move || {
        let Some(window) = weak.upgrade() else { return };
        window.global::<RestreamState>().set_watching(true);
        let port = PORT.get();
        crate::spawn(restream::watch_self(port, &STATE), |window, result| {
            window.global::<RestreamState>().set_watching(false);
            if let Err(e) = result {
                crate::show_error(window, &e);
            }
        });
    });

    let weak = window.as_weak();
    state.on_share(move || {
        let Some(window) = weak.upgrade() else { return };
        let Some(channel) = CHANNEL.with_borrow(|c| c.clone()) else {
            return;
        };
        let state = window.global::<RestreamState>();
        // Each entry is already the full stream URL.
        let Some(address) = state
            .get_share_ips()
            .row_data(state.get_share_ip() as usize)
            .map(|a| a.to_string())
        else {
            return;
        };
        let file_name = format!(
            "{}_rst.otv",
            iptelly_core::utils::sanitize(channel.name.clone())
        );
        crate::spawn(
            async move {
                let Some(path) = rfd::AsyncFileDialog::new()
                    .set_title("Select where to export re-stream")
                    .set_file_name(file_name)
                    .save_file()
                    .await
                else {
                    return Ok(false);
                };
                let path = path.path().to_string_lossy().into_owned();
                crate::blocking(move || restream::share_restream(address, channel, path)).await?;
                Ok(true)
            },
            |window, result| match result {
                Ok(true) => crate::show_toast(window, "Re-stream exported"),
                Ok(false) => {}
                Err(e) => crate::show_error(window, &e.context("Failed to export the re-stream")),
            },
        );
    });
}

pub fn open(window: &AppWindow, channel: Channel) {
    let state = window.global::<RestreamState>();
    state.set_channel_name(channel.name.as_str().into());
    state.set_local_ips(ModelRc::default());
    state.set_wan_ip("".into());
    state.set_share_ips(ModelRc::default());
    state.set_open(true);
    CHANNEL.set(Some(channel));
    crate::spawn(restream::get_network_info(), |window, result| {
        let state = window.global::<RestreamState>();
        match result {
            Ok(info) => {
                PORT.set(info.port);
                let local: Vec<SharedString> =
                    info.local_ips.iter().map(|ip| ip.as_str().into()).collect();
                let mut share = local.clone();
                share.push(info.wan_ip.as_str().into());
                state.set_local_ips(ModelRc::new(VecModel::from(local)));
                state.set_wan_ip(info.wan_ip.into());
                state.set_share_ips(ModelRc::new(VecModel::from(share)));
                state.set_share_ip(0);
            }
            // Re-streaming still works without the address list; only
            // sharing needs it.
            Err(e) => {
                if let Some(port) = settings::get_settings().ok().and_then(|s| s.restream_port) {
                    PORT.set(port);
                }
                crate::show_error(
                    window,
                    &e.context("Couldn't look up this computer's addresses"),
                );
            }
        }
    });
}
