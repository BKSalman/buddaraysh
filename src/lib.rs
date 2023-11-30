mod cursor;
mod drawing;
mod focus;
mod grabs;
mod handlers;
mod input;
mod protocols;
mod render;
mod shell;
mod ssd;
mod state;
mod systemd;
pub mod udev;
mod window;
pub mod winit;
mod workspace;

use smithay::{
    output::Output,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, DisplayHandle},
};
pub use state::Buddaraysh;

// The button is a button code as defined in the
// Linux kernel's linux/input-event-codes.h header file, e.g. BTN_LEFT.
pub const BTN_LEFT: u32 = 0x110;
pub const BTN_RIGHT: u32 = 0x111;

pub struct CalloopData<BackendData: Backend + 'static> {
    state: Buddaraysh<BackendData>,
    display_handle: DisplayHandle,
}

pub trait Backend {
    const HAS_RELATIVE_MOTION: bool = false;
    const HAS_GESTURES: bool = false;
    fn seat_name(&self) -> String;
    fn reset_buffers(&mut self, output: &Output);
    fn early_import(&mut self, surface: &WlSurface);
}

#[derive(Debug, Clone)]
pub enum Action {
    Spawn(String),
    Quit,
    SwitchToWorkspace(usize),
    None,
    Close,
}
