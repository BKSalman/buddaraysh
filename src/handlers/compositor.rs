use crate::{
    grabs::resize_grab, state::ClientState, window::WindowElement, Backend, Buddaraysh, CalloopData,
};
use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    delegate_compositor, delegate_shm,
    desktop::{layer_map_for_output, PopupKind, PopupManager, Space, WindowSurfaceType},
    reexports::{
        calloop::Interest,
        wayland_server::{
            protocol::{wl_buffer, wl_surface::WlSurface},
            Client, Resource,
        },
    },
    wayland::{
        buffer::BufferHandler,
        compositor::{
            add_blocker, add_pre_commit_hook, get_parent, is_sync_subsurface, with_states,
            BufferAssignment, CompositorClientState, CompositorHandler, CompositorState,
            SurfaceAttributes,
        },
        dmabuf::get_dmabuf,
        shell::{
            wlr_layer::LayerSurfaceData,
            xdg::{XdgPopupSurfaceData, XdgToplevelSurfaceData},
        },
        shm::{ShmHandler, ShmState},
    },
    xwayland::{X11Wm, XWaylandClientData},
};

use super::xdg_shell;

impl<BackendData: Backend + 'static> CompositorHandler for Buddaraysh<BackendData> {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        #[cfg(feature = "xwayland")]
        if let Some(state) = client.get_data::<XWaylandClientData>() {
            return &state.compositor_state;
        }
        if let Some(state) = client.get_data::<ClientState>() {
            return &state.compositor_state;
        }
        panic!("Unknown client data type")
    }

    fn new_surface(&mut self, surface: &WlSurface) {
        add_pre_commit_hook::<Self, _>(surface, move |state, _dh, surface| {
            let maybe_dmabuf = with_states(surface, |surface_data| {
                surface_data
                    .cached_state
                    .pending::<SurfaceAttributes>()
                    .buffer
                    .as_ref()
                    .and_then(|assignment| match assignment {
                        BufferAssignment::NewBuffer(buffer) => get_dmabuf(buffer).ok(),
                        _ => None,
                    })
            });
            if let Some(dmabuf) = maybe_dmabuf {
                if let Ok((blocker, source)) = dmabuf.generate_blocker(Interest::READ) {
                    let client = surface.client().unwrap();
                    let res = state.loop_handle.insert_source(source, move |_, _, data| {
                        data.state
                            .client_compositor_state(&client)
                            .blocker_cleared(&mut data.state, &data.display_handle);
                        Ok(())
                    });
                    if res.is_ok() {
                        add_blocker(surface, blocker);
                    }
                }
            }
        })
    }

    fn commit(&mut self, surface: &WlSurface) {
        #[cfg(feature = "xwayland")]
        X11Wm::commit_hook::<CalloopData<BackendData>>(surface);

        on_commit_buffer_handler::<Self>(surface);
        self.backend_data.early_import(surface);

        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self.space.elements().find(|w| match w {
                crate::window::WindowElement::Wayland(w) => w.toplevel().wl_surface() == &root,
                #[cfg(feature = "xwayland")]
                crate::window::WindowElement::X11(w) => false,
            }) {
                match window {
                    crate::window::WindowElement::Wayland(w) => w.on_commit(),
                    #[cfg(feature = "xwayland")]
                    crate::window::WindowElement::X11(w) => {}
                }
            }
        };

        ensure_initial_configure(surface, &self.space, &mut self.popups);

        xdg_shell::handle_commit(&mut self.popups, &self.space, surface);
        resize_grab::handle_commit(&mut self.space, surface);
    }
}

impl<BackendData: Backend + 'static> BufferHandler for Buddaraysh<BackendData> {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl<BackendData: Backend + 'static> ShmHandler for Buddaraysh<BackendData> {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

delegate_compositor!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);
delegate_shm!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

// TODO:
// #[derive(Default)]
// pub struct SurfaceData {
//     pub geometry: Option<Rectangle<i32, Logical>>,
//     pub resize_state: ResizeState,
// }

fn ensure_initial_configure(
    surface: &WlSurface,
    space: &Space<WindowElement>,
    popups: &mut PopupManager,
) {
    // TODO:
    // with_surface_tree_upward(
    //     surface,
    //     (),
    //     |_, _, _| TraversalAction::DoChildren(()),
    //     |_, states, _| {
    //         states
    //             .data_map
    //             .insert_if_missing(|| RefCell::new(SurfaceData::default()));
    //     },
    //     |_, _, _| true,
    // );

    if let Some(window) = space
        .elements()
        .find(|window| window.wl_surface().map(|s| s == *surface).unwrap_or(false))
        .cloned()
    {
        // send the initial configure if relevant
        #[cfg_attr(not(feature = "xwayland"), allow(irrefutable_let_patterns))]
        if let WindowElement::Wayland(ref toplevel) = window {
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
                toplevel.toplevel().send_configure();
            }
        }

        // TODO:
        // with_states(surface, |states| {
        //     let mut data = states
        //         .data_map
        //         .get::<RefCell<SurfaceData>>()
        //         .unwrap()
        //         .borrow_mut();

        //     // Finish resizing.
        //     if let ResizeState::WaitingForCommit(_) = data.resize_state {
        //         data.resize_state = ResizeState::NotResizing;
        //     }
        // });

        return;
    }

    if let Some(popup) = popups.find_popup(surface) {
        let popup = match popup {
            PopupKind::Xdg(ref popup) => popup,
            // Doesn't require configure
            PopupKind::InputMethod(ref _input_popup) => {
                return;
            }
        };

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
            popup.send_configure().expect("initial configure failed");
        }

        return;
    };

    if let Some(output) = space.outputs().find(|o| {
        let map = layer_map_for_output(o);
        map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
            .is_some()
    }) {
        let initial_configure_sent = with_states(surface, |states| {
            states
                .data_map
                .get::<LayerSurfaceData>()
                .unwrap()
                .lock()
                .unwrap()
                .initial_configure_sent
        });

        let mut map = layer_map_for_output(output);

        // arrange the layers before sending the initial configure
        // to respect any size the client may have sent
        map.arrange();
        // send the initial configure if relevant
        if !initial_configure_sent {
            let layer = map
                .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
                .unwrap();

            layer.layer_surface().send_configure();
        }
    };
}
