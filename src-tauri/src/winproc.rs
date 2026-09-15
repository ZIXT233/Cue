//! Console-window suppression for helper subprocesses.
//!
//! The release build is a GUI (`windows_subsystem = "windows"`) app without a
//! console. On Windows, spawning a console subprocess (ssh, powershell, the
//! CLI `--version` probes, sqlite3) without `CREATE_NO_WINDOW` allocates a
//! brand-new console whose conhost window flashes on screen until exit.
//! Helper spawns must always opt out; the PTY sessions are ConPTY-attached
//! and never flash.

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
pub trait NoWindow {
    fn no_window(&mut self) -> &mut Self;
}

#[cfg(windows)]
impl NoWindow for tokio::process::Command {
    fn no_window(&mut self) -> &mut Self {
        self.creation_flags(CREATE_NO_WINDOW)
    }
}

#[cfg(windows)]
impl NoWindow for std::process::Command {
    fn no_window(&mut self) -> &mut Self {
        self.creation_flags(CREATE_NO_WINDOW)
    }
}

#[cfg(not(windows))]
pub trait NoWindow {
    fn no_window(&mut self) -> &mut Self;
}

#[cfg(not(windows))]
impl<T> NoWindow for T {
    fn no_window(&mut self) -> &mut Self {
        self
    }
}
