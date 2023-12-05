use std::{process::Stdio, sync::atomic::Ordering, time::Instant};

use smithay::{
    backend::{
        input::{
            AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, GestureBeginEvent as _,
            GestureEndEvent, GesturePinchUpdateEvent as _, GestureSwipeUpdateEvent as _,
            InputBackend, InputEvent, KeyState, KeyboardKeyEvent, PointerAxisEvent,
            PointerButtonEvent, PointerMotionEvent, ProximityState, TabletToolButtonEvent,
            TabletToolEvent, TabletToolProximityEvent,
        },
        libinput::LibinputInputBackend,
        session::Session,
    },
    desktop::{layer_map_for_output, space::SpaceElement, WindowSurfaceType},
    input::{
        keyboard::{keysyms as xkb, FilterResult, Keysym, ModifiersState},
        pointer::{
            AxisFrame, ButtonEvent, GesturePinchBeginEvent, GesturePinchEndEvent,
            GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
            GestureSwipeUpdateEvent, GrabStartData, MotionEvent, RelativeMotionEvent,
        },
    },
    reexports::{
        input::{event::tablet_tool::TipState, DeviceCapability, Led},
        wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge,
        wayland_server::DisplayHandle,
    },
    utils::{Logical, Point, Serial, SERIAL_COUNTER},
    wayland::{
        input_method::InputMethodSeat,
        keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitorSeat,
        pointer_constraints::{with_pointer_constraint, PointerConstraint},
        seat::WaylandFocus,
        shell::wlr_layer::Layer as WlrLayer,
        tablet_manager::{TabletDescriptor, TabletSeatTrait},
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
        _keysym: Keysym,
        raw_syms: &[Keysym],
        state: KeyState,
    ) -> Option<Action> {
        if state == KeyState::Pressed && !self.seat.keyboard_shortcuts_inhibited() {
            if let Some(value) = move_to_workspace(modifiers, raw_syms) {
                return value;
            }

            if let Some(value) = switch_workspace(modifiers, raw_syms) {
                return value;
            }

            if modifiers.logo
                && !modifiers.alt
                && !modifiers.shift
                && !modifiers.ctrl
                && raw_syms.contains(&Keysym::c)
            {
                return Some(Action::Close);
            }

            if modifiers.logo
                && !modifiers.alt
                && !modifiers.shift
                && !modifiers.ctrl
                && raw_syms.contains(&Keysym::q)
            {
                return Some(Action::Spawn(String::from("kitty")));
            }

            if modifiers.logo
                && !modifiers.alt
                && !modifiers.shift
                && !modifiers.ctrl
                && raw_syms.contains(&Keysym::d)
            {
                return Some(Action::Spawn(String::from(
                    "pkill rofi || ~/.config/rofi/launcher.sh",
                )));
            }

            if modifiers.logo
                && !modifiers.alt
                && !modifiers.ctrl
                && modifiers.shift
                && raw_syms.contains(&Keysym::x)
            {
                return Some(Action::Quit);
            }

            if modifiers.logo
                && !modifiers.alt
                && !modifiers.shift
                && modifiers.ctrl
                && raw_syms.contains(&Keysym::v)
            {
                return Some(Action::Spawn(String::from(
                    "pkill rofi || rofi -theme $HOME/.config/rofi/clipboard_theme -modi clipboard:~/.local/bin/cliphist-rofi -show clipboard",
                )));
            }

            if modifiers.logo
                && !modifiers.alt
                && !modifiers.shift
                && !modifiers.ctrl
                && raw_syms.contains(&Keysym::period)
            {
                return Some(Action::Spawn(String::from(
                    "pkill rofi || rofi -theme $HOME/.config/rofi/clipboard_theme -modi emoji -show emoji",
                )));
            }

            if raw_syms.contains(&Keysym::Print) {
                return Some(Action::Spawn(String::from(
                    "grimblast --freeze copysave area ~/Pictures/$(date +%Y-%m-%d_%H-%m-%s).png",
                )));
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
                        // TODO: keep track of child processes
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
                info!("switching to {workspace_index}");
                if self
                    .workspaces
                    .set_current_workspace(workspace_index)
                    .is_none()
                {
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
                pointer.frame(self);

                let target = self.surface_under(pointer.current_location());
                let keyboard = self.seat.get_keyboard().unwrap();
                let serial = SERIAL_COUNTER.next_serial();
                info!("target: {target:#?}");
                keyboard.set_focus(self, target.map(|(f, _)| f), serial);
            }
            Action::MoveToWorkspace(workspace_index) => {
                if self.workspaces.current_workspace_index() == workspace_index {
                    return;
                }

                let keyboard = self.seat.get_keyboard().unwrap();
                if let Some(window) = keyboard
                    .current_focus()
                    .and_then(|t| t.wl_surface())
                    .and_then(|s| self.window_for_surface(&s))
                {
                    let location = self
                        .workspaces
                        .current_workspace()
                        .window_location(&window)
                        .unwrap();

                    self.workspaces
                        .current_workspace_mut()
                        .unmap_window(&window);

                    if let Some(workspace) = self.workspaces.get_mut(workspace_index) {
                        workspace.map_window(window, location, true);
                    } else {
                        self.workspaces
                            .current_workspace_mut()
                            .map_window(window, location, false);
                        error!("invalid workspace index");
                    }
                }
            }
            Action::None => {}
        }
    }
}

fn switch_workspace(modifiers: &ModifiersState, raw_syms: &[Keysym]) -> Option<Option<Action>> {
    if modifiers.logo && raw_syms.contains(&Keysym::_1) {
        return Some(Some(Action::SwitchToWorkspace(0)));
    }
    if modifiers.logo && raw_syms.contains(&Keysym::_2) {
        return Some(Some(Action::SwitchToWorkspace(1)));
    }
    if modifiers.logo && raw_syms.contains(&Keysym::_3) {
        return Some(Some(Action::SwitchToWorkspace(2)));
    }
    if modifiers.logo && raw_syms.contains(&Keysym::_4) {
        return Some(Some(Action::SwitchToWorkspace(3)));
    }
    if modifiers.logo && raw_syms.contains(&Keysym::_5) {
        return Some(Some(Action::SwitchToWorkspace(4)));
    }
    if modifiers.logo && raw_syms.contains(&Keysym::_6) {
        return Some(Some(Action::SwitchToWorkspace(5)));
    }
    if modifiers.logo && raw_syms.contains(&Keysym::_7) {
        return Some(Some(Action::SwitchToWorkspace(6)));
    }
    if modifiers.logo && raw_syms.contains(&Keysym::_8) {
        return Some(Some(Action::SwitchToWorkspace(7)));
    }
    if modifiers.logo && raw_syms.contains(&Keysym::_9) {
        return Some(Some(Action::SwitchToWorkspace(8)));
    }
    None
}

fn move_to_workspace(modifiers: &ModifiersState, raw_syms: &[Keysym]) -> Option<Option<Action>> {
    if modifiers.logo
        && modifiers.shift
        && !modifiers.alt
        && !modifiers.ctrl
        && raw_syms.contains(&Keysym::_1)
    {
        return Some(Some(Action::MoveToWorkspace(0)));
    }
    if modifiers.logo
        && modifiers.shift
        && !modifiers.alt
        && !modifiers.ctrl
        && raw_syms.contains(&Keysym::_2)
    {
        return Some(Some(Action::MoveToWorkspace(1)));
    }
    if modifiers.logo
        && modifiers.shift
        && !modifiers.alt
        && !modifiers.ctrl
        && raw_syms.contains(&Keysym::_3)
    {
        return Some(Some(Action::MoveToWorkspace(2)));
    }
    if modifiers.logo
        && modifiers.shift
        && !modifiers.alt
        && !modifiers.ctrl
        && raw_syms.contains(&Keysym::_4)
    {
        return Some(Some(Action::MoveToWorkspace(3)));
    }
    if modifiers.logo
        && modifiers.shift
        && !modifiers.alt
        && !modifiers.ctrl
        && raw_syms.contains(&Keysym::_5)
    {
        return Some(Some(Action::MoveToWorkspace(4)));
    }
    if modifiers.logo
        && modifiers.shift
        && !modifiers.alt
        && !modifiers.ctrl
        && raw_syms.contains(&Keysym::_6)
    {
        return Some(Some(Action::MoveToWorkspace(5)));
    }
    if modifiers.logo
        && modifiers.shift
        && !modifiers.alt
        && !modifiers.ctrl
        && raw_syms.contains(&Keysym::_7)
    {
        return Some(Some(Action::MoveToWorkspace(6)));
    }
    if modifiers.logo
        && modifiers.shift
        && !modifiers.alt
        && !modifiers.ctrl
        && raw_syms.contains(&Keysym::_8)
    {
        return Some(Some(Action::MoveToWorkspace(7)));
    }
    if modifiers.logo
        && modifiers.shift
        && !modifiers.alt
        && !modifiers.ctrl
        && raw_syms.contains(&Keysym::_9)
    {
        return Some(Some(Action::MoveToWorkspace(8)));
    }
    None
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
                        let raw_syms = handle.raw_syms();

                        data.input_to_action(modifiers, keysym, raw_syms, event.state())
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
                        let raw_syms = handle.raw_syms();

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

                        data.input_to_action(modifiers, keysym, raw_syms, event.state())
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
                                    WindowElement::Wayland(ref w) => {
                                        let seat = self.seat.clone();
                                        let toplevel = w.toplevel().clone();
                                        let focus = self
                                            .workspaces
                                            .current_workspace()
                                            .window_location(&window)
                                            .map(|l| (FocusTarget::Window(window), l));
                                        let start_data = smithay::input::pointer::GrabStartData {
                                            focus,
                                            button,
                                            location: pointer.current_location(),
                                        };

                                        self.move_request_xdg(&toplevel, &seat, serial, start_data);
                                    }
                                    #[cfg(feature = "xwayland")]
                                    WindowElement::X11(ref w) => {
                                        let w = w.clone();
                                        let focus = self
                                            .workspaces
                                            .current_workspace()
                                            .window_location(&window)
                                            .map(|l| (FocusTarget::Window(window), l));

                                        let start_data = smithay::input::pointer::GrabStartData {
                                            focus,
                                            button,
                                            location: pointer.current_location(),
                                        };

                                        self.move_request_x11(&w, start_data);
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
                                        let window_location = self
                                            .workspaces
                                            .current_workspace()
                                            .window_location(&window)
                                            .unwrap();
                                        let geometry = window.geometry();
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

                                        let start_data = GrabStartData {
                                            focus: None,
                                            button,
                                            location: pointer.current_location(),
                                        };

                                        self.resize_request_xdg(
                                            toplevel, seat, serial, edge, start_data,
                                        );
                                    }
                                    #[cfg(feature = "xwayland")]
                                    WindowElement::X11(w) => {
                                        let window = w.clone();
                                        self.loop_handle.insert_idle(move |data| {
                                            // TODO: get resize edge
                                            // data.state.resize_request_x11(&window)
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
            InputEvent::GestureSwipeBegin { event } => {
                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.pointer.clone();
                pointer.gesture_swipe_begin(
                    self,
                    &GestureSwipeBeginEvent {
                        serial,
                        time: event.time_msec(),
                        fingers: event.fingers(),
                    },
                );
            }
            InputEvent::GestureSwipeUpdate { event } => {
                let pointer = self.pointer.clone();
                pointer.gesture_swipe_update(
                    self,
                    &GestureSwipeUpdateEvent {
                        time: event.time_msec(),
                        delta: event.delta(),
                    },
                );
            }
            InputEvent::GestureSwipeEnd { event } => {
                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.pointer.clone();
                pointer.gesture_swipe_end(
                    self,
                    &GestureSwipeEndEvent {
                        serial,
                        time: event.time_msec(),
                        cancelled: event.cancelled(),
                    },
                );
            }
            InputEvent::GesturePinchBegin { event } => {
                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.pointer.clone();
                pointer.gesture_pinch_begin(
                    self,
                    &GesturePinchBeginEvent {
                        serial,
                        time: event.time_msec(),
                        fingers: event.fingers(),
                    },
                );
            }
            InputEvent::GesturePinchUpdate { event } => {
                let pointer = self.pointer.clone();
                pointer.gesture_pinch_update(
                    self,
                    &GesturePinchUpdateEvent {
                        time: event.time_msec(),
                        delta: event.delta(),
                        scale: event.scale(),
                        rotation: event.rotation(),
                    },
                );
            }
            InputEvent::GesturePinchEnd { event } => {
                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.pointer.clone();
                pointer.gesture_pinch_end(
                    self,
                    &GesturePinchEndEvent {
                        serial,
                        time: event.time_msec(),
                        cancelled: event.cancelled(),
                    },
                );
            }
            InputEvent::TabletToolAxis { event } => {
                let tablet_seat = self.seat.tablet_seat();

                let output_geometry = self.workspaces.outputs().next().map(|o| {
                    self.workspaces
                        .current_workspace()
                        .output_geometry(o)
                        .unwrap()
                });

                if let Some(rect) = output_geometry {
                    let pointer_location =
                        event.position_transformed(rect.size) + rect.loc.to_f64();

                    let pointer = self.pointer.clone();
                    let under = self.surface_under(pointer_location);
                    let tablet = tablet_seat.get_tablet(&TabletDescriptor::from(&event.device()));
                    let tool = tablet_seat.get_tool(&event.tool());

                    pointer.motion(
                        self,
                        under.clone(),
                        &MotionEvent {
                            location: pointer_location,
                            serial: SERIAL_COUNTER.next_serial(),
                            time: 0,
                        },
                    );

                    if let (Some(tablet), Some(tool)) = (tablet, tool) {
                        if event.pressure_has_changed() {
                            tool.pressure(event.pressure());
                        }
                        if event.distance_has_changed() {
                            tool.distance(event.distance());
                        }
                        if event.tilt_has_changed() {
                            tool.tilt(event.tilt());
                        }
                        if event.slider_has_changed() {
                            tool.slider_position(event.slider_position());
                        }
                        if event.rotation_has_changed() {
                            tool.rotation(event.rotation());
                        }
                        if event.wheel_has_changed() {
                            tool.wheel(event.wheel_delta(), event.wheel_delta_discrete());
                        }

                        tool.motion(
                            pointer_location,
                            under.and_then(|(f, loc)| f.wl_surface().map(|s| (s, loc))),
                            &tablet,
                            SERIAL_COUNTER.next_serial(),
                            event.time_msec(),
                        );
                    }

                    pointer.frame(self);
                }
            }
            InputEvent::TabletToolProximity { event } => {
                let tablet_seat = self.seat.tablet_seat();

                let output_geometry = self.workspaces.outputs().next().map(|o| {
                    self.workspaces
                        .current_workspace()
                        .output_geometry(o)
                        .unwrap()
                });

                if let Some(rect) = output_geometry {
                    let tool = event.tool();
                    tablet_seat.add_tool::<Self>(&self.display_handle, &tool);

                    let pointer_location =
                        event.position_transformed(rect.size) + rect.loc.to_f64();

                    let pointer = self.pointer.clone();
                    let under = self.surface_under(pointer_location);
                    let tablet = tablet_seat.get_tablet(&TabletDescriptor::from(&event.device()));
                    let tool = tablet_seat.get_tool(&tool);

                    pointer.motion(
                        self,
                        under.clone(),
                        &MotionEvent {
                            location: pointer_location,
                            serial: SERIAL_COUNTER.next_serial(),
                            time: 0,
                        },
                    );
                    pointer.frame(self);

                    if let (Some(under), Some(tablet), Some(tool)) = (
                        under.and_then(|(f, loc)| f.wl_surface().map(|s| (s, loc))),
                        tablet,
                        tool,
                    ) {
                        match event.state() {
                            ProximityState::In => tool.proximity_in(
                                pointer_location,
                                under,
                                &tablet,
                                SERIAL_COUNTER.next_serial(),
                                event.time_msec(),
                            ),
                            ProximityState::Out => tool.proximity_out(event.time_msec()),
                        }
                    }
                }
            }
            InputEvent::TabletToolTip { event } => {
                let tool = self.seat.tablet_seat().get_tool(&event.tool());

                if let Some(tool) = tool {
                    match event.tip_state() {
                        TipState::Down => {
                            let serial = SERIAL_COUNTER.next_serial();
                            tool.tip_down(serial, event.time_msec());

                            // change the keyboard focus
                            self.update_keyboard_focus(serial);
                        }
                        TipState::Up => {
                            tool.tip_up(event.time_msec());
                        }
                    }
                }
            }
            InputEvent::TabletToolButton { event } => {
                let tool = self.seat.tablet_seat().get_tool(&event.tool());

                if let Some(tool) = tool {
                    tool.button(
                        event.button(),
                        TabletToolButtonEvent::button_state(&event),
                        SERIAL_COUNTER.next_serial(),
                        event.time_msec(),
                    );
                }
            }
            InputEvent::DeviceAdded { device } => {
                if device.has_capability(DeviceCapability::TabletTool) {
                    self.seat
                        .tablet_seat()
                        .add_tablet::<Self>(&self.display_handle, &TabletDescriptor::from(&device));
                }
            }
            InputEvent::DeviceRemoved { device } => {
                if device.has_capability(DeviceCapability::TabletTool) {
                    let tablet_seat = self.seat.tablet_seat();

                    tablet_seat.remove_tablet(&TabletDescriptor::from(&device));

                    // If there are no tablets in seat we can remove all tools
                    if tablet_seat.count_tablets() == 0 {
                        tablet_seat.clear_tools();
                    }
                }
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
