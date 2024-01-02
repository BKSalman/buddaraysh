use std::{
    sync::{atomic::Ordering, Mutex},
    time::Duration,
};

use smithay::{
    backend::{
        allocator::dmabuf::Dmabuf,
        egl::EGLDevice,
        renderer::{
            damage::OutputDamageTracker, element::AsRenderElements, gles::GlesRenderer, ImportDma,
            ImportEgl,
        },
        winit::{self, WinitEvent, WinitGraphicsBackend},
    },
    delegate_dmabuf,
    desktop::space::SurfaceTree,
    input::pointer::{CursorImageAttributes, CursorImageStatus},
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::EventLoop,
        wayland_server::{protocol::wl_surface, Display},
    },
    utils::{IsAlive, Rectangle, Scale, Transform},
    wayland::{
        compositor,
        dmabuf::{
            DmabufFeedback, DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState,
            ImportNotifier,
        },
    },
};
use tracing::{error, info, warn};

use crate::{render::CustomRenderElements, Backend, Buddaraysh, CalloopData};

pub struct WinitData {
    backend: WinitGraphicsBackend<GlesRenderer>,
    damage_tracker: OutputDamageTracker,
    dmabuf_state: (DmabufState, DmabufGlobal, Option<DmabufFeedback>),
    full_redraw: u8,
    #[cfg(feature = "debug")]
    pub fps: fps_ticker::Fps,
}

impl Backend for WinitData {
    fn seat_name(&self) -> String {
        String::from("winit")
    }
    fn reset_buffers(&mut self, _output: &Output) {
        self.full_redraw = 4;
    }
    fn early_import(&mut self, _surface: &wl_surface::WlSurface) {}
}

impl DmabufHandler for Buddaraysh<WinitData> {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.backend_data.dmabuf_state.0
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        if self
            .backend_data
            .backend
            .renderer()
            .import_dmabuf(&dmabuf, None)
            .is_err()
        {
            notifier.failed();
        }
    }
}
delegate_dmabuf!(Buddaraysh<WinitData>);

pub const OUTPUT_NAME: &str = "winit";

