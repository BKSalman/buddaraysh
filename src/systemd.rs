// https://github.com/pop-os/cosmic-comp/blob/c99126d2c786dcc368d667bb1247bc2e75b42545/src/systemd.rs
// SPDX-License-Identifier: GPL-3.0-only

use crate::{state::Buddaraysh, Backend};
use libsystemd::daemon::{booted, notify, NotifyState};
use std::process::Command;
use tracing::{error, warn};

/// import environment variables to systemd
pub fn ready<BackendData: Backend>(state: &Buddaraysh<BackendData>) {
    if booted() {
        match Command::new("systemctl")
            .args(["--user", "import-environment", "WAYLAND_DISPLAY", "DISPLAY"])
            .env("WAYLAND_DISPLAY", &state.socket_name)
            .env(
                "DISPLAY",
                &state
                    .xdisplay
                    .as_ref()
                    .map(|xdisplay| format!(":{}", xdisplay))
                    .unwrap_or_default(),
            )
            .status()
        {
            Ok(x) if x.success() => {}
            Ok(x) => warn!(
                exit_code = ?x.code(),
                "Failed to import WAYLAND_DISPLAY/DISPLAY into systemd",
            ),
            Err(err) => error!(?err, "Failed to run systemctl although booted with systemd",),
        };

        if let Err(err) = notify(false, &[NotifyState::Ready]) {
            error!(?err, "Failed to notify systemd");
        }
    }
}
