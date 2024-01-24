use std::{cell::RefCell, os::unix::io::OwnedFd};

use smithay::{
    desktop::space::SpaceElement,
    input::{
        pointer::{Focus, GrabStartData},
        Seat,
    },
    utils::{Logical, Rectangle, Serial, SERIAL_COUNTER},
    wayland::{
        compositor,
        selection::data_device::{
            clear_data_device_selection, current_data_device_selection_userdata,
            request_data_device_client_selection, set_data_device_selection,
        },
        selection::primary_selection::{
            clear_primary_selection, current_primary_selection_userdata,
            request_primary_client_selection, set_primary_selection,
        },
        selection::SelectionTarget,
    },
    xwayland::{
        xwm::{Reorder, ResizeEdge as X11ResizeEdge, XwmId},
        X11Surface, X11Wm, XwmHandler,
    },
};
use tracing::{error, trace};

use crate::{
    focus::FocusTarget,
    grabs::{
        resize_grab::{self, ResizeSurfaceState},
        MoveSurfaceGrab, ResizeSurfaceGrab,
    },
    shell::FullscreenSurface,
    ssd::HEADER_BAR_HEIGHT,
    window::WindowElement,
    Backend, Buddaraysh, CalloopData,
};

use super::place_new_window;

#[derive(Debug, Default)]
struct OldGeometry(RefCell<Option<Rectangle<i32, Logical>>>);
impl OldGeometry {
    pub fn save(&self, geo: Rectangle<i32, Logical>) {
        *self.0.borrow_mut() = Some(geo);
    }

    pub fn restore(&self) -> Option<Rectangle<i32, Logical>> {
        self.0.borrow_mut().take()
    }
}

