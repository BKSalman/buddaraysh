use std::cell::RefCell;

use smithay::{
    delegate_xdg_activation, delegate_xdg_shell,
    desktop::{space::SpaceElement, PopupKind, PopupManager, Space, Window},
    input::{
        keyboard::GrabStartData,
        pointer::{Focus, GrabStartData as PointerGrabStartData},
        Seat,
    },
    output::Output,
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            protocol::{wl_output::WlOutput, wl_seat, wl_surface::WlSurface},
            Resource,
        },
    },
    utils::{Rectangle, Serial},
    wayland::{
        compositor::{self, with_states},
        seat::WaylandFocus,
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgPopupSurfaceData, XdgShellHandler,
            XdgShellState, XdgToplevelSurfaceData,
        },
        xdg_activation::XdgActivationHandler,
    },
};
use tracing::{debug, trace};

use crate::{
    grabs::{resize_grab::ResizeSurfaceState, MoveSurfaceGrab, ResizeSurfaceGrab},
    shell::FullscreenSurface,
    window::WindowElement,
    Backend, Buddaraysh,
};

use super::{fullscreen_output_geometry, place_new_window};

impl<BackendData: Backend + 'static> XdgShellHandler for Buddaraysh<BackendData> {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let window = WindowElement::Wayland(Window::new(surface));
        place_new_window(
            self.workspaces.current_workspace_mut().space_mut(),
            self.pointer.current_location(),
            &window,
            true,
        );
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        let _ = self.popups.track_popup(PopupKind::from(surface));
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            // NOTE: This is again a simplification, a proper compositor would
            // calculate the geometry of the popup here. For simplicity we just
            // use the default implementation here that does not take the
            // window position and output constraints into account.
            let geometry = positioner.get_geometry();
            state.geometry = geometry;
            state.positioner = positioner;
        });
        surface.send_repositioned(token);
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let seat: Seat<Buddaraysh<BackendData>> = Seat::from_resource(&seat).unwrap();
        if let Some(start_data) = check_grab(&seat, surface.wl_surface(), serial) {
            self.move_request_xdg(&surface, &seat, serial, start_data);
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let seat = Seat::from_resource(&seat).unwrap();
        let wl_surface = surface.wl_surface();

        if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
            self.resize_request_xdg(surface, seat, serial, edges, start_data);
        }
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        // TODO popup grabs
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        // TODO
        surface.send_configure();
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        // TODO
    }

    fn fullscreen_request(&mut self, surface: ToplevelSurface, mut wl_output: Option<WlOutput>) {
        if surface
            .current_state()
            .capabilities
            .contains(xdg_toplevel::WmCapabilities::Fullscreen)
        {
            // NOTE: This is only one part of the solution. We can set the
            // location and configure size here, but the surface should be rendered fullscreen
            // independently from its buffer size
            let wl_surface = surface.wl_surface();

            let output_geometry = fullscreen_output_geometry(
                wl_surface,
                wl_output.as_ref(),
                self.workspaces.current_workspace_mut().space_mut(),
            );

            if let Some(geometry) = output_geometry {
                let output = wl_output
                    .as_ref()
                    .and_then(Output::from_resource)
                    .unwrap_or_else(|| self.workspaces.outputs().next().unwrap().clone());
                let client = self.display_handle.get_client(wl_surface.id()).unwrap();
                for output in output.client_outputs(&client) {
                    wl_output = Some(output);
                }
                let window = self
                    .workspaces
                    .current_workspace()
                    .windows()
                    .find(|window| {
                        window
                            .wl_surface()
                            .map(|s| s == *wl_surface)
                            .unwrap_or(false)
                    })
                    .unwrap();

                surface.with_pending_state(|state| {
                    state.states.set(xdg_toplevel::State::Fullscreen);
                    state.size = Some(geometry.size);
                    state.fullscreen_output = wl_output;
                });
                output
                    .user_data()
                    .insert_if_missing(FullscreenSurface::default);
                output
                    .user_data()
                    .get::<FullscreenSurface>()
                    .unwrap()
                    .set(window.clone(), self.workspaces.current_workspace_index());
                trace!("Fullscreening: {:?}", window);
            }
        }

        // The protocol demands us to always reply with a configure,
        // regardless of we fulfilled the request or not
        surface.send_configure();
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        if !surface
            .current_state()
            .states
            .contains(xdg_toplevel::State::Fullscreen)
        {
            return;
        }

        let ret = surface.with_pending_state(|state| {
            state.states.unset(xdg_toplevel::State::Fullscreen);
            state.size = None;
            state.fullscreen_output.take()
        });
        if let Some(output) = ret {
            let output = Output::from_resource(&output).unwrap();
            if let Some(fullscreen) = output.user_data().get::<FullscreenSurface>() {
                trace!("Unfullscreening: {:?}", fullscreen.get());
                fullscreen.clear();
                self.backend_data.reset_buffers(&output);
            }
        }

        surface.send_pending_configure();
    }
}

// Xdg Shell
delegate_xdg_shell!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

fn check_grab<BackendData: Backend + 'static>(
    seat: &Seat<Buddaraysh<BackendData>>,
    surface: &WlSurface,
    serial: Serial,
) -> Option<PointerGrabStartData<Buddaraysh<BackendData>>> {
    let pointer = seat.get_pointer()?;

    // Check that this surface has a click grab.
    if !pointer.has_grab(serial) {
        return None;
    }

    let start_data = pointer.grab_start_data()?;

    let (focus, _) = start_data.focus.as_ref()?;

    let wl_surface = WaylandFocus::wl_surface(focus).unwrap();

    // If the focus was for a different surface, ignore the request.
    if !wl_surface.id().same_client_as(&surface.id()) {
        return None;
    }

    Some(start_data)
}

