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
pub mod utils;
mod window;
pub mod winit;
mod workspace;

use smithay::{
    output::Output,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, DisplayHandle},
    utils::Rectangle,
};
pub use state::Buddaraysh;
use utils::geometry::Global;
use utils::geometry::RectExt;

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
    ToggleFloating,
    Spawn(String),
    Quit,
    SwitchToWorkspace(usize),
    MoveToWorkspace(usize),
    None,
    Close,
}

pub trait OutputExt {
    fn geometry(&self) -> Rectangle<i32, Global>;
}

impl OutputExt for Output {
    fn geometry(&self) -> Rectangle<i32, Global> {
        Rectangle::from_loc_and_size(self.current_location(), {
            self.current_transform()
                .transform_size(
                    self.current_mode()
                        .map(|m| m.size)
                        .unwrap_or_else(|| (0, 0).into()),
                )
                .to_f64()
                .to_logical(self.current_scale().fractional_scale())
                .to_i32_round()
        })
        .as_global()
    }
}
