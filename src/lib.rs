mod handlers;

mod cursor;
mod drawing;
mod focus;
mod grabs;
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

use smithay::{
    output::Output,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, DisplayHandle},
};
pub use state::Buddaraysh;

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

pub enum Action<'a> {
    Spawn(&'a str),
    Quit,
    None,
    CycleLayout,
    Close,
}
