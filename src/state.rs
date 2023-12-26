use std::{
    collections::HashMap,
    ffi::OsString,
    os::fd::OwnedFd,
    process::Child,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

use smithay::{
    delegate_data_control, delegate_pointer_gestures, delegate_presentation,
    delegate_primary_selection, delegate_relative_pointer,
    desktop::{layer_map_for_output, space::SpaceElement, PopupManager},
    input::{
        pointer::{CursorImageStatus, PointerHandle},
        Seat, SeatState,
    },
    reexports::{
        calloop::{
            generic::Generic, EventLoop, Interest, LoopHandle, LoopSignal, Mode, PostAction,
        },
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
            Display, DisplayHandle,
        },
    },
    utils::{Clock, Logical, Monotonic, Point},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitState,
        output::OutputManagerState,
        pointer_constraints::PointerConstraintsState,
        pointer_gestures::PointerGesturesState,
        presentation::PresentationState,
        relative_pointer::RelativePointerManagerState,
        selection::{
            data_device::{DataDeviceHandler, DataDeviceState},
            primary_selection::{PrimarySelectionHandler, PrimarySelectionState},
            wlr_data_control::{DataControlHandler, DataControlState},
            SelectionHandler,
        },
        shell::{
            wlr_layer::{Layer as WlrLayer, WlrLayerShellState},
            xdg::XdgShellState,
        },
        shm::ShmState,
        socket::ListeningSocketSource,
        xdg_activation::XdgActivationState,
    },
    xwayland::X11Surface,
};

#[cfg(feature = "xwayland")]
use smithay::{
    delegate_xwayland_keyboard_grab,
    utils::Size,
    wayland::selection::{SelectionSource, SelectionTarget},
    wayland::xwayland_keyboard_grab::{XWaylandKeyboardGrabHandler, XWaylandKeyboardGrabState},
    xwayland::{X11Wm, XWayland, XWaylandEvent},
};

use crate::{
    cursor::Cursor, focus::FocusTarget, shell::FullscreenSurface, window::WindowElement,
    workspace::Workspaces, Backend, CalloopData,
};

pub type ChildID = u32;

pub struct Buddaraysh<BackendData: Backend + 'static> {
    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,
    pub loop_handle: LoopHandle<'static, CalloopData<BackendData>>,
    pub running: Arc<AtomicBool>,
    pub clock: Clock<Monotonic>,

    pub child_processes: HashMap<ChildID, Child>,
    pub reap_requested: Arc<AtomicBool>,

    pub backend_data: BackendData,

    pub workspaces: Workspaces,
    pub override_redirect_windows: Vec<X11Surface>,
    pub loop_signal: LoopSignal,

    // Smithay State
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub xdg_activation_state: XdgActivationState,
    pub wlr_layer_shell_state: WlrLayerShellState,
    pub data_control_state: DataControlState,
    pub shm_state: ShmState,
    pub primary_selection_state: PrimarySelectionState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Buddaraysh<BackendData>>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,
    pub presentation_state: PresentationState,
    pub keyboard_shortcuts_inhibit_state: KeyboardShortcutsInhibitState,

    pub dnd_icon: Option<WlSurface>,

    pub seat_name: String,
    pub seat: Seat<Self>,
    pub cursor_status: Arc<Mutex<CursorImageStatus>>,
    pub pointer: PointerHandle<Buddaraysh<BackendData>>,

    #[cfg(feature = "xwayland")]
    pub xwayland: XWayland,
    #[cfg(feature = "xwayland")]
    pub xwm: Option<X11Wm>,
    #[cfg(feature = "xwayland")]
    pub xdisplay: Option<u32>,
}

