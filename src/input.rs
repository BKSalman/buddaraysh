use std::{process::Stdio, sync::atomic::Ordering};

use smithay::{
    backend::{
        input::{
            AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
            KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
        },
        session::Session,
    },
    desktop::{layer_map_for_output, space::SpaceElement, WindowSurfaceType},
    input::{
        keyboard::{keysyms as xkb, FilterResult, Keysym},
        pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent},
    },
    reexports::wayland_server::{protocol::wl_surface::WlSurface, DisplayHandle},
    utils::{Logical, Point, SERIAL_COUNTER},
    wayland::{
        pointer_constraints::{with_pointer_constraint, PointerConstraint},
        seat::WaylandFocus,
        shell::wlr_layer::Layer as WlrLayer,
    },
};
use tracing::{debug, error, info};

use crate::{
    commands::Command, state::Buddaraysh, udev::UdevData, window::WindowElement, winit::WinitData,
};

impl Buddaraysh<WinitData> {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);

                self.seat.get_keyboard().unwrap().input::<(), _>(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |_, _, _| FilterResult::Forward,
                );
            }
            InputEvent::PointerMotion { .. } => {}
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output = self.space.outputs().next().unwrap();

                let output_geo = self.space.output_geometry(output).unwrap();

                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.seat.get_pointer().unwrap();

                let under = self.surface_under(pos);

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let keyboard = self.seat.get_keyboard().unwrap();

                let serial = SERIAL_COUNTER.next_serial();

                let button = event.button_code();

                let button_state = event.state();

                if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
                    if let Some((window, _loc)) = self
                        .space
                        .element_under(pointer.current_location())
                        .map(|(w, l)| (w.clone(), l))
                    {
                        self.space.raise_element(&window, true);
                        keyboard.set_focus(self, Some(window.into()), serial);
                        self.space.elements().for_each(|window| {
                            if let WindowElement::Wayland(window) = window {
                                window.toplevel().send_pending_configure();
                            }
                        });
                    } else {
                        self.space.elements().for_each(|window| {
                            if let WindowElement::Wayland(window) = window {
                                window.set_activated(false);
                                window.toplevel().send_pending_configure();
                            }
                        });
                        keyboard.set_focus(self, None, serial);
                    }
                };

                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();

                let horizontal_amount = event.amount(Axis::Horizontal).unwrap_or_else(|| {
                    event.amount_discrete(Axis::Horizontal).unwrap_or(0.0) * 3.0
                });
                let vertical_amount = event
                    .amount(Axis::Vertical)
                    .unwrap_or_else(|| event.amount_discrete(Axis::Vertical).unwrap_or(0.0) * 3.0);
                let horizontal_amount_discrete = event.amount_discrete(Axis::Horizontal);
                let vertical_amount_discrete = event.amount_discrete(Axis::Vertical);

                let mut frame = AxisFrame::new(event.time_msec()).source(source);
                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);
                    if let Some(discrete) = horizontal_amount_discrete {
                        frame = frame.discrete(Axis::Horizontal, discrete as i32);
                    }
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.discrete(Axis::Vertical, discrete as i32);
                    }
                }

                if source == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }

                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }
}

