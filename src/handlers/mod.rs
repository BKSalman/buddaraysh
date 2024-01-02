mod compositor;
mod keyboard_shortcuts_inhibit;
mod layer_shell;
mod pointer_contraints;

use crate::focus::FocusTarget;
use crate::{Backend, Buddaraysh};

use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_data_source::WlDataSource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::selection::data_device::{
    set_data_device_focus, ClientDndGrabHandler, ServerDndGrabHandler,
};

use smithay::wayland::selection::primary_selection::set_primary_focus;
use smithay::{delegate_data_device, delegate_output, delegate_seat, delegate_tablet_manager};

impl<BackendData: Backend + 'static> SeatHandler for Buddaraysh<BackendData> {
    type KeyboardFocus = FocusTarget;
    type PointerFocus = FocusTarget;

    fn seat_state(&mut self) -> &mut SeatState<Buddaraysh<BackendData>> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        image: smithay::input::pointer::CursorImageStatus,
    ) {
        *self.cursor_status.lock().unwrap() = image;
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&FocusTarget>) {
        let dh = &self.display_handle;

        let wl_surface = focused.and_then(WaylandFocus::wl_surface);

        let client = wl_surface.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client.clone());
        set_primary_focus(dh, seat, client);
    }
}

delegate_seat!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

delegate_tablet_manager!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

impl<BackendData: Backend + 'static> ClientDndGrabHandler for Buddaraysh<BackendData> {
    fn started(
        &mut self,
        _source: Option<WlDataSource>,
        icon: Option<WlSurface>,
        _seat: Seat<Self>,
    ) {
        self.dnd_icon = icon;
    }

    fn dropped(&mut self, _seat: Seat<Self>) {
        self.dnd_icon = None;
    }
}
impl<BackendData: Backend + 'static> ServerDndGrabHandler for Buddaraysh<BackendData> {}

delegate_data_device!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

//
// Wl Output & Xdg Output
//

delegate_output!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);
