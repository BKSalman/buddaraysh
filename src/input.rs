use std::{process::Stdio, sync::atomic::Ordering, time::Instant};

use smithay::{
    backend::{
        input::{
            AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
            KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
        },
        libinput::LibinputInputBackend,
        session::Session,
    },
    desktop::{layer_map_for_output, space::SpaceElement, WindowSurfaceType},
    input::{
        keyboard::{keysyms as xkb, FilterResult, Keysym, ModifiersState},
        pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent},
    },
    reexports::{
        input::Led, wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge,
        wayland_server::DisplayHandle,
    },
    utils::{Logical, Point, Serial, SERIAL_COUNTER},
    wayland::{
        compositor,
        input_method::InputMethodSeat,
        keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitorSeat,
        pointer_constraints::{with_pointer_constraint, PointerConstraint},
        seat::WaylandFocus,
        shell::{wlr_layer::Layer as WlrLayer, xdg::XdgToplevelSurfaceData},
    },
};
use tracing::{error, info};

use crate::{
    focus::FocusTarget, shell::FullscreenSurface, state::Buddaraysh, udev::UdevData,
    window::WindowElement, winit::WinitData, Action, Backend, BTN_LEFT, BTN_RIGHT,
};

impl<BackendData: Backend> Buddaraysh<BackendData> {
    fn input_to_action(
        &mut self,
        modifiers: &ModifiersState,
        keysym: Keysym,
        state: KeyState,
    ) -> Option<Action> {
        if state == KeyState::Pressed && !self.seat.keyboard_shortcuts_inhibited() {
            if modifiers.logo && keysym == Keysym::_1 {
                return Some(Action::SwitchToWorkspace(0));
            }
            if modifiers.logo && keysym == Keysym::_2 {
                return Some(Action::SwitchToWorkspace(1));
            }
            if modifiers.logo && keysym == Keysym::_3 {
                return Some(Action::SwitchToWorkspace(2));
            }
            if modifiers.logo && keysym == Keysym::_4 {
                return Some(Action::SwitchToWorkspace(3));
            }
            if modifiers.logo && keysym == Keysym::_5 {
                return Some(Action::SwitchToWorkspace(4));
            }
            if modifiers.logo && keysym == Keysym::_6 {
                return Some(Action::SwitchToWorkspace(5));
            }
            if modifiers.logo && keysym == Keysym::_7 {
                return Some(Action::SwitchToWorkspace(6));
            }
            if modifiers.logo && keysym == Keysym::_8 {
                return Some(Action::SwitchToWorkspace(7));
            }
            if modifiers.logo && keysym == Keysym::_9 {
                return Some(Action::SwitchToWorkspace(8));
            }

            if modifiers.logo && keysym == Keysym::c {
                return Some(Action::Close);
            }

            if modifiers.logo && keysym == Keysym::q {
                return Some(Action::Spawn(String::from("kitty")));
            }

            if modifiers.logo && keysym == Keysym::d {
                return Some(Action::Spawn(String::from(
                    "pkill rofi || ~/.config/rofi/launcher.sh",
                )));
            }

            if modifiers.logo && modifiers.shift && keysym == Keysym::X {
                return Some(Action::Quit);
            }
        }

        None
    }
    fn process_common_actions(&mut self, action: Action) {
        match action {
            Action::Spawn(program) => {
                match std::process::Command::new("sh")
                    .arg("-c")
                    .arg(program)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .envs(
                        [self.socket_name.clone()]
                            .into_iter()
                            .map(|v| ("WAYLAND_DISPLAY", v.to_string_lossy().to_string()))
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
            Action::Quit => {
                info!("Quitting.");
                self.running.store(false, Ordering::SeqCst);
            }
            Action::Close => {
                let keyboard = self.seat.get_keyboard().unwrap();
                if let Some(focused_surface) = keyboard
                    .current_focus()
                    .and_then(|focused| focused.wl_surface())
                {
                    if let Some(window) = self.window_for_surface(&focused_surface) {
                        window.send_close();
                    }
                }
            }
            Action::SwitchToWorkspace(workspace_index) => {
                if let None = self.workspaces.set_current_workspace(workspace_index) {
                    error!("workspace index does not exist");
                }
                let pointer = self.pointer.clone();
                let now = Instant::now();
                let time = now.duration_since(self.start_time).as_millis() as u32;
                // this is to fix the button press being sent to the fullscreen
                // surface when switching to another workspace
                //
                // but doesn't fix when switching back to the workspace with the
                // fullscreen surface
                pointer.motion(
                    self,
                    None,
                    &MotionEvent {
                        location: pointer.current_location(),
                        serial: SERIAL_COUNTER.next_serial(),
                        time,
                    },
                );
            }
            Action::None => {}
        }
    }
}

impl Buddaraysh<WinitData> {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);

                let keyboard = self.seat.get_keyboard().unwrap();

                let action = keyboard.input::<Action, _>(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |data, modifiers, handle| {
                        let keysym = handle.modified_sym();

                        data.input_to_action(modifiers, keysym, event.state())
                            .map_or(FilterResult::Forward, |action| {
                                FilterResult::Intercept(action)
                            })
                    },
                );

                if let Some(action) = action {
                    self.process_common_actions(action);
                }
            }
            InputEvent::PointerMotion { .. } => {}
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output = self.workspaces.outputs().next().unwrap();