impl Buddaraysh<UdevData> {
    pub fn process_input_event<I: InputBackend>(
        &mut self,
        _display_handle: &DisplayHandle,
        event: InputEvent<I>,
    ) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);

                let command = self.seat.get_keyboard().unwrap().input::<Command, _>(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |_, modifiers, handle| {
                        let state = event.state();
                        let keysym = handle.modified_sym();

                        if state == KeyState::Pressed {
                            if modifiers.logo && keysym == Keysym::q {
                                return FilterResult::Intercept(Command::Spawn("kitty"));
                            } else if modifiers.logo && keysym == Keysym::d {
                                return FilterResult::Intercept(Command::Spawn(
                                    "pkill rofi || ~/.config/rofi/launcher.sh",
                                ));
                            } else if modifiers.logo && modifiers.shift && keysym == Keysym::X {
                                return FilterResult::Intercept(Command::Quit);
                            } else if (xkb::KEY_XF86Switch_VT_1..=xkb::KEY_XF86Switch_VT_12)
                                .contains(&keysym.raw())
                            {
                                // VTSwitch
                                return FilterResult::Intercept(Command::SwitchVT(
                                    (keysym.raw() - xkb::KEY_XF86Switch_VT_1 + 1) as i32,
                                ));
                            }
                        }

                        FilterResult::Forward
                    },
                );

                if let Some(command) = command {
                    match command {
                        Command::SwitchVT(vt) => {
                            info!(to = vt, "Trying to switch vt");
                            if let Err(err) = self.backend_data.session.change_vt(vt) {
                                error!(vt, "Error switching vt: {}", err);
                            }
                        }
                        Command::Spawn(program) => {
                            match std::process::Command::new("sh")
                                .arg("-c")
                                .arg(&program)
                                .stdin(Stdio::null())
                                .stdout(Stdio::null())
                                .envs(
                                    [self.socket_name.clone()]
                                        .into_iter()
                                        .map(|v| {
                                            ("WAYLAND_DISPLAY", v.to_string_lossy().to_string())
                                        })
                                        .into_iter()
                                        .chain(
                                            #[cfg(feature = "xwayland")]
                                            self.xdisplay.map(|v| ("DISPLAY", format!(":{}", v))),
                                            #[cfg(not(feature = "xwayland"))]
                                            None,
                                        ),
                                )
                                .spawn()
                            {
                                Ok(_child) => {
                                    // TODO: keep track of children processes
                                }
                                Err(e) => error!("Failed to run command: {e}"),
                            }
                            // self.children.insert(child);
                        }
                        Command::Quit => {
                            info!("Quitting.");
                            self.running.store(false, Ordering::SeqCst);
                        }
                        Command::None => {}
                    }
                }
            }
            InputEvent::PointerMotion { event, .. } => {
                let mut pointer_location = self.pointer.current_location();
                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.pointer.clone();
                let under = self.surface_under(pointer_location);

                let mut pointer_locked = false;
                let mut pointer_confined = false;
                let mut confine_region = None;
                if let Some((surface, surface_loc)) = under
                    .as_ref()
                    .and_then(|(target, l)| Some((target.wl_surface()?, l)))
                {
                    with_pointer_constraint(&surface, &pointer, |constraint| match constraint {
                        Some(constraint) if constraint.is_active() => {
                            // Constraint does not apply if not within region
                            if !constraint.region().map_or(true, |x| {
                                x.contains(pointer_location.to_i32_round() - *surface_loc)
                            }) {
                                return;
                            }
                            match &*constraint {
                                PointerConstraint::Locked(_locked) => {
                                    pointer_locked = true;
                                }
                                PointerConstraint::Confined(confine) => {
                                    pointer_confined = true;
                                    confine_region = confine.region().cloned();
                                }
                            }
                        }
                        _ => {}
                    });
                }

                pointer.relative_motion(
                    self,
                    under.clone(),
                    &RelativeMotionEvent {
                        delta: event.delta(),
                        delta_unaccel: event.delta_unaccel(),
                        utime: event.time(),
                    },
                );

                // If pointer is locked, only emit relative motion
                if pointer_locked {
                    pointer.frame(self);
                    return;
                }

                pointer_location += event.delta();

                // clamp to screen limits
                // this event is never generated by winit
                pointer_location = self.clamp_coords(pointer_location);

                let new_under = self.surface_under(pointer_location);

                // If confined, don't move pointer if it would go outside surface or region
                if pointer_confined {
                    if let Some((surface, surface_loc)) = &under {
                        if new_under.as_ref().and_then(|(under, _)| under.wl_surface())
                            != surface.wl_surface()
                        {
                            pointer.frame(self);
                            return;
                        }
                        if let Some(region) = confine_region {
                            if !region.contains(pointer_location.to_i32_round() - *surface_loc) {
                                pointer.frame(self);
                                return;
                            }
                        }
                    }
                }

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pointer_location,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);

                // If pointer is now in a constraint region, activate it
                // TODO Anywhere else pointer is moved needs to do this
                if let Some((under, surface_location)) =
                    new_under.and_then(|(target, loc)| Some((target.wl_surface()?, loc)))
                {
                    with_pointer_constraint(&under, &pointer, |constraint| match constraint {
                        Some(constraint) if !constraint.is_active() => {
                            let point = pointer_location.to_i32_round() - surface_location;
                            if constraint
                                .region()
                                .map_or(true, |region| region.contains(point))
                            {
                                constraint.activate();
                            }
                        }
                        _ => {}
                    });
                }
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();

                let max_x = self.space.outputs().fold(0, |acc, o| {
                    acc + self.space.output_geometry(o).unwrap().size.w
                });

                let max_h_output = self
                    .space
                    .outputs()
                    .max_by_key(|o| self.space.output_geometry(o).unwrap().size.h)
                    .unwrap();

                let max_y = self.space.output_geometry(max_h_output).unwrap().size.h;

                let mut pointer_location =
                    (event.x_transformed(max_x), event.y_transformed(max_y)).into();

                // clamp to screen limits
                pointer_location = self.clamp_coords(pointer_location);

                let pointer = self.pointer.clone();
                let under = self.surface_under(pointer_location);

                debug!("pointer location: {pointer_location:#?}");

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pointer_location,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let keyboard = self.seat.get_keyboard().unwrap();

                let serial = SERIAL_COUNTER.next_serial();

                let button = event.button_code();

                let button_state = event.state();

                if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
                    let output = self
                        .space
                        .output_under(self.pointer.current_location())
                        .next()
                        .cloned();

                    if let Some(output) = output.as_ref() {
                        let output_geo = self.space.output_geometry(output).unwrap();
                        // if let Some(window) = output
                        //     .user_data()
                        //     .get::<FullscreenSurface>()
                        //     .and_then(|f| f.get())
                        // {
                        //     if let Some((_, _)) = window.surface_under(
                        //         self.pointer.current_location() - output_geo.loc.to_f64(),
                        //         WindowSurfaceType::ALL,
                        //     ) {
                        //         #[cfg(feature = "xwayland")]
                        //         if let WindowElement::X11(surf) = &window {
                        //             self.xwm.as_mut().unwrap().raise_window(surf).unwrap();
                        //         }
                        //         keyboard.set_focus(self, Some(window.into()), serial);
                        //         return;
                        //     }
                        // }

                        let layers = layer_map_for_output(output);
                        if let Some(layer) = layers
                            .layer_under(WlrLayer::Overlay, self.pointer.current_location())
                            .or_else(|| {
                                layers.layer_under(WlrLayer::Top, self.pointer.current_location())
                            })
                        {
                            if layer.can_receive_keyboard_focus() {
                                if let Some((_, _)) = layer.surface_under(
                                    self.pointer.current_location()
                                        - output_geo.loc.to_f64()
                                        - layers.layer_geometry(layer).unwrap().loc.to_f64(),
                                    WindowSurfaceType::ALL,
                                ) {
                                    keyboard.set_focus(self, Some(layer.clone().into()), serial);
                                    return;
                                }
                            }
                        }
                    }

                    if let Some((window, _loc)) = self
                        .space
                        .element_under(pointer.current_location())
                        .map(|(w, l)| (w.clone(), l))
                    {
                        debug!("raising window: {window:#?}");
                        self.space.raise_element(&window, true);
                        keyboard.set_focus(self, Some(window.clone().into()), serial);
                        self.space.elements().for_each(|window| {
                            if let WindowElement::Wayland(window) = window {
                                window.toplevel().send_pending_configure();
                            }
                        });
                        #[cfg(feature = "xwayland")]
                        if let WindowElement::X11(surf) = &window {
                            self.xwm.as_mut().unwrap().raise_window(surf).unwrap();
                        }
                    } else {
                        self.space.elements().for_each(|window| {
                            window.set_activate(false);
                            if let WindowElement::Wayland(window) = window {
                                window.toplevel().send_pending_configure();
                            }
                        });
                        keyboard.set_focus(self, None, serial);

                        if let Some(output) = output.as_ref() {
                            let output_geo = self.space.output_geometry(output).unwrap();
                            let layers = layer_map_for_output(output);
                            if let Some(layer) = layers
                                .layer_under(WlrLayer::Bottom, self.pointer.current_location())
                                .or_else(|| {
                                    layers.layer_under(
                                        WlrLayer::Background,
                                        self.pointer.current_location(),
                                    )
                                })
                            {
                                if layer.can_receive_keyboard_focus() {
                                    if let Some((_, _)) = layer.surface_under(
                                        self.pointer.current_location()
                                            - output_geo.loc.to_f64()
                                            - layers.layer_geometry(layer).unwrap().loc.to_f64(),
                                        WindowSurfaceType::ALL,
                                    ) {
                                        keyboard.set_focus(
                                            self,
                                            Some(layer.clone().into()),
                                            serial,
                                        );
                                    }
                                }
                            }
                        }
                    }
                };

                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();

                let horizontal_amount = event.amount(Axis::Horizontal).unwrap_or_else(|| {
                    event.amount_discrete(Axis::Horizontal).unwrap_or(0.0) * 3.0
                });
                let vertical_amount = event
                    .amount(Axis::Vertical)
                    .unwrap_or_else(|| event.amount_discrete(Axis::Vertical).unwrap_or(0.0) * 3.0);
                let horizontal_amount_discrete = event.amount_discrete(Axis::Horizontal);
                let vertical_amount_discrete = event.amount_discrete(Axis::Vertical);

                let mut frame = AxisFrame::new(event.time_msec()).source(source);
                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);
                    if let Some(discrete) = horizontal_amount_discrete {
                        frame = frame.discrete(Axis::Horizontal, discrete as i32);
                    }
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.discrete(Axis::Vertical, discrete as i32);
                    }
                }

                if source == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }

                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }

    fn clamp_coords(&self, pos: Point<f64, Logical>) -> Point<f64, Logical> {
        if self.space.outputs().next().is_none() {
            return pos;
        }

        let (pos_x, pos_y) = pos.into();
        let max_x = self.space.outputs().fold(0, |acc, o| {
            acc + self.space.output_geometry(o).unwrap().size.w
        });
        let clamped_x = pos_x.clamp(0.0, max_x as f64);
        let max_y = self
            .space
            .outputs()
            .find(|o| {
                let geo = self.space.output_geometry(o).unwrap();
                geo.contains((clamped_x as i32, 0))
            })
            .map(|o| self.space.output_geometry(o).unwrap().size.h);

        if let Some(max_y) = max_y {
            let clamped_y = pos_y.clamp(0.0, max_y as f64);
            (clamped_x, clamped_y).into()
        } else {
            (clamped_x, pos_y).into()
        }
    }
}
