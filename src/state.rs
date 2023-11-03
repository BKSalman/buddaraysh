use std::sync::Arc;

use crate::data;
use crate::element::WindowElement;
use smithay::delegate_output;
use smithay::desktop::PopupManager;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction};
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::{
    delegate_compositor, delegate_data_device, delegate_layer_shell, delegate_presentation,
    delegate_seat, delegate_shm, delegate_xdg_shell,
    desktop::Space,
    input::{Seat, SeatState},
    reexports::wayland_server::Display,
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        selection::data_device::DataDeviceState,
        shell::{wlr_layer::WlrLayerShellState, xdg::XdgShellState},
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

pub struct State {
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub seat_state: SeatState<Self>,
    pub data_device_state: DataDeviceState,
    pub layer_shell_state: WlrLayerShellState,
    pub space: Space<WindowElement>,

    pub seat: Seat<Self>,
    pub start_time: std::time::Instant,
    pub socket_name: std::ffi::OsString,
    pub loop_signal: smithay::reexports::calloop::LoopSignal,
    pub popups: PopupManager,
}

impl State {
    pub fn new(display: Display<State>, event_loop: &mut EventLoop<data::Data>) -> Self {
        let start_time = std::time::Instant::now();

        let display_handle = display.handle();
        let compositor_state = CompositorState::new::<State>(&display_handle);
        let mut seat_state = SeatState::new();
        let shm_state = ShmState::new::<State>(&display_handle, vec![]);

        // A seat is a group of keyboards, pointer and touch devices.
        // A seat typically has a pointer and maintains a keyboard focus and a pointer focus.
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&display_handle, "winit");

        // Notify clients that we have a keyboard, for the sake of the example we assume that keyboard is always present.
        // You may want to track keyboard hot-plug in real compositor.
        seat.add_keyboard(Default::default(), 200, 25).unwrap();

        // Notify clients that we have a pointer (mouse)
        // Here we assume that there is always pointer plugged in
        seat.add_pointer();

        let listener = ListeningSocketSource::with_name("wayland-5").unwrap();

        // Get the name of the listening socket.
        // Clients will connect to this socket.
        let socket_name = listener.socket_name().to_os_string();

        let handle = event_loop.handle();

        handle
            .insert_source(listener, move |client_stream, _, state| {
                // Insert a new client into Display with data associated with that client.
                // This starts the management of the client, the communication is over the UnixStream.
                state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .unwrap();
            })
            .expect("Failed to init the wayland event source.");

        // Add Display to event loop
        // The EventLoop can take a Generic struct which is a struct containing a file descriptor that
        // calloop monitors for producing events. This file descriptor is created from winit below.
        // We only need to read (Interest::READ) the fd, and Mode::Level will look for events every
        // time the event loop polls.
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

        // Get the loop signal, used to stop the event loop
        let loop_signal = event_loop.get_signal();

        Self {
            loop_signal,
            socket_name,
            start_time,
            compositor_state,
            xdg_shell_state: XdgShellState::new::<State>(&display_handle),
            shm_state,
            seat_state,
            data_device_state: DataDeviceState::new::<State>(&display_handle),
            seat,
            layer_shell_state: WlrLayerShellState::new::<State>(&display_handle),
            space: Space::default(),
            popups: PopupManager::default(),
        }
    }
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}
impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {
        println!("initialized");
    }

    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {
        println!("disconnected");
    }
}

// Macros used to delegate protocol handling to types in the app state.
delegate_xdg_shell!(State);
delegate_compositor!(State);
delegate_shm!(State);
delegate_seat!(State);
delegate_data_device!(State);
delegate_layer_shell!(State);
delegate_presentation!(State);
delegate_output!(State);
