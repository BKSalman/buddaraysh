mod data;
mod element;
mod handlers;
mod input;
mod state;

use std::time::Duration;

use smithay::backend::input::KeyState;
use smithay::input::keyboard::keysyms;
use smithay::reexports::wayland_server::protocol::wl_surface;
use smithay::{
    backend::{
        input::{AbsolutePositionEvent, Event, InputEvent, KeyboardKeyEvent},
        renderer::{
            damage::OutputDamageTracker, element::surface::WaylandSurfaceRenderElement,
            gles::GlesRenderer,
        },
        winit::{self, WinitEvent},
    },
    input::{keyboard::FilterResult, pointer::MotionEvent},
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::EventLoop,
        wayland_server::Display,
        winit::{dpi::LogicalSize, window::WindowBuilder},
    },
    utils::{Rectangle, Transform, SERIAL_COUNTER},
    wayland::compositor::{with_surface_tree_downward, SurfaceAttributes, TraversalAction},
};
use state::State;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }

    let mut event_loop: EventLoop<data::Data> = EventLoop::try_new()?;
    let display: Display<State> = Display::new()?;
    let display_handle = display.handle();
    let state = State::new(display, &mut event_loop);

    let mut data = data::Data {
        display_handle,
        state,
    };

    run_winit(&mut event_loop, &mut data)?;

    dbg!(&data.state.socket_name);

    std::env::set_var("WAYLAND_DISPLAY", &data.state.socket_name);

    event_loop.run(None, &mut data, move |_| {})?;

    Ok(())
}

pub fn run_winit(
    event_loop: &mut EventLoop<data::Data>,
    data: &mut data::Data,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut backend, winit) = winit::init_from_builder::<GlesRenderer>(
        WindowBuilder::new()
            .with_inner_size(LogicalSize::new(1280.0, 800.0))
            .with_title("Buddaraysh")
            .with_visible(true),
    )?;

    let mode = Mode {
        size: backend.window_size(),
        refresh: 60_000,
    };

    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Buddaraysh".into(),
            model: "Winit".into(),
        },
    );
    let _global = output.create_global::<State>(&data.display_handle);
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);

    data.state.space.map_output(&output, (0, 0));

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    event_loop
        .handle()
        .insert_source(winit, move |event, _, data| {
            let display = &mut data.display_handle;
            let mut state = &mut data.state;

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
                WinitEvent::Input(event) => match event {
                    InputEvent::Keyboard { event } => {
                        let serial = SERIAL_COUNTER.next_serial();
                        let time = Event::time_msec(&event);
                        let key_state = event.state();

                        state.seat.get_keyboard().unwrap().input::<(), _>(
                            state,
                            event.key_code(),
                            event.state(),
                            serial,
                            time,
                            |_, modifiers, keysym| {
                                if key_state == KeyState::Pressed
                                    && modifiers.alt
                                    && u32::from(keysym.modified_sym())
                                        == keysyms::KEY_q | keysyms::KEY_Q
                                {
                                    if let Err(e) = std::process::Command::new("kitty").spawn() {
                                        tracing::error!("{e}");
                                    } else {
                                        tracing::info!("successfully launched kitty");
                                    }
                                }

                                FilterResult::Forward
                            },
                        );
                    }
                    InputEvent::PointerMotionAbsolute { event, .. } => {
                        let output = state.space.outputs().next().unwrap();

                        let output_geo = state.space.output_geometry(output).unwrap();

                        let pos =
                            event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                        let serial = SERIAL_COUNTER.next_serial();

                        let pointer = state.seat.get_pointer().unwrap();

                        let under = state.surface_under(pos);

                        pointer.motion(
                            &mut state,
                            under,
                            &MotionEvent {
                                location: pos,
                                serial,
                                time: event.time_msec(),
                            },
                        );
                        pointer.frame(&mut state);
                    }
                    _ => {}
                },
                WinitEvent::Redraw => {
                    let size = backend.window_size();
                    let damage = Rectangle::from_loc_and_size((0, 0), size);

                    backend.bind().unwrap();
                    smithay::desktop::space::render_output::<
                        _,
                        WaylandSurfaceRenderElement<GlesRenderer>,
                        _,
                        _,
                    >(
                        &output,
                        backend.renderer(),
                        1.0,
                        0,
                        [&state.space],
                        &[],
                        &mut damage_tracker,
                        [0.1, 0.1, 0.1, 1.0],
                    )
                    .unwrap();
                    backend.submit(Some(&[damage])).unwrap();

                    state.space.elements().for_each(|window| {
                        window.send_frame(
                            &output,
                            state.start_time.elapsed(),
                            Some(Duration::ZERO),
                            |_, _| Some(output.clone()),
                        )
                    });

                    state.space.refresh();
                    state.popups.cleanup();
                    let _ = display.flush_clients();

                    // Ask for redraw to schedule new frame.
                    backend.window().request_redraw();
                }
                _ => (),
            }
        })?;

    Ok(())
}

pub fn send_frames_surface_tree(surface: &wl_surface::WlSurface, time: u32) {
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_surf, states, &()| {
            // the surface may not have any user_data if it is a subsurface and has not
            // yet been commited
            for callback in states
                .cached_state
                .current::<SurfaceAttributes>()
                .frame_callbacks
                .drain(..)
            {
                callback.done(time);
            }
        },
        |_, _, &()| true,
    );
}
