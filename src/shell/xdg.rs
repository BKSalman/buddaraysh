use std::cell::RefCell;

use smithay::{
    delegate_xdg_activation, delegate_xdg_shell,
    desktop::{
        find_popup_root_surface, get_popup_toplevel_coords, layer_map_for_output,
        space::SpaceElement, LayerSurface, PopupKind, PopupManager, Space, Window,
        WindowSurfaceType,
    },
    input::{
        pointer::{Focus, GrabStartData as PointerGrabStartData},
        Seat,
    },
    output::Output,
    reexports::{
        wayland_protocols::xdg::shell::server::{
            xdg_positioner::ConstraintAdjustment, xdg_toplevel,
        },
        wayland_server::{
            protocol::{wl_output::WlOutput, wl_seat, wl_surface::WlSurface},
            Resource,
        },
    },
    utils::{Logical, Rectangle, Serial},
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
    focus::FocusTarget,
    grabs::{resize_grab::ResizeSurfaceState, MoveSurfaceGrab, ResizeSurfaceGrab},
    shell::{layout::ManagedLayer, FullscreenSurface},
    utils::geometry::{PointExt, PointLocalExt, SizeExt},
    window::{WindowElement, WindowMapped},
    Backend, Buddaraysh, OutputExt,
};

impl<BackendData: Backend + 'static> XdgShellHandler for Buddaraysh<BackendData> {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let window = WindowMapped::new(WindowElement::Wayland(Window::new(surface)), None);
        if let Some(output) = self.output_under(self.pointer.current_location().as_global()) {
            let workspace = self.current_workspace_mut(&output);
            workspace.map_window(window);
            // place_new_window(
            //     self.workspaces.current_workspace_mut().space_mut(),
            //     self.pointer.current_location(),
            //     &window,
            //     true,
            // );
        }

        for workspace in self.workspaces.workspaces_mut() {
            workspace.refresh();
            workspace.tile_windows();
        }
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        let _ = self.popups.track_popup(PopupKind::from(surface));
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        tracing::info!("reposition request");
        surface.with_pending_state(|state| {
            // NOTE: This is again a simplification, a proper compositor would
            // calculate the geometry of the popup here. For simplicity we just
            // use the default implementation here that does not take the
            // window position and output constraints into account.
            let pointer_location = self.pointer.current_location().as_global();
            let output = self
                .output_under(pointer_location)
                .or_else(|| self.outputs().next().cloned())
                .unwrap();

            let output_geometry = output.geometry();

            let geometry = positioner.get_geometry();

            // TODO
            // tracing::info!(?output_geometry, ?geometry);

            // if geometry.loc.x + geometry.size.w > output_geometry.size.w {

            // }
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

    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let seat: Seat<Buddaraysh<BackendData>> = Seat::from_resource(&seat).unwrap();
        let kind = PopupKind::Xdg(surface);
        if let Some(root) = find_popup_root_surface(&kind).ok().and_then(|root| {
            self.window_for_surface(&root)
                .map(FocusTarget::Window)
                .or_else(|| {
                    self.outputs()
                        .find_map(|o| {
                            let map = layer_map_for_output(o);
                            map.layer_for_surface(&root, WindowSurfaceType::TOPLEVEL)
                                .cloned()
                        })
                        .map(FocusTarget::LayerSurface)
                })
        }) {
            let ret = self.popups.grab_popup(root, kind, &seat, serial);

            if let Ok(mut grab) = ret {
                if let Some(keyboard) = seat.get_keyboard() {
                    if keyboard.is_grabbed()
                        && !(keyboard.has_grab(serial)
                            || keyboard.has_grab(grab.previous_serial().unwrap_or(serial)))
                    {
                        grab.ungrab(PopupUngrabStrategy::All);
                        return;
                    }
                    keyboard.set_focus(self, grab.current_grab(), serial);
                    keyboard.set_grab(PopupKeyboardGrab::new(&grab), serial);
                }
                if let Some(pointer) = seat.get_pointer() {
                    if pointer.is_grabbed()
                        && !(pointer.has_grab(serial)
                            || pointer
                                .has_grab(grab.previous_serial().unwrap_or_else(|| grab.serial())))
                    {
                        grab.ungrab(PopupUngrabStrategy::All);
                        return;
                    }
                    pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
                }
            }
        }
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

            let output = wl_output
                .as_ref()
                .and_then(Output::from_resource)
                .unwrap_or_else(|| self.outputs().next().unwrap().clone());

            // let output_geometry = fullscreen_output_geometry(
            //     wl_surface,
            //     wl_output.as_ref(),
            //     self.workspaces.current_workspace_mut(),
            // );

            let workspace = self.current_workspace(&output);
            let output_geometry = output.geometry();
            let client = self.display_handle.get_client(wl_surface.id()).unwrap();
            for output in output.client_outputs(&client) {
                wl_output = Some(output);
            }
            let window = self.window_for_surface(wl_surface).unwrap();

            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Fullscreen);
                state.size = Some(output_geometry.size.as_logical());
                state.fullscreen_output = wl_output;
            });
            let layer = if workspace.is_tiled(&window) {
                ManagedLayer::Tiling
            } else {
                ManagedLayer::Floating
            };
            trace!("Fullscreening: {:?}", window);

            let workspace = self.current_workspace_mut(&output);
            workspace.fullscreen = Some(FullscreenSurface {
                window: window.clone(),
                previously: Some((layer, workspace.handle)),
                original_geometry: window.geometry(),
            });
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
            let workspace = self.current_workspace_mut(&output);
            if let Some(fullscreen) = workspace.fullscreen.take() {
                trace!("Unfullscreening: {:?}", fullscreen);
                self.backend_data.reset_buffers(&output);
            }
        }

        surface.send_pending_configure();
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        if let Some(window) = self.window_for_surface(surface.wl_surface()) {
            if let Some(workspace) = self.workspace_for_mut(&window) {
                workspace.unmap_window(&window);
                workspace.refresh();
                workspace.tile_windows();
            }
        }
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
pub fn handle_commit(
    popups: &mut PopupManager,
    window: Option<&WindowMapped>,
    surface: &WlSurface,
) {
    // Handle toplevel commits.
    if let Some(w) = &window {
        if let WindowElement::Wayland(window) = &w.element {
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
            return;
        }

        let Some(workspace) = self.workspace_for(&window) else {
            return;
        };

        let mut initial_window_location = workspace.window_location(&window).unwrap();

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
            initial_window_location: initial_window_location.as_logical(),
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
            .workspaces()
            .flat_map(|w| w.windows())
            .find(|window| {
                window
                    .wl_surface()
                    .map(|s| s == *wl_surface)
                    .unwrap_or(false)
            })
            .unwrap()
            .clone();

        let Some(workspace) = self.workspace_for(&window) else {
            return;
        };

        let initial_window_location = workspace.window_location(&window).unwrap();
        let initial_window_size = window.geometry().size;

        surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Resizing);
        });

        surface.send_pending_configure();

        let initial_rect =
            Rectangle::from_loc_and_size(initial_window_location.as_logical(), initial_window_size);

        if window.decoration_state().is_ssd {
            initial_rect.size.h -= HEADER_BAR_HEIGHT;
        }

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

    pub fn unconstrain_popup(&self, popup: &PopupSurface) {
        // Popups with a NULL parent will get repositioned in their respective protocol handlers
        // (i.e. layer-shell).
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };

        // Figure out if the root is a window or a layer surface.
        if let Some((window, output)) = self
            .window_for_surface(&root)
            .zip(self.workspaces.current_workspace().outputs().next())
        {
            self.unconstrain_window_popup(popup, &window, output);
        } else if let Some((layer_surface, output)) = self.workspaces.outputs().find_map(|o| {
            let map = layer_map_for_output(o);
            let layer_surface = map.layer_for_surface(&root, WindowSurfaceType::TOPLEVEL)?;
            Some((layer_surface.clone(), o))
        }) {
            self.unconstrain_layer_shell_popup(popup, &layer_surface, output);
        }
    }

    fn unconstrain_window_popup(
        &self,
        popup: &PopupSurface,
        window: &WindowElement,
        output: &Output,
    ) {
        let workspace = self.workspaces.current_workspace();
        let output_geo = workspace.output_geometry(output).unwrap();
        let window_location = workspace.window_location(window).unwrap();

        let mut target = Rectangle::from_loc_and_size((0, 0), output_geo.size);
        target.loc -= window_location;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));

        popup.with_pending_state(|state| {
            state.geometry = unconstrain_with_padding(state.positioner, target);
        });
    }

    pub fn unconstrain_layer_shell_popup(
        &self,
        popup: &PopupSurface,
        layer_surface: &LayerSurface,
        output: &Output,
    ) {
        let output_geo = self
            .workspaces
            .current_workspace()
            .output_geometry(output)
            .unwrap();
        let map = layer_map_for_output(output);
        let Some(layer_geo) = map.layer_geometry(layer_surface) else {
            return;
        };

        // The target geometry for the positioner should be relative to its parent's geometry, so
        // we will compute that here.
        let mut target = Rectangle::from_loc_and_size((0, 0), output_geo.size);
        target.loc -= layer_geo.loc;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));

        popup.with_pending_state(|state| {
            state.geometry = unconstrain_with_padding(state.positioner, target);
        });
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

fn unconstrain_with_padding(
    positioner: PositionerState,
    target: Rectangle<i32, Logical>,
) -> Rectangle<i32, Logical> {
    // Try unconstraining with a small padding first which looks nicer, then if it doesn't fit try
    // unconstraining without padding.
    const PADDING: i32 = 8;

    let mut padded = target;
    if PADDING * 2 < padded.size.w {
        padded.loc.x += PADDING;
        padded.size.w -= PADDING * 2;
    }
    if PADDING * 2 < padded.size.h {
        padded.loc.y += PADDING;
        padded.size.h -= PADDING * 2;
    }

    // No padding, so just unconstrain with the original target.
    if padded == target {
        return positioner.get_unconstrained_geometry(target);
    }

    // Do not try to resize to fit the padded target rectangle.
    let mut no_resize = positioner;
    no_resize
        .constraint_adjustment
        .remove(ConstraintAdjustment::ResizeX);
    no_resize
        .constraint_adjustment
        .remove(ConstraintAdjustment::ResizeY);

    let geo = no_resize.get_unconstrained_geometry(padded);
    if padded.contains_rect(geo) {
        return geo;
    }

    // Could not unconstrain into the padded target, so resort to the regular one.
    positioner.get_unconstrained_geometry(target)
}