                let output_geo = self
                    .workspaces
                    .current_workspace()
                    .output_geometry(output)
                    .unwrap();

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
                        .workspaces
                        .current_workspace()
                        .window_under(pointer.current_location())
                        .map(|(w, l)| (w.clone(), l))
                    {
                        self.workspaces
                            .current_workspace_mut()
                            .raise_window(&window, true);
                        keyboard.set_focus(self, Some(window.into()), serial);
                        self.workspaces
                            .current_workspace()
                            .windows()
                            .for_each(|window| {
                                if let WindowElement::Wayland(window) = window {
                                    window.toplevel().send_pending_configure();
                                }
                            });
                    } else {
                        self.workspaces.current_workspace().windows().for_each(
                            |window| match window {
                                WindowElement::Wayland(window) => {
                                    window.set_activated(false);
                                    window.toplevel().send_pending_configure();
                                }
                                #[cfg(feature = "xwayland")]
                                WindowElement::X11(surface) => {
                                    let _ = surface.set_activated(false);
                                }
                            },
                        );
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
    pub fn process_input_event(
        &mut self,
        _display_handle: &DisplayHandle,
        event: InputEvent<LibinputInputBackend>,
    ) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);

                let keyboard = self.seat.get_keyboard().unwrap();

                let modifiers = keyboard.modifier_state();

                let mut leds = Led::empty();
                if modifiers.num_lock {
                    leds |= Led::NUMLOCK;
                }
                if modifiers.caps_lock {
                    leds |= Led::CAPSLOCK;
                }

                event.device().led_update(leds);

                let action = keyboard.input::<Action, _>(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |data, modifiers, handle| {
                        let keysym = handle.modified_sym();

                        if (xkb::KEY_XF86Switch_VT_1..=xkb::KEY_XF86Switch_VT_12)
                            .contains(&keysym.raw())
                        {
                            // VTSwitch
                            let vt = (keysym.raw() - xkb::KEY_XF86Switch_VT_1 + 1) as i32;
                            info!(to = vt, "Trying to switch vt");
                            if let Err(err) = data.backend_data.session.change_vt(vt) {
                                error!(vt, "Error switching vt: {}", err);
                            }

                            return FilterResult::Intercept(Action::None);
                        }

                        data.input_to_action(modifiers, keysym, event.state())
                            .map_or(FilterResult::Forward, |action| {
                                FilterResult::Intercept(action)
                            })
                    },
                );

