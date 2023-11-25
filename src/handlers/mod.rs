mod compositor;
mod layer_shell;
mod xdg_shell;

use crate::{Backend, Buddaraysh};

//
// Wl Seat
//

use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::wayland::selection::data_device::{
    set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState,
    ServerDndGrabHandler,
};
use smithay::wayland::selection::SelectionHandler;
use smithay::{delegate_data_device, delegate_output, delegate_seat};

impl<BackendData: Backend + 'static> SeatHandler for Buddaraysh<BackendData> {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Buddaraysh<BackendData>> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus,
    ) {
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client);
    }
}

delegate_seat!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

//
// Wl Data Device
//

impl<BackendData: Backend + 'static> SelectionHandler for Buddaraysh<BackendData> {
    type SelectionUserData = ();
}

impl<BackendData: Backend + 'static> DataDeviceHandler for Buddaraysh<BackendData> {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl<BackendData: Backend + 'static> ClientDndGrabHandler for Buddaraysh<BackendData> {}
impl<BackendData: Backend + 'static> ServerDndGrabHandler for Buddaraysh<BackendData> {}

delegate_data_device!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

//
// Wl Output & Xdg Output
//

delegate_output!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);
