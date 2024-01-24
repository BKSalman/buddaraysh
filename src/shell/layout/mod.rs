mod floating;
mod tiling;

pub use floating::FloatingLayout;
use regex::RegexSet;
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;
use smithay::xwayland::xwm::WmWindowType;
pub use tiling::Layout;
pub use tiling::TilingLayout;

use crate::window::WindowElement;

use super::FullscreenSurface;

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedState {
    pub layer: ManagedLayer,
    pub was_fullscreen: Option<FullscreenSurface>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ManagedLayer {
    Tiling,
    Floating,
}

lazy_static::lazy_static! {
    static ref EXCEPTIONS_APPID: RegexSet = RegexSet::new([
        r"Authy Desktop",
        r"Com.github.amezin.ddterm",
        r"Com.github.donadigo.eddy",
        r".*",
        r"Enpass",
        r"Gjs",
        r"Gnome-initial-setup",
        r"Gnome-terminal",
        r"Guake",
        r"Io.elementary.sideload",
        r"KotatogramDesktop",
        r"Mozilla VPN",
        r"update-manager",
        r"Solaar",
        r"Steam",
        r"",
        r"TelegramDesktop",
        r"Zotero",
        r"gjs",
        r"gnome-screenshot",
        r"ibus-.*",
        r"jetbrains-toolbox",
        r"jetbrains-webstorm",
        r"jetbrains-webstorm",
        r"jetbrains-webstorm",
        r"krunner",
        r"pritunl",
        r"re.sonny.Junction",
        r"system76-driver",
        r"tilda",
        r"zoom",
        r"^.*?action=join.*$",
    ]).unwrap();
    static ref EXCEPTIONS_TITLE: RegexSet = RegexSet::new([
        r".*",
        r".*",
        r".*",
        r"Discord Updater",
        r"Enpass Assistant",
        r"Settings",
        r".*",
        r"Preferences – General",
        r".*",
        r".*",
        r"Media viewer",
        r".*",
        r"Software Updater",
        r".*",
        r"^.*?(Guard|Login).*",
        r"Steam",
        r"Media viewer",
        r"Quick Format Citation",
        r".*",
        r".*",
        r".*",
        r".*",
        r"Customize WebStorm",
        r"License Activation",
        r"Welcome to WebStorm",
        r".*",
        r".*",
        r".*",
        r".*",
        r".*",
        r".*",
        r".*",
    ]).unwrap();
}

pub fn should_be_floating(window: &WindowElement) -> bool {
    // Check "window type"
    match window {
        WindowElement::Wayland(window) => {
            if with_states(window.toplevel().wl_surface(), |states| {
                let attrs = states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .unwrap()
                    .lock()
                    .unwrap();
                attrs.parent.is_some()
            }) {
                return true;
            }
        }
        WindowElement::X11(surface) => {
            if surface.is_override_redirect()
                || surface.is_popup()
                || !matches!(
                    surface.window_type(),
                    None | Some(WmWindowType::Normal) | Some(WmWindowType::Utility)
                )
            {
                return true;
            }
        }
    }

    // Check if sizing suggest dialog
    let max_size = window.max_size();
    let min_size = window.min_size();

    if min_size.is_some() && min_size == max_size {
        return true;
    }

    // else take a look at our exceptions
    let appid_matches = EXCEPTIONS_APPID.matches(&window.app_id());
    let title_matches = EXCEPTIONS_TITLE.matches(&window.title());
    for idx in appid_matches.into_iter() {
        if title_matches.matched(idx) {
            return true;
        }
    }

    false
}
