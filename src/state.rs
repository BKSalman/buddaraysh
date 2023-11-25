use std::{
    ffi::OsString,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

use smithay::{
    delegate_presentation, delegate_primary_selection,
    desktop::{PopupManager, Space, Window, WindowSurfaceType},
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
        output::OutputManagerState,
        presentation::PresentationState,
        selection::{
            data_device::DataDeviceState,
            primary_selection::{PrimarySelectionHandler, PrimarySelectionState},
        },
        shell::{wlr_layer::WlrLayerShellState, xdg::XdgShellState},
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

use crate::{Backend, CalloopData};

pub struct Buddaraysh<BackendData: Backend + 'static> {
    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,
    pub loop_handle: LoopHandle<'static, CalloopData<BackendData>>,
    pub running: Arc<AtomicBool>,
    pub clock: Clock<Monotonic>,

    pub backend_data: BackendData,

    pub space: Space<Window>,
    pub loop_signal: LoopSignal,

    // Smithay State
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub wlr_layer_shell_state: WlrLayerShellState,
    pub shm_state: ShmState,
    pub primary_selection_state: PrimarySelectionState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Buddaraysh<BackendData>>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,
    pub presentation_state: PresentationState,

    pub seat_name: String,
    pub seat: Seat<Self>,
    pub cursor_status: Arc<Mutex<CursorImageStatus>>,
    pub pointer: PointerHandle<Buddaraysh<BackendData>>,
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

        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let wlr_layer_shell_state = WlrLayerShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let primary_selection_state = PrimarySelectionState::new::<Self>(&dh);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        let popups = PopupManager::default();
        let presentation_state = PresentationState::new::<Self>(&dh, clock.id() as u32);

        // A seat is a group of keyboards, pointer and touch devices.
        // A seat typically has a pointer and maintains a keyboard focus and a pointer focus.
        let seat_name = backend_data.seat_name();
        let mut seat = seat_state.new_wl_seat(&dh, seat_name.clone());

        // Notify clients that we have a keyboard, for the sake of the example we assume that keyboard is always present.
        // You may want to track keyboard hot-plug in real compositor.
        seat.add_keyboard(Default::default(), 200, 25).unwrap();

        // Notify clients that we have a pointer (mouse)
        // Here we assume that there is always pointer plugged in
        let pointer = seat.add_pointer();

        // A space represents a two-dimensional plane. Windows and Outputs can be mapped onto it.
        //
        // Windows get a position and stacking order through mapping.
        // Outputs become views of a part of the Space and can be rendered via Space::render_output.
        let space = Space::default();

        let socket_name = Self::init_wayland_listener(display, event_loop);

        // Get the loop signal, used to stop the event loop
        let loop_signal = event_loop.get_signal();

        let cursor_status = Arc::new(Mutex::new(CursorImageStatus::default_named()));

        Self {
            start_time,
            display_handle: dh,

            space,
            loop_signal,
            socket_name,

            compositor_state,
            xdg_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            presentation_state,
            popups,

            seat,
            backend_data,
            loop_handle,
            // TODO: use this for PresentationState
            clock,
            pointer,
            cursor_status,
            seat_name,
            running: Arc::new(AtomicBool::new(true)),
            primary_selection_state,
            wlr_layer_shell_state,
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
    ) -> Option<(WlSurface, Point<i32, Logical>)> {
        self.space
            .element_under(pos)
            .and_then(|(window, location)| {
                window
                    .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(s, p)| (s, p + location))
            })
    }
}

impl<BackendData: Backend> PrimarySelectionHandler for Buddaraysh<BackendData> {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.primary_selection_state
    }
}
delegate_primary_selection!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

delegate_presentation!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
