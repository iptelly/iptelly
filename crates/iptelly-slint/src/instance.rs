// Single instance, like tauri-plugin-single-instance in the Tauri app: a
// second launch asks the running app to show its window, then exits.
//
// The running app listens on a loopback port and writes the port to a file
// in the cache folder. The exchange is a fixed greeting and reply, so a
// stale port file pointing at some other program is never mistaken for
// IPTelly.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use iptelly_core::log;
use slint::ComponentHandle;
use slint::winit_030::WinitWindowAccessor;

const GREETING: &str = "iptelly-slint show";
const REPLY: &str = "iptelly-slint ok";
const TIMEOUT: Duration = Duration::from_millis(500);

fn port_file() -> Option<PathBuf> {
    Some(
        directories::ProjectDirs::from("dev", "iptelly", "iptelly")?
            .cache_dir()
            .join("slint-instance-port"),
    )
}

/// Returns true if another instance is running (and was asked to show its
/// window), in which case this one should exit.
pub fn already_running() -> bool {
    let Some(port) = port_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| text.trim().parse::<u16>().ok())
    else {
        return false;
    };
    ask_to_show(port).unwrap_or(false)
}

fn ask_to_show(port: u16) -> std::io::Result<bool> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, TIMEOUT)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    writeln!(stream, "{GREETING}")?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply)?;
    Ok(reply.trim() == REPLY)
}

/// Listens for later launches. Failing here only means a second launch
/// opens a second window, so errors are just logged.
pub fn listen() {
    if let Err(e) = try_listen() {
        log::log(format!("Single instance disabled: {e:?}"));
    }
}

fn try_listen() -> anyhow::Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    let path = port_file().ok_or_else(|| anyhow::anyhow!("No cache folder"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, port.to_string())?;
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if answer(stream).unwrap_or(false) {
                let _ = slint::invoke_from_event_loop(show_window);
            }
        }
    });
    Ok(())
}

fn answer(stream: TcpStream) -> std::io::Result<bool> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    let mut greeting = String::new();
    BufReader::new(&stream).read_line(&mut greeting)?;
    if greeting.trim() != GREETING {
        return Ok(false);
    }
    writeln!(&stream, "{REPLY}")?;
    Ok(true)
}

/// Brings the window back, including from the tray.
pub fn show_window() {
    let Some(window) = crate::window() else {
        return;
    };
    let _ = window.show();
    window.window().set_minimized(false);
    // Desktops may only flash the taskbar entry rather than switch to it
    // (Wayland requires an activation token from the launcher).
    window.window().with_winit_window(|w| w.focus_window());
}