/// Should be called on `WlSurface::commit`
pub fn handle_commit(popups: &mut PopupManager, space: &Space<WindowElement>, surface: &WlSurface) {
    // Handle toplevel commits.
    if let Some(WindowElement::Wayland(ref window)) = space
        .elements()
        .find(|window| window.wl_surface().map(|s| s == *surface).unwrap_or(false))
        .cloned()
    {
        let initial_configure_sent = with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap()
                .initial_configure_sent
        });

        if !initial_configure_sent {
            window.toplevel().send_configure();
        }
    }

    // Handle popup commits.
    popups.commit(surface);
    if let Some(popup) = popups.find_popup(surface) {
        match popup {
            PopupKind::Xdg(ref xdg) => {
                let initial_configure_sent = with_states(surface, |states| {
                    states
                        .data_map
                        .get::<XdgPopupSurfaceData>()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .initial_configure_sent
                });
                if !initial_configure_sent {
                    // NOTE: This should never fail as the initial configure is always
                    // allowed.
                    xdg.send_configure().expect("initial configure failed");
                }
            }
            PopupKind::InputMethod(ref _input_method) => {}
        }
    }
}

impl<BackendData: Backend> Buddaraysh<BackendData> {
    pub fn move_request_xdg(
        &mut self,
        surface: &ToplevelSurface,
        seat: &Seat<Self>,
        serial: Serial,
        start_data: smithay::input::pointer::GrabStartData<Buddaraysh<BackendData>>,
    ) {
        // TODO: touch move.
        let pointer = seat.get_pointer().unwrap();

        // If the client disconnects after requesting a move
        // we can just ignore the request
        let Some(window) = self.window_for_surface(surface.wl_surface()) else {
            return;
        };

        // If the focus was for a different surface, ignore the request.
        if start_data.focus.is_none()
            || !start_data
                .focus
                .as_ref()
                .unwrap()
                .0
                .same_client_as(&surface.wl_surface().id())
        {
            tracing::info!("not the same");
            return;
        }

        let mut initial_window_location = self
            .workspaces
            .current_workspace()
            .window_location(&window)
            .unwrap();

        // If surface is maximized then unmaximize it
        let current_state = surface.current_state();
        if current_state
            .states
            .contains(xdg_toplevel::State::Maximized)
        {
            surface.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Maximized);
                state.size = None;
            });

            surface.send_configure();

            // NOTE: In real compositor mouse location should be mapped to a new window size
            // For example, you could:
            // 1) transform mouse pointer position from compositor space to window space (location relative)
            // 2) divide the x coordinate by width of the window to get the percentage
            //   - 0.0 would be on the far left of the window
            //   - 0.5 would be in middle of the window
            //   - 1.0 would be on the far right of the window
            // 3) multiply the percentage by new window width
            // 4) by doing that, drag will look a lot more natural
            //
            // but for now setting location to pointer location is fine
            let pos = pointer.current_location();
            initial_window_location = (pos.x as i32, pos.y as i32).into();
        }

        let grab = MoveSurfaceGrab {
            start_data,
            window,
            initial_window_location,
        };

        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    pub fn resize_request_xdg(
        &mut self,
        surface: ToplevelSurface,
        seat: Seat<Buddaraysh<BackendData>>,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
        start_data: smithay::input::pointer::GrabStartData<Buddaraysh<BackendData>>,
    ) {
        let wl_surface = surface.wl_surface();

        let pointer = seat.get_pointer().unwrap();

        let window = self
            .workspaces
            .current_workspace()
            .windows()
            .find(|window| {
                window
                    .wl_surface()
                    .map(|s| s == *wl_surface)
                    .unwrap_or(false)
            })
            .unwrap()
            .clone();
        let initial_window_location = self
            .workspaces
            .current_workspace()
            .window_location(&window)
            .unwrap();
        let initial_window_size = window.geometry().size;

        surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Resizing);
        });

        surface.send_pending_configure();

        let initial_rect =
            Rectangle::from_loc_and_size(initial_window_location, initial_window_size);

        compositor::with_states(surface.wl_surface(), |states| {
            states
                .data_map
                .insert_if_missing(RefCell::<ResizeSurfaceState>::default);
            let state = states
                .data_map
                .get::<RefCell<ResizeSurfaceState>>()
                .unwrap();

            *state.borrow_mut() = ResizeSurfaceState::Resizing {
                edges: edges.into(),
                initial_rect,
            };
        });

        let grab = ResizeSurfaceGrab {
            start_data,
            window,
            edges: edges.into(),
            initial_rect,
            last_window_size: initial_rect.size,
        };

        pointer.set_grab(self, grab, serial, Focus::Clear);
    }
}

impl<BackendData: Backend + 'static> XdgActivationHandler for Buddaraysh<BackendData> {
    fn activation_state(&mut self) -> &mut smithay::wayland::xdg_activation::XdgActivationState {
        &mut self.xdg_activation_state
    }

    fn request_activation(
        &mut self,
        token: smithay::wayland::xdg_activation::XdgActivationToken,
        token_data: smithay::wayland::xdg_activation::XdgActivationTokenData,
        surface: WlSurface,
    ) {
        debug!("activation request");
    }
}

delegate_xdg_activation!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);