impl<BackendData: Backend + 'static> Buddaraysh<BackendData> {
    pub fn new(
        loop_handle: LoopHandle<'static, CalloopData<BackendData>>,
        event_loop: &mut EventLoop<CalloopData<BackendData>>,
        display: Display<Self>,
        backend_data: BackendData,
    ) -> Self {
        let start_time = std::time::Instant::now();

        let clock = Clock::new();

        let display_handle = display.handle();

        let compositor_state = CompositorState::new::<Self>(&display_handle);
        let xdg_shell_state = XdgShellState::new::<Self>(&display_handle);
        let xdg_activation_state = XdgActivationState::new::<Self>(&display_handle);
        let wlr_layer_shell_state = WlrLayerShellState::new::<Self>(&display_handle);
        let primary_selection_state = PrimarySelectionState::new::<Self>(&display_handle);
        let data_control_state = DataControlState::new::<Self, _>(
            &display_handle,
            Some(&primary_selection_state),
            |_| true,
        );
        let shm_state = ShmState::new::<Self>(&display_handle, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&display_handle);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&display_handle);
        let popups = PopupManager::default();
        let presentation_state = PresentationState::new::<Self>(&display_handle, clock.id() as u32);
        let keyboard_shortcuts_inhibit_state =
            KeyboardShortcutsInhibitState::new::<Self>(&display_handle);
        PointerConstraintsState::new::<Self>(&display_handle);
        RelativePointerManagerState::new::<Self>(&display_handle);
        PointerGesturesState::new::<Self>(&display_handle);

        // A seat is a group of keyboards, pointer and touch devices.
        // A seat typically has a pointer and maintains a keyboard focus and a pointer focus.
        let seat_name = backend_data.seat_name();
        let mut seat = seat_state.new_wl_seat(&display_handle, seat_name.clone());

        // Notify clients that we have a keyboard, for the sake of the example we assume that keyboard is always present.
        // You may want to track keyboard hot-plug in real compositor.
        seat.add_keyboard(
            smithay::input::keyboard::XkbConfig {
                layout: "us,ara",
                options: Some(String::from("grp:alt_shift_toggle")),
                ..Default::default()
            },
            200,
            25,
        )
        .unwrap();

        // Notify clients that we have a pointer (mouse)
        // Here we assume that there is always pointer plugged in
        let pointer = seat.add_pointer();

        // A space represents a two-dimensional plane. Windows and Outputs can be mapped onto it.
        //
        // Windows get a position and stacking order through mapping.
        // Outputs become views of a part of the Space and can be rendered via Space::render_output.
        // let space = Space::default();

        let socket_name = Self::init_wayland_listener(display, event_loop);

        std::env::set_var("WAYLAND_DISPLAY", &socket_name);

        // Get the loop signal, used to stop the event loop
        let loop_signal = event_loop.get_signal();

        let cursor_status = Arc::new(Mutex::new(CursorImageStatus::default_named()));

        #[cfg(feature = "xwayland")]
        let xwayland = {
            XWaylandKeyboardGrabState::new::<Self>(&display_handle);

            let (xwayland, channel) = XWayland::new(&display_handle);
            let display_handle = display_handle.clone();
            let ret = loop_handle.insert_source(channel, move |event, _, data| match event {
                XWaylandEvent::Ready {
                    connection,
                    client,
                    client_fd: _,
                    display,
                } => {
                    let mut wm = X11Wm::start_wm(
                        data.state.loop_handle.clone(),
                        display_handle.clone(),
                        connection,
                        client,
                    )
                    .expect("Failed to attach X11 Window Manager");
                    let cursor = Cursor::load();
                    let image = cursor.get_image(1, Duration::ZERO);
                    wm.set_cursor(
                        &image.pixels_rgba,
                        Size::from((image.width as u16, image.height as u16)),
                        Point::from((image.xhot as u16, image.yhot as u16)),
                    )
                    .expect("Failed to set xwayland default cursor");
                    data.state.xwm = Some(wm);
                    data.state.xdisplay = Some(display);
                }
                XWaylandEvent::Exited => {
                    let _ = data.state.xwm.take();
                }
            });
            if let Err(e) = ret {
                tracing::error!(
                    "Failed to insert the XWaylandSource into the event loop: {}",
                    e
                );
            }
            xwayland
        };

        Self {
            start_time,
            display_handle,

            workspaces: Workspaces::default(),
            override_redirect_windows: Vec::new(),
            loop_signal,
            socket_name,
            child_processes: HashMap::default(),
            reap_requested: Default::default(),

            compositor_state,
            xdg_shell_state,
            xdg_activation_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            primary_selection_state,
            wlr_layer_shell_state,
            data_control_state,
            presentation_state,
            popups,
            keyboard_shortcuts_inhibit_state,

            dnd_icon: None,

            seat,
            backend_data,
            loop_handle,
            clock,
            pointer,
            cursor_status,
            seat_name,
            running: Arc::new(AtomicBool::new(true)),
            #[cfg(feature = "xwayland")]
            xwayland,
            #[cfg(feature = "xwayland")]
            xwm: None,
            #[cfg(feature = "xwayland")]
            xdisplay: None,
        }
    }

    fn init_wayland_listener(
        display: Display<Buddaraysh<BackendData>>,
        event_loop: &mut EventLoop<CalloopData<BackendData>>,
    ) -> OsString {
        // Creates a new listening socket, automatically choosing the next available `wayland` socket name.
        let listening_socket = ListeningSocketSource::new_auto().unwrap();

        // Get the name of the listening socket.
        // Clients will connect to this socket.
        let socket_name = listening_socket.socket_name().to_os_string();

        let handle = event_loop.handle();

        event_loop
            .handle()
            .insert_source(listening_socket, move |client_stream, _, state| {
                // Inside the callback, you should insert the client into the display.
                //
                // You may also associate some data with the client when inserting the client.
                state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .unwrap();
            })
            .expect("Failed to init the wayland event source.");

        // You also need to add the display itself to the event loop, so that client events will be processed by wayland-server.
        handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| {
                    // Safety: we don't drop the display
                    unsafe {
                        display
                            .get_mut()
                            .dispatch_clients(&mut state.state)
                            .unwrap();
                    }
                    Ok(PostAction::Continue)
                },
            )
            .unwrap();

        socket_name
    }

    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(FocusTarget, Point<i32, Logical>)> {
        let output = self.workspaces.outputs().find(|o| {
            let geometry = self
                .workspaces
                .current_workspace()
                .output_geometry(o)
                .unwrap();
            geometry.contains(pos.to_i32_round())
        })?;
        let output_geo = self
            .workspaces
            .current_workspace()
            .output_geometry(output)
            .unwrap();
        let layers = layer_map_for_output(output);

        if let Some(window) =
            output
                .user_data()
                .get::<FullscreenSurface>()
                .and_then(|f| match f.get() {
                    (Some(window), Some(workspace_index))
                        if workspace_index == self.workspaces.current_workspace_index() =>
                    {
                        Some(window)
                    }
                    _ => None,
                })
        {
            return Some((window.into(), output_geo.loc));
        } else if let Some(layer) = layers
            .layer_under(WlrLayer::Overlay, pos)
            .or_else(|| layers.layer_under(WlrLayer::Top, pos))
        {
            let layer_loc = layers.layer_geometry(layer).unwrap().loc;
            return Some((layer.clone().into(), output_geo.loc + layer_loc));
        } else if let Some(or) = self
            .override_redirect_windows
            .iter()
            .find(|or| or.is_in_input_region(&pos))
        {
            let window = FocusTarget::Window(WindowElement::X11(or.clone()));
            return Some((window, output_geo.loc + or.geometry().loc));
        } else if let Some((window, location)) =
            self.workspaces.current_workspace().window_under(pos)
        {
            return Some((window.clone().into(), location));
        } else if let Some(layer) = layers
            .layer_under(WlrLayer::Bottom, pos)
            .or_else(|| layers.layer_under(WlrLayer::Background, pos))
        {
            let layer_loc = layers.layer_geometry(layer).unwrap().loc;
            return Some((layer.clone().into(), output_geo.loc + layer_loc));
        }

        None
    }
}

