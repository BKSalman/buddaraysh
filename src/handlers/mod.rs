mod compositor;
mod layer_shell;

use crate::focus::FocusTarget;
use crate::{Backend, Buddaraysh};

//
// Wl Seat
//

use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::Resource;
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::selection::data_device::{
    set_data_device_focus, ClientDndGrabHandler, ServerDndGrabHandler,
};

use smithay::{delegate_data_device, delegate_output, delegate_seat};

impl<BackendData: Backend + 'static> SeatHandler for Buddaraysh<BackendData> {
    type KeyboardFocus = FocusTarget;
    type PointerFocus = FocusTarget;

    fn seat_state(&mut self) -> &mut SeatState<Buddaraysh<BackendData>> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus,
    ) {
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&FocusTarget>) {
        let dh = &self.display_handle;

        let wl_surface = focused.and_then(WaylandFocus::wl_surface);

        let client = wl_surface.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client);
    }
}

delegate_seat!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

impl<BackendData: Backend + 'static> ClientDndGrabHandler for Buddaraysh<BackendData> {}
impl<BackendData: Backend + 'static> ServerDndGrabHandler for Buddaraysh<BackendData> {}

delegate_data_device!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

//
// Wl Output & Xdg Output
//

delegate_output!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);