impl<BackendData: Backend> XwmHandler for CalloopData<BackendData> {
    fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
        self.state.xwm.as_mut().unwrap()
    }

    fn new_window(&mut self, _xwm: XwmId, _window: X11Surface) {}
    fn new_override_redirect_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn map_window_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Err(err) = window.set_mapped(true) {
            tracing::warn!(?window, ?err, "Failed to send Xwayland Mapped-Event");
        }

        let window = WindowElement::X11(window);
        place_new_window(
            self.state.workspaces.current_workspace_mut().space_mut(),
            self.state.pointer.current_location(),
            &window,
            true,
        );
        let bbox = self
            .state
            .workspaces
            .current_workspace()
            .window_bbox(&window)
            .unwrap();
        let WindowElement::X11(xsurface) = &window else {
            unreachable!()
        };
        if !xsurface.is_override_redirect() {
            xsurface.configure(Some(bbox)).unwrap();
            window.set_ssd(!xsurface.is_decorated());
        }
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
        self.state.override_redirect_windows.push(window.clone());

        let location = window.geometry().loc;
        let window = WindowElement::X11(window);
        self.state
            .workspaces
            .current_workspace_mut()
            .map_window(window, location, true);
    }

    fn unmapped_window(&mut self, _xwm: XwmId, window: X11Surface) {
        let maybe = self
            .state
            .workspaces
            .current_workspace()
            .windows()
            .find(|e| matches!(e, WindowElement::X11(w) if w == &window))
            .cloned();
        if let Some(elem) = maybe {
            self.state
                .workspaces
                .current_workspace_mut()
                .unmap_window(&elem);
        }

        if window.is_override_redirect() {
            self.state
                .override_redirect_windows
                .retain(|or| or != &window);
        } else {
            window.set_mapped(false).unwrap();
        }
    }

    fn destroyed_window(&mut self, _xwm: XwmId, window: X11Surface) {
        let maybe = self
            .state
            .workspaces
            .current_workspace()
            .windows()
            .find(|e| matches!(e, WindowElement::X11(w) if w == &window))
            .cloned();
        if let Some(win) = maybe {
            tracing::debug!("removing x11 window from windows");
            self.state
                .workspaces
                .current_workspace_mut()
                .unmap_window(&win);
        }

        if window.is_override_redirect() {
            self.state
                .override_redirect_windows
                .retain(|win| win != &window);
        } else {
            window.set_mapped(false).unwrap();
        }
    }

    fn configure_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        _x: Option<i32>,
        _y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<Reorder>,
    ) {
        // we just set the new size, but don't let windows move themselves around freely
        let mut geo = window.geometry();
        if let Some(w) = w {
            geo.size.w = w as i32;
        }
        if let Some(h) = h {
            geo.size.h = h as i32;
        }
        let _ = window.configure(geo);
    }

    fn configure_notify(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        geometry: Rectangle<i32, Logical>,
        _above: Option<u32>,
    ) {
        let Some(elem) = self
            .state
            .workspaces
            .current_workspace()
            .windows()
            .find(|e| matches!(e, WindowElement::X11(w) if w == &window))
            .cloned()
        else {
            return;
        };
        self.state
            .workspaces
            .current_workspace_mut()
            .map_window(elem, geometry.loc, false);
        // TODO: We don't properly handle the order of override-redirect windows here,
        //       they are always mapped top and then never reordered.
    }

    fn maximize_request(&mut self, _xwm: XwmId, window: X11Surface) {
        self.state.maximize_request_x11(&window);
    }

    fn unmaximize_request(&mut self, _xwm: XwmId, window: X11Surface) {
        let Some(elem) = self
            .state
            .workspaces
            .current_workspace()
            .windows()
            .find(|e| matches!(e, WindowElement::X11(w) if w == &window))
            .cloned()
        else {
            return;
        };

        window.set_maximized(false).unwrap();
        if let Some(old_geo) = window
            .user_data()
            .get::<OldGeometry>()
            .and_then(|data| data.restore())
        {
            window.configure(old_geo).unwrap();
            self.state
                .workspaces
                .current_workspace_mut()
                .map_window(elem, old_geo.loc, false);
        }
    }

    fn fullscreen_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(elem) = self
            .state
            .workspaces
            .current_workspace()
            .windows()
            .find(|e| matches!(e, WindowElement::X11(w) if w == &window))
        {
            let outputs_for_window = self
                .state
                .workspaces
                .current_workspace()
                .outputs_for_window(elem);
            let output = outputs_for_window
                .first()
                // The window hasn't been mapped yet, use the primary output instead
                .or_else(|| self.state.workspaces.outputs().next())
                // Assumes that at least one output exists
                .expect("No outputs found");
            let geometry = self
                .state
                .workspaces
                .current_workspace()
                .output_geometry(output)
                .unwrap();

            window.set_fullscreen(true).unwrap();
            elem.set_ssd(false);
            window.configure(geometry).unwrap();
            output
                .user_data()
                .insert_if_missing(FullscreenSurface::default);
            output.user_data().get::<FullscreenSurface>().unwrap().set(
                elem.clone(),
                self.state.workspaces.current_workspace_index(),
            );
            trace!("Fullscreening: {:?}", elem);
        }
    }

    fn unfullscreen_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(elem) = self
            .state
            .workspaces
            .current_workspace()
            .windows()
            .find(|e| matches!(e, WindowElement::X11(w) if w == &window))
        {
            window.set_fullscreen(false).unwrap();
            elem.set_ssd(!window.is_decorated());
            if let Some(output) = self.state.workspaces.outputs().find(|o| {
                o.user_data()
                    .get::<FullscreenSurface>()
                    .and_then(|f| f.get().0)
                    .map(|w| &w == elem)
                    .unwrap_or(false)
            }) {
                trace!("Unfullscreening: {:?}", elem);
                output
                    .user_data()
                    .get::<FullscreenSurface>()
                    .unwrap()
                    .clear();
                window
                    .configure(self.state.workspaces.current_workspace().window_bbox(elem))
                    .unwrap();
                self.state.backend_data.reset_buffers(output);
            }
        }
    }

    fn resize_request(
        &mut self,
        _xwm: XwmId,
        x11_surface: X11Surface,
        _button: u32,
        edges: X11ResizeEdge,
    ) {
        let start_data = self.state.pointer.grab_start_data().unwrap();
        self.state.resize_request_x11(
            edges,
            x11_surface,
            self.state.seat.clone(),
            SERIAL_COUNTER.next_serial(),
            start_data,
        );
    }

    fn move_request(&mut self, _xwm: XwmId, window: X11Surface, _button: u32) {
        if let Some(start_data) = self.state.pointer.grab_start_data() {
            self.state.move_request_x11(&window, start_data);
        }
    }

    fn allow_selection_access(&mut self, xwm: XwmId, _selection: SelectionTarget) -> bool {
        if let Some(keyboard) = self.state.seat.get_keyboard() {
            // check that an X11 window is focused
            if let Some(FocusTarget::Window(WindowElement::X11(surface))) = keyboard.current_focus()
            {
                if surface.xwm_id().unwrap() == xwm {
                    return true;
                }
            }
        }
        false
    }

    fn send_selection(
        &mut self,
        _xwm: XwmId,
        selection: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
    ) {
        match selection {
            SelectionTarget::Clipboard => {
                if let Err(err) =
                    request_data_device_client_selection(&self.state.seat, mime_type, fd)
                {
                    error!(
                        ?err,
                        "Failed to request current wayland clipboard for Xwayland",
                    );
                }
            }
            SelectionTarget::Primary => {
                if let Err(err) = request_primary_client_selection(&self.state.seat, mime_type, fd)
                {
                    error!(
                        ?err,
                        "Failed to request current wayland primary selection for Xwayland",
                    );
                }
            }
        }
    }

    fn new_selection(&mut self, xwm: XwmId, selection: SelectionTarget, mime_types: Vec<String>) {
        trace!(?selection, ?mime_types, "Got Selection from X11",);
        if let Some(keyboard) = self.state.seat.get_keyboard() {
            if let Some(FocusTarget::Window(WindowElement::X11(surface))) = keyboard.current_focus()
            {
                if surface.xwm_id().unwrap() == xwm {
                    match selection {
                        SelectionTarget::Clipboard => set_data_device_selection(
                            &self.state.display_handle,
                            &self.state.seat,
                            mime_types,
                            (),
                        ),
                        SelectionTarget::Primary => set_primary_selection(
                            &self.state.display_handle,
                            &self.state.seat,
                            mime_types,
                            (),
                        ),
                    }
                }
            }
        }
    }

    fn cleared_selection(&mut self, _xwm: XwmId, selection: SelectionTarget) {
        match selection {
            SelectionTarget::Clipboard => {
                if current_data_device_selection_userdata(&self.state.seat).is_some() {
                    clear_data_device_selection(&self.state.display_handle, &self.state.seat)
                }
            }
            SelectionTarget::Primary => {
                if current_primary_selection_userdata(&self.state.seat).is_some() {
                    clear_primary_selection(&self.state.display_handle, &self.state.seat)
                }
            }
        }
    }
}