                if let Some(action) = action {
                    self.process_common_actions(action);
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

                let max_x = self.workspaces.outputs().fold(0, |acc, o| {
                    acc + self
                        .workspaces
                        .current_workspace()
                        .output_geometry(o)
                        .unwrap()
                        .size
                        .w
                });

                let max_h_output = self
                    .workspaces
                    .outputs()
                    .max_by_key(|o| {
                        self.workspaces
                            .current_workspace()
                            .output_geometry(o)
                            .unwrap()
                            .size
                            .h
                    })
                    .unwrap();

                let max_y = self
                    .workspaces
                    .current_workspace()
                    .output_geometry(max_h_output)
                    .unwrap()
                    .size
                    .h;

                let mut pointer_location =
                    (event.x_transformed(max_x), event.y_transformed(max_y)).into();

                // clamp to screen limits
                pointer_location = self.clamp_coords(pointer_location);

                let pointer = self.pointer.clone();
                let under = self.surface_under(pointer_location);

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
                let serial = SERIAL_COUNTER.next_serial();
                self.update_keyboard_focus(serial);

                let pointer = self.pointer.clone();

                let button = event.button_code();

                let button_state = event.state();

                if ButtonState::Pressed == button_state {
                    let keyboard = self.seat.get_keyboard().unwrap();

                    let modifiers = keyboard.modifier_state();

                    if !self.seat.keyboard_shortcuts_inhibited() {
                        // TODO: make this better dear mr.future Salman xqcL
                        if modifiers.logo
                            && button == BTN_LEFT
                            && !keyboard.is_grabbed()
                            && !pointer.is_grabbed()
                        {
                            if let Some((FocusTarget::Window(window), _loc)) =
                                self.surface_under(pointer.current_location())
                            {
                                match window {
                                    WindowElement::Wayland(w) => {
                                        let seat = self.seat.clone();
                                        let toplevel = w.toplevel().clone();
                                        self.loop_handle.insert_idle(move |data| {
                                            data.state.move_request_xdg(&toplevel, &seat, serial)
                                        });
                                    }
                                    #[cfg(feature = "xwayland")]
                                    WindowElement::X11(w) => {
                                        let window = w.clone();
                                        self.loop_handle.insert_idle(move |data| {
                                            data.state.move_request_x11(&window)
                                        });
                                    }
                                }
                            }
                        }

                        if modifiers.logo
                            && button == BTN_RIGHT
                            && !keyboard.is_grabbed()
                            && !pointer.is_grabbed()
                        {
                            if let Some((FocusTarget::Window(window), _loc)) =
                                self.surface_under(pointer.current_location())
                            {
                                match window {
                                    WindowElement::Wayland(ref w) => {
                                        let seat = self.seat.clone();
                                        let toplevel = w.toplevel().clone();
                                        let pointer_location = pointer.current_location();
                                        info!("pointer locatin: {pointer_location:#?}");
                                        let window_location = self
                                            .workspaces
                                            .current_workspace()
                                            .window_location(&window)
                                            .unwrap();
                                        let geometry = window.geometry();
                                        info!("geometry: {geometry:#?}");
                                        let diff = pointer_location - window_location.to_f64();
                                        let half_width = (geometry.size.w / 2) as f64;
                                        let half_height = (geometry.size.h / 2) as f64;
                                        let edge = if diff.x > half_width && diff.y > half_height {
                                            ResizeEdge::BottomRight
                                        } else if diff.x < half_width && diff.y < half_height {
                                            ResizeEdge::TopLeft
                                        } else if diff.x > half_width && diff.y < half_height {
                                            ResizeEdge::TopRight
                                        } else if diff.x < half_width && diff.y > half_height {
                                            ResizeEdge::BottomLeft
                                        } else {
                                            ResizeEdge::None
                                        };
                                        self.loop_handle.insert_idle(move |data| {
                                            data.state
                                                .resize_request_xdg(toplevel, seat, serial, edge)
                                        });
                                    }
                                    #[cfg(feature = "xwayland")]
                                    WindowElement::X11(w) => {
                                        let window = w.clone();
                                        self.loop_handle.insert_idle(move |data| {
                                            // TODO use resize_request_x11()
                                            // data.state.move_request_x11(&window)
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

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
        if self.workspaces.outputs().next().is_none() {
            return pos;
        }

        let (pos_x, pos_y) = pos.into();
        let max_x = self.workspaces.outputs().fold(0, |acc, o| {
            acc + self
                .workspaces
                .current_workspace()
                .output_geometry(o)
                .unwrap()
                .size
                .w
        });
        let clamped_x = pos_x.clamp(0.0, max_x as f64);
        let max_y = self
            .workspaces
            .outputs()
            .find(|o| {
                let geo = self
                    .workspaces
                    .current_workspace()
                    .output_geometry(o)
                    .unwrap();
                geo.contains((clamped_x as i32, 0))
            })
            .map(|o| {
                self.workspaces
                    .current_workspace()
                    .output_geometry(o)
                    .unwrap()
                    .size
                    .h
            });

        if let Some(max_y) = max_y {
            let clamped_y = pos_y.clamp(0.0, max_y as f64);
            (clamped_x, clamped_y).into()
        } else {
            (clamped_x, pos_y).into()
        }
    }

    fn update_keyboard_focus(&mut self, serial: Serial) {
        let keyboard = self.seat.get_keyboard().unwrap();
        let input_method = self.seat.input_method();
        // change the keyboard focus unless the pointer or keyboard is grabbed
        // We test for any matching surface type here but always use the root
        // (in case of a window the toplevel) surface for the focus.
        // So for example if a user clicks on a subsurface or popup the toplevel
        // will receive the keyboard focus. Directly assigning the focus to the
        // matching surface leads to issues with clients dismissing popups and
        // subsurface menus (for example firefox-wayland).
        // see here for a discussion about that issue:
        // https://gitlab.freedesktop.org/wayland/wayland/-/issues/294
        if !self.pointer.is_grabbed() && (!keyboard.is_grabbed() || input_method.keyboard_grabbed())
        {
            let output = self
                .workspaces
                .output_under(self.pointer.current_location())
                .next()
                .cloned();
            if let Some(output) = output.as_ref() {
                let output_geo = self
                    .workspaces
                    .current_workspace()
                    .output_geometry(output)
                    .unwrap();
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
                    if let Some((_, _)) = window.surface_under(
                        self.pointer.current_location() - output_geo.loc.to_f64(),
                        WindowSurfaceType::ALL,
                    ) {
                        #[cfg(feature = "xwayland")]
                        if let WindowElement::X11(surf) = &window {
                            self.xwm.as_mut().unwrap().raise_window(surf).unwrap();
                        }
                        keyboard.set_focus(self, Some(window.into()), serial);
                        return;
                    }
                }

                let layers = layer_map_for_output(output);
                if let Some(layer) = layers
                    .layer_under(WlrLayer::Overlay, self.pointer.current_location())
                    .or_else(|| layers.layer_under(WlrLayer::Top, self.pointer.current_location()))
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

            if let Some((window, _)) = self
                .workspaces
                .current_workspace()
                .window_under(self.pointer.current_location())
                .map(|(w, p)| (w.clone(), p))
            {
                self.workspaces
                    .current_workspace_mut()
                    .raise_window(&window, true);
                keyboard.set_focus(self, Some(window.clone().into()), serial);
                #[cfg(feature = "xwayland")]
                if let WindowElement::X11(surf) = &window {
                    self.xwm.as_mut().unwrap().raise_window(surf).unwrap();
                }
                return;
            }

            if let Some(output) = output.as_ref() {
                let output_geo = self
                    .workspaces
                    .current_workspace()
                    .output_geometry(output)
                    .unwrap();
                let layers = layer_map_for_output(output);
                if let Some(layer) = layers
                    .layer_under(WlrLayer::Bottom, self.pointer.current_location())
                    .or_else(|| {
                        layers.layer_under(WlrLayer::Background, self.pointer.current_location())
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
                        }
                    }
                }
            }
        }
    }
}
