use std::{cell::RefCell, os::unix::io::OwnedFd};

use smithay::{
    desktop::space::SpaceElement,
    input::pointer::Focus,
    utils::{Logical, Rectangle, SERIAL_COUNTER},
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
    grabs::{resize_grab::ResizeSurfaceState, MoveSurfaceGrab, ResizeSurfaceGrab},
    shell::FullscreenSurface,
    ssd::HEADER_BAR_HEIGHT,
    window::WindowElement,
    Backend, Buddaraysh, CalloopData, OutputExt,
};

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

    fn map_window_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        surface.set_mapped(true).unwrap();
        let window = WindowElement::X11(surface);
        if let Some(output) = self
            .state
            .output_under(self.state.pointer.current_location())
        {
            let location = self.state.pointer.current_location();
            if let Some(workspace) = self.state.workspace_for_output_mut(&output) {
                workspace.map_window(window.clone());
                let bbox = workspace.window_bbox(&window).unwrap();
                window.set_geometry(bbox);
                window.set_ssd(!window.is_decorated(false));
            }
        }
        // place_new_window(
        //     self.state.workspaces.current_workspace_mut().space_mut(),
        //     self.state.pointer.current_location(),
        //     &window,
        //     true,
        // );
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, surface: X11Surface) {
        self.state.override_redirect_windows.push(surface.clone());

        let location = surface.geometry().loc;
        let window = WindowElement::X11(surface);
        if let Some(workspace) = self.state.workspace_for_mut(&window) {
            workspace.map_window(window);
        }
    }

    fn unmapped_window(&mut self, _xwm: XwmId, surface: X11Surface) {
        let window = WindowElement::X11(surface.clone());
        if let Some(workspace) = self.state.workspace_for_mut(&window) {
            let maybe = workspace.windows().find(|e| **e == window).cloned();
            if let Some(elem) = maybe {
                workspace.unmap_window(&elem);
            }
        }

        if surface.is_override_redirect() {
            self.state
                .override_redirect_windows
                .retain(|or| or != &surface);
        } else {
            surface.set_mapped(false).unwrap();
        }
    }

    fn destroyed_window(&mut self, _xwm: XwmId, surface: X11Surface) {
        let window = WindowElement::X11(surface.clone());
        if let Some(workspace) = self.state.workspace_for_mut(&window) {
            let maybe = workspace.windows().find(|e| **e == window).cloned();
            if let Some(window) = maybe {
                workspace.unmap_window(&window);
            }
        }

        if surface.is_override_redirect() {
            self.state
                .override_redirect_windows
                .retain(|win| win != &surface);
        } else {
            surface.set_mapped(false).unwrap();
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
        surface: X11Surface,
        geometry: Rectangle<i32, Logical>,
        _above: Option<u32>,
    ) {
        let window = WindowElement::X11(surface);
        if let Some(workspace) = self.state.workspace_for_mut(&window) {
            workspace.map_window(window);
        }
        // TODO: We don't properly handle the order of override-redirect windows here,
        //       they are always mapped top and then never reordered.
    }

    fn maximize_request(&mut self, _xwm: XwmId, window: X11Surface) {
        self.state.maximize_request_x11(&window);
    }

    fn unmaximize_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Some(window) = self
            .state
            .window_for_surface(&surface.wl_surface().unwrap())
        {
            if let Some(workspace) = self.state.workspace_for_mut(&window) {
                surface.set_maximized(false).unwrap();
                if let Some(old_geo) = surface
                    .user_data()
                    .get::<OldGeometry>()
                    .and_then(|data| data.restore())
                {
                    surface.configure(old_geo).unwrap();
                    workspace.map_window(window);
                }
            }
        }
    }

    fn fullscreen_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Some(window) = self
            .state
            .window_for_surface(&surface.wl_surface().unwrap())
        {
            if let Some(workspace) = self.state.workspace_for_mut(&window) {
                workspace.fullscreen_request(&window, None)
            }
        }
    }

    fn unfullscreen_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        let window = WindowElement::X11(surface);
        let output = if let Some(workspace) = self
            .state
            .window_for_surface(&window.wl_surface().unwrap())
            .and_then(|window| self.state.workspace_for_mut(&window))
        {
            let previous = workspace.unfullscreen_request(&window);
            window.set_ssd(!window.is_decorated(false));
            trace!("Unfullscreening: {:?}", window);
            assert!(previous.is_none());
            Some(workspace.output.clone())
        } else {
            None
        };
        if let Some(output) = output {
            self.state.backend_data.reset_buffers(&output);
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
        self.state
            .resize_request_x11(edges, &x11_surface, start_data);
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
        surface: &X11Surface,
        start_data: smithay::input::pointer::GrabStartData<Buddaraysh<BackendData>>,
    ) {
        let pointer = self.pointer.clone();
        let serial = SERIAL_COUNTER.next_serial();

        if let Some(window) = self.window_for_surface(&surface.wl_surface().unwrap()) {
            if let Some(workspace) = self.workspace_for(&window) {
                let geometry = window.geometry();
                let loc = workspace.window_location(&window).unwrap();
                let (initial_window_location, initial_window_size) = (loc, geometry.size);

        let geometry = window.geometry();
        let loc = self
            .workspaces
            .current_workspace()
            .window_location(window)
            .unwrap();

        let mut initial_rect = Rectangle::from_loc_and_size(loc, geometry.size);

        if window.decoration_state().is_ssd {
            initial_rect.size.h -= HEADER_BAR_HEIGHT;
        }

                    *state.borrow_mut() = ResizeSurfaceState::Resizing {
                        edges: edges.into(),
                        initial_rect,
                    };
                });

                tracing::info!(?edges);

                let grab = ResizeSurfaceGrab {
                    start_data,
                    window: window.clone(),
                    edges: edges.into(),
                    initial_rect,
                    last_window_size: initial_window_size,
                };

                pointer.set_grab(self, grab, serial, Focus::Clear);
            }
        }
    }
}