impl<BackendData: Backend + 'static> Buddaraysh<BackendData> {
    pub fn resize_request_x11(
        &mut self,
        edges: X11ResizeEdge,
        x11_surface: X11Surface,
        seat: Seat<Buddaraysh<BackendData>>,
        serial: Serial,
        start_data: GrabStartData<Buddaraysh<BackendData>>,
    ) {
        let pointer = seat.get_pointer().unwrap();
        let edges = resize_grab::ResizeEdge::from(edges);

        let Some(window) = self
            .workspaces
            .current_workspace()
            .windows()
            .find(|e| matches!(e, WindowElement::X11(w) if w == &x11_surface))
        else {
            return;
        };

        let geometry = window.geometry();
        let loc = self
            .workspaces
            .current_workspace()
            .window_location(window)
            .unwrap();

        let mut initial_rect = Rectangle::from_loc_and_size(loc, geometry.size);

        if window.decoration_state().is_ssd {
            initial_rect.size.h -= HEADER_BAR_HEIGHT;
            initial_rect.loc.y += HEADER_BAR_HEIGHT;
        }

        compositor::with_states(&window.wl_surface().unwrap(), |states| {
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
            window: window.clone(),
            edges,
            initial_rect,
            last_window_size: initial_rect.size,
        };

        pointer.set_grab(self, grab, serial, Focus::Clear);
    }
}

impl<BackendData: Backend> Buddaraysh<BackendData> {
    pub fn maximize_request_x11(&mut self, window: &X11Surface) {
        let Some(elem) = self
            .workspaces
            .current_workspace()
            .windows()
            .find(|e| matches!(e, WindowElement::X11(w) if w == window))
            .cloned()
        else {
            return;
        };

        let old_geo = self
            .workspaces
            .current_workspace()
            .window_bbox(&elem)
            .unwrap();
        let outputs_for_window = self
            .workspaces
            .current_workspace()
            .outputs_for_window(&elem);
        let output = outputs_for_window
            .first()
            // The window hasn't been mapped yet, use the primary output instead
            .or_else(|| self.workspaces.outputs().next())
            // Assumes that at least one output exists
            .expect("No outputs found");
        let geometry = self
            .workspaces
            .current_workspace()
            .output_geometry(output)
            .unwrap();

        window.set_maximized(true).unwrap();
        window.configure(geometry).unwrap();
        window.user_data().insert_if_missing(OldGeometry::default);
        window
            .user_data()
            .get::<OldGeometry>()
            .unwrap()
            .save(old_geo);
        self.workspaces
            .current_workspace_mut()
            .map_window(elem, geometry.loc, false);
    }

    pub fn move_request_x11(
        &mut self,
        window: &X11Surface,
        start_data: smithay::input::pointer::GrabStartData<Buddaraysh<BackendData>>,
    ) {
        let Some(element) = self
            .workspaces
            .current_workspace()
            .windows()
            .find(|e| matches!(e, WindowElement::X11(w) if w == window))
        else {
            return;
        };

        let mut initial_window_location = self
            .workspaces
            .current_workspace()
            .window_location(element)
            .unwrap();

        // If surface is maximized then unmaximize it
        if window.is_maximized() {
            window.set_maximized(false).unwrap();
            let pos = self.pointer.current_location();
            initial_window_location = (pos.x as i32, pos.y as i32).into();
            if let Some(old_geo) = window
                .user_data()
                .get::<OldGeometry>()
                .and_then(|data| data.restore())
            {
                window
                    .configure(Rectangle::from_loc_and_size(
                        initial_window_location,
                        old_geo.size,
                    ))
                    .unwrap();
            }
        }

        let grab = MoveSurfaceGrab {
            start_data,
            window: element.clone(),
            initial_window_location,
        };

        let pointer = self.pointer.clone();
        pointer.set_grab(self, grab, SERIAL_COUNTER.next_serial(), Focus::Clear);
    }
}
