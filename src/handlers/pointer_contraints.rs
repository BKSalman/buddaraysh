use crate::{state::Buddaraysh, Backend};
use smithay::{
    delegate_pointer_constraints,
    input::pointer::PointerHandle,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    wayland::{
        pointer_constraints::{with_pointer_constraint, PointerConstraintsHandler},
        seat::WaylandFocus,
    },
};

impl<BackendData: Backend> PointerConstraintsHandler for Buddaraysh<BackendData> {
    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        // XXX region
        if pointer
            .current_focus()
            .and_then(|x| x.wl_surface())
            .as_ref()
            == Some(surface)
        {
            with_pointer_constraint(surface, pointer, |constraint| {
                constraint.unwrap().activate();
            });
        }
    }
}
delegate_pointer_constraints!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);