impl<BackendData: Backend> PrimarySelectionHandler for Buddaraysh<BackendData> {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.primary_selection_state
    }
}
delegate_primary_selection!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

impl<BackendData: Backend> DataControlHandler for Buddaraysh<BackendData> {
    fn data_control_state(
        &self,
    ) -> &smithay::wayland::selection::wlr_data_control::DataControlState {
        &self.data_control_state
    }
}
delegate_data_control!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

#[cfg(feature = "xwayland")]
impl<BackendData: Backend + 'static> XWaylandKeyboardGrabHandler for Buddaraysh<BackendData> {
    fn keyboard_focus_for_xsurface(&self, surface: &WlSurface) -> Option<FocusTarget> {
        let elem = self
            .workspaces
            .current_workspace()
            .windows()
            .find(|elem| elem.wl_surface().as_ref() == Some(surface))?;
        Some(FocusTarget::Window(elem.clone()))
    }
}
#[cfg(feature = "xwayland")]
delegate_xwayland_keyboard_grab!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

//
// Wl Data Device
//

impl<BackendData: Backend + 'static> SelectionHandler for Buddaraysh<BackendData> {
    type SelectionUserData = ();

    #[cfg(feature = "xwayland")]
    fn new_selection(
        &mut self,
        ty: SelectionTarget,
        source: Option<SelectionSource>,
        _seat: Seat<Self>,
    ) {
        use tracing::warn;

        if let Some(xwm) = self.xwm.as_mut() {
            if let Err(err) = xwm.new_selection(ty, source.map(|source| source.mime_types())) {
                warn!(?err, ?ty, "Failed to set Xwayland selection");
            }
        }
    }

    #[cfg(feature = "xwayland")]
    fn send_selection(
        &mut self,
        ty: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
        _seat: Seat<Self>,
        _user_data: &(),
    ) {
        use tracing::warn;

        if let Some(xwm) = self.xwm.as_mut() {
            if let Err(err) = xwm.send_selection(ty, mime_type, fd, self.loop_handle.clone()) {
                warn!(?err, "Failed to send primary (X11 -> Wayland)");
            }
        }
    }
}

impl<BackendData: Backend + 'static> DataDeviceHandler for Buddaraysh<BackendData> {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

delegate_pointer_gestures!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);
delegate_presentation!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);
delegate_relative_pointer!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);
