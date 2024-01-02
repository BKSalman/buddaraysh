use crate::{
    grabs::resize_grab, shell, state::ClientState, window::WindowElement, Backend, Buddaraysh,
    CalloopData,
};
use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    delegate_compositor, delegate_shm,
    desktop::{layer_map_for_output, PopupKind, WindowSurfaceType},
    output::Output,
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

            if let Some(WindowElement::Wayland(window)) = self.window_for_surface(surface) {
                window.on_commit();
            }
        }
        let window = self
            .windows()
            .find(|window| window.wl_surface().map(|s| s == *surface).unwrap_or(false))
            .cloned();

        let outputs = self.outputs().cloned().collect::<Vec<_>>();

        self.ensure_initial_configure(surface, window.as_ref(), outputs.iter());

        shell::xdg::handle_commit(&mut self.popups, window.as_ref(), surface);

        if let Some(window) = self.window_for_surface(surface) {
            if let Some(workspace) = self.workspace_for_mut(&window) {
                resize_grab::handle_commit(workspace, surface);
            }
        }
    }
}
delegate_compositor!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

impl<BackendData: Backend + 'static> BufferHandler for Buddaraysh<BackendData> {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl<BackendData: Backend + 'static> ShmHandler for Buddaraysh<BackendData> {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

delegate_shm!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);

// #[derive(Default)]
// pub struct SurfaceData {
//     pub geometry: Option<Rectangle<i32, Logical>>,
//     pub resize_state: ResizeState,
// }

impl<BackendData: Backend> Buddaraysh<BackendData> {
    fn ensure_initial_configure<'a>(
        &mut self,
        surface: &WlSurface,
        window: Option<&'a WindowElement>,
        mut outputs: impl Iterator<Item = &'a Output>,
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

        if let Some(window) = window {
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

        if let Some(popup) = self.popups.find_popup(surface) {
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
        }

        if let Some(output) = outputs.find(|o| {
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
            let changed = map.arrange();
            // send the initial configure if relevant
            if !initial_configure_sent {
                let layer = map
                    .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
                    .unwrap();

                layer.layer_surface().send_configure();
            }
            drop(map);
            if changed {
                for workspace in self.workspaces.workspaces_mut() {
                    workspace.tile_windows();
                }
            }
        }
    }
}