pub fn run_winit() -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop: EventLoop<CalloopData<WinitData>> = EventLoop::try_new()?;
    let display: Display<Buddaraysh<WinitData>> = Display::new()?;
    let display_handle = display.handle();

    #[cfg_attr(not(feature = "egl"), allow(unused_mut))]
    let (mut backend, winit) = match winit::init::<GlesRenderer>() {
        Ok(ret) => ret,
        Err(err) => {
            error!("Failed to initialize Winit backend: {}", err);
            return Err(format!("Failed to initialize Winit backend: {}", err).into());
        }
    };
    let size = backend.window_size();

    let mode = Mode {
        size,
        refresh: 60_000,
    };
    let output = Output::new(
        OUTPUT_NAME.to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Smithay".into(),
            model: "Winit".into(),
        },
    );
    let _global = output.create_global::<Buddaraysh<WinitData>>(&display.handle());
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);

    let damage_tracker = OutputDamageTracker::from_output(&output);

    let render_node = EGLDevice::device_for_display(backend.renderer().egl_context().display())
        .and_then(|device| device.try_get_render_node());

    let dmabuf_default_feedback = match render_node {
        Ok(Some(node)) => {
            let dmabuf_formats = backend.renderer().dmabuf_formats().collect::<Vec<_>>();
            let dmabuf_default_feedback = DmabufFeedbackBuilder::new(node.dev_id(), dmabuf_formats)
                .build()
                .unwrap();
            Some(dmabuf_default_feedback)
        }
        Ok(None) => {
            warn!("failed to query render node, dmabuf will use v3");
            None
        }
        Err(err) => {
            warn!(?err, "failed to egl device for display, dmabuf will use v3");
            None
        }
    };

    // if we failed to build dmabuf feedback we fall back to dmabuf v3
    // Note: egl on Mesa requires either v4 or wl_drm (initialized with bind_wl_display)
    let dmabuf_state = if let Some(default_feedback) = dmabuf_default_feedback {
        let mut dmabuf_state = DmabufState::new();
        let dmabuf_global = dmabuf_state
            .create_global_with_default_feedback::<Buddaraysh<WinitData>>(
                &display.handle(),
                &default_feedback,
            );
        (dmabuf_state, dmabuf_global, Some(default_feedback))
    } else {
        let dmabuf_formats = backend.renderer().dmabuf_formats().collect::<Vec<_>>();
        let mut dmabuf_state = DmabufState::new();
        let dmabuf_global =
            dmabuf_state.create_global::<Buddaraysh<WinitData>>(&display.handle(), dmabuf_formats);
        (dmabuf_state, dmabuf_global, None)
    };

    #[cfg(feature = "egl")]
    if backend
        .renderer()
        .bind_wl_display(&display.handle())
        .is_ok()
    {
        info!("EGL hardware-acceleration enabled");
    };

    let data = WinitData {
        backend,
        damage_tracker,
        dmabuf_state,
        full_redraw: 0,
    };
    let state = Buddaraysh::new(event_loop.handle(), &mut event_loop, display, data);

    let mut data = CalloopData {
        state,
        display_handle,
    };

    let state = &mut data.state;

    state.workspaces.add_output(&output);

    _ = signal_hook::flag::register(
        signal_hook::consts::signal::SIGCHLD,
        state.reap_requested.clone(),
    )
    .map_err(|err| tracing::error!("Cannot register SIGCHLD signal handler: {:?}", err));

    event_loop
        .handle()
        .insert_source(winit, move |event, _, data| {
            let display = &mut data.display_handle;
            let state = &mut data.state;

            if state.reap_requested.swap(false, Ordering::SeqCst) {
                state
                    .child_processes
                    .retain(|_, child| child.try_wait().map_or(true, |ret| ret.is_none()));
            }

            match event {
                WinitEvent::Resized { size, .. } => {
                    output.change_current_state(
                        Some(Mode {
                            size,
                            refresh: 60_000,
                        }),
                        None,
                        None,
                        None,
                    );
                }
                WinitEvent::Input(event) => state.process_input_event(event),
                WinitEvent::Redraw => {
                    let size = state.backend_data.backend.window_size();
                    let damage = Rectangle::from_loc_and_size((0, 0), size);

                    state.backend_data.backend.bind().unwrap();

                    let renderer = state.backend_data.backend.renderer();

                    let mut cursor_guard = state.cursor_status.lock().unwrap();

                    // draw the cursor as relevant
                    // reset the cursor if the surface is no longer alive
                    let mut reset = false;
                    if let CursorImageStatus::Surface(ref surface) = *cursor_guard {
                        reset = !surface.alive();
                    }
                    if reset {
                        *cursor_guard = CursorImageStatus::default_named();
                    }
                    // let cursor_visible = !matches!(*cursor_guard, CursorImageStatus::Surface(_));

                    // pointer_element.set_status(cursor_guard.clone());

                    let scale = Scale::from(output.current_scale().fractional_scale());
                    let cursor_hotspot =
                        if let CursorImageStatus::Surface(ref surface) = *cursor_guard {
                            compositor::with_states(surface, |states| {
                                states
                                    .data_map
                                    .get::<Mutex<CursorImageAttributes>>()
                                    .unwrap()
                                    .lock()
                                    .unwrap()
                                    .hotspot
                            })
                        } else {
                            (0, 0).into()
                        };

                    let cursor_pos = state.pointer.current_location() - cursor_hotspot.to_f64();
                    let cursor_pos_scaled = cursor_pos.to_physical(scale).to_i32_round();

                    let mut custom_elements = vec![];

                    let dnd_icon = state.dnd_icon.as_ref();

                    // draw the dnd icon if any
                    if let Some(surface) = dnd_icon {
                        if surface.alive() {
                            custom_elements.extend(
                                AsRenderElements::<GlesRenderer>::render_elements(
                                    &SurfaceTree::from_surface(surface),
                                    renderer,
                                    cursor_pos_scaled,
                                    scale,
                                    1.0,
                                ),
                            );
                        }
                    }

                    for workspace in state.workspaces.workspaces_mut() {
                        // let workspace = state.workspaces.current_workspace();

                        if smithay::desktop::space::render_output::<
                            _,
                            CustomRenderElements<GlesRenderer>,
                            _,
                            _,
                        >(
                            &output,
                            state.backend_data.backend.renderer(),
                            1.0,
                            0,
                            [
                                workspace.tiling_layer.space(),
                                workspace.floating_layer.space(),
                            ],
                            &custom_elements,
                            &mut state.backend_data.damage_tracker,
                            [0.1, 0.1, 0.1, 1.0],
                        )
                        .is_ok()
                        {
                            state.backend_data.backend.submit(Some(&[damage])).unwrap();
                        }

                        workspace.windows().for_each(|window| {
                            window.send_frame(
                                &output,
                                state.start_time.elapsed(),
                                Some(Duration::ZERO),
                                |_, _| Some(output.clone()),
                            )
                        });

                        workspace.refresh();
                    }
                    state.popups.cleanup();
                    let _ = display.flush_clients();

                    // Ask for redraw to schedule new frame.
                    state.backend_data.backend.window().request_redraw();
                }
                WinitEvent::CloseRequested => {
                    state.loop_signal.stop();
                }
                _ => (),
            };
        })?;

    let mut args = std::env::args().skip(1);
    let flag = args.next();
    let arg = args.next();

    match (flag.as_deref(), arg) {
        (Some("-c") | Some("--command"), Some(command)) => {
            std::process::Command::new(command).spawn().ok();
        }
        _ => {
            std::process::Command::new("kitty").spawn().ok();
        }
    }

    event_loop.run(None, &mut data, move |_| {
        // Buddaraysh is running
    })?;

    Ok(())
}