impl<BackendData: Backend> Buddaraysh<BackendData> {
    pub fn maximize_request_x11(&mut self, surface: &X11Surface) {
        if let Some(window) = self.window_for_surface(&surface.wl_surface().unwrap()) {
            if let Some(workspace) = self.workspace_for_mut(&window) {
                let old_geo = workspace.window_bbox(&window).unwrap();
                let geometry = workspace.output.geometry();

                surface.set_maximized(true).unwrap();
                window.set_geometry(geometry);
                window.user_data().insert_if_missing(OldGeometry::default);
                window
                    .user_data()
                    .get::<OldGeometry>()
                    .unwrap()
                    .save(old_geo);
                workspace.map_window(window);
            }
        }
    }

    pub fn move_request_x11(
        &mut self,
        surface: &X11Surface,
        start_data: smithay::input::pointer::GrabStartData<Buddaraysh<BackendData>>,
    ) {
        if let Some(window) = self.window_for_surface(&surface.wl_surface().unwrap()) {
            if let Some(workspace) = self.workspace_for_mut(&window) {
                let mut initial_window_location = workspace.window_location(&window).unwrap();

                // If surface is maximized then unmaximize it
                if surface.is_maximized() {
                    surface.set_maximized(false).unwrap();
                    let pos = self.pointer.current_location();
                    initial_window_location = (pos.x as i32, pos.y as i32).into();
                    if let Some(old_geo) = window
                        .user_data()
                        .get::<OldGeometry>()
                        .and_then(|data| data.restore())
                    {
                        window.set_geometry(Rectangle::from_loc_and_size(
                            initial_window_location,
                            old_geo.size,
                        ));
                    }
                }

                let grab = MoveSurfaceGrab {
                    start_data,
                    window: window.clone(),
                    initial_window_location,
                };

                let pointer = self.pointer.clone();
                pointer.set_grab(self, grab, SERIAL_COUNTER.next_serial(), Focus::Clear);
            }
        }
    }
}
