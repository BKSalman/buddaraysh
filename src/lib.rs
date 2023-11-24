mod handlers;

mod commands;
mod cursor;
mod drawing;
mod grabs;
mod input;
mod render;
mod state;
pub mod udev;
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
