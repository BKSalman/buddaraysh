use crate::{
    focus::FocusTarget, window::WindowElement, workspace::Workspace, Backend, Buddaraysh, BTN_LEFT,
    BTN_RIGHT,
};
use smithay::{
    desktop::space::SpaceElement,
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
        GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
        GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData as PointerGrabStartData,
        MotionEvent, PointerGrab, PointerInnerHandle, RelativeMotionEvent,
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::protocol::wl_surface::WlSurface,
    },
    utils::{IsAlive, Logical, Point, Rectangle, Size},
    wayland::{compositor, shell::xdg::SurfaceCachedState},
    xwayland::xwm,
};
use std::cell::RefCell;

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct ResizeEdge: u32 {
        const TOP          = 0b0001;
        const BOTTOM       = 0b0010;
        const LEFT         = 0b0100;
        const RIGHT        = 0b1000;

        const TOP_LEFT     = Self::TOP.bits() | Self::LEFT.bits();
        const BOTTOM_LEFT  = Self::BOTTOM.bits() | Self::LEFT.bits();

        const TOP_RIGHT    = Self::TOP.bits() | Self::RIGHT.bits();
        const BOTTOM_RIGHT = Self::BOTTOM.bits() | Self::RIGHT.bits();
    }
}

impl From<xdg_toplevel::ResizeEdge> for ResizeEdge {
    #[inline]
    fn from(value: xdg_toplevel::ResizeEdge) -> Self {
        Self::from_bits(value as u32).unwrap()
    }
}

impl From<xwm::ResizeEdge> for ResizeEdge {
    #[inline]
    fn from(x: xwm::ResizeEdge) -> Self {
        match x {
            xwm::ResizeEdge::Top => ResizeEdge::TOP,
            xwm::ResizeEdge::Bottom => ResizeEdge::BOTTOM,
            xwm::ResizeEdge::Left => ResizeEdge::LEFT,
            xwm::ResizeEdge::Right => ResizeEdge::RIGHT,
            xwm::ResizeEdge::TopLeft => ResizeEdge::TOP_LEFT,
            xwm::ResizeEdge::BottomLeft => ResizeEdge::BOTTOM_LEFT,
            xwm::ResizeEdge::TopRight => ResizeEdge::TOP_RIGHT,
            xwm::ResizeEdge::BottomRight => ResizeEdge::BOTTOM_RIGHT,
        }
    }
}

pub struct ResizeSurfaceGrab<BackendData: Backend + 'static> {
    pub start_data: PointerGrabStartData<Buddaraysh<BackendData>>,
    pub window: WindowElement,

    pub edges: ResizeEdge,

    pub initial_rect: Rectangle<i32, Logical>,
    pub last_window_size: Size<i32, Logical>,
}

impl<BackendData: Backend + 'static> PointerGrab<Buddaraysh<BackendData>>
    for ResizeSurfaceGrab<BackendData>
{
    fn motion(
        &mut self,
        data: &mut Buddaraysh<BackendData>,
        handle: &mut PointerInnerHandle<'_, Buddaraysh<BackendData>>,
        _focus: Option<(FocusTarget, Point<i32, Logical>)>,
        event: &MotionEvent,
    ) {
        // While the grab is active, no client has pointer focus
        handle.motion(data, None, event);

        let mut delta = event.location - self.start_data.location;

        let mut new_window_width = self.initial_rect.size.w;
        let mut new_window_height = self.initial_rect.size.h;

        if self.edges.intersects(ResizeEdge::LEFT | ResizeEdge::RIGHT) {
            if self.edges.intersects(ResizeEdge::LEFT) {
                delta.x = -delta.x;
            }

            new_window_width = (self.initial_rect.size.w as f64 + delta.x) as i32;
        }

        if self.edges.intersects(ResizeEdge::TOP | ResizeEdge::BOTTOM) {
            if self.edges.intersects(ResizeEdge::TOP) {
                delta.y = -delta.y;
            }

            new_window_height = (self.initial_rect.size.h as f64 + delta.y) as i32;
        }

        let (min_size, max_size) = if let Some(surface) = self.window.wl_surface() {
            compositor::with_states(&surface, |states| {
                let data = states.cached_state.current::<SurfaceCachedState>();
                (data.min_size, data.max_size)
            })
        } else {
            ((0, 0).into(), (0, 0).into())
        };

        let min_width = min_size.w.max(1);
        let min_height = min_size.h.max(1);

        let max_width = (max_size.w == 0).then(i32::max_value).unwrap_or(max_size.w);
        let max_height = (max_size.h == 0).then(i32::max_value).unwrap_or(max_size.h);

        self.last_window_size = Size::from((
            new_window_width.clamp(min_width, max_width),
            new_window_height.clamp(min_height, max_height),
        ));

        if let Some(workspace) = data.workspace_for(&self.window) {
            if !workspace.is_tiled(&self.window) {
                match &self.window {
                    WindowElement::Wayland(w) => {
                        let xdg = w.toplevel();
                        xdg.with_pending_state(|state| {
                            state.states.set(xdg_toplevel::State::Resizing);
                            state.size = Some(self.last_window_size);
                        });
                        xdg.send_pending_configure();
                    }
                    #[cfg(feature = "xwayland")]
                    WindowElement::X11(x11) => {
                        x11.configure(Rectangle::from_loc_and_size(
                            x11.geometry().loc,
                            self.last_window_size,
                        ))
                        .unwrap();
                    }
                }
            }
        }
    }

    fn relative_motion(
        &mut self,
        data: &mut Buddaraysh<BackendData>,
        handle: &mut PointerInnerHandle<'_, Buddaraysh<BackendData>>,
        focus: Option<(FocusTarget, Point<i32, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut Buddaraysh<BackendData>,
        handle: &mut PointerInnerHandle<'_, Buddaraysh<BackendData>>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);

        if !handle.current_pressed().contains(&BTN_LEFT)
            && !handle.current_pressed().contains(&BTN_RIGHT)
        {
            // No more buttons are pressed, release the grab.
            handle.unset_grab(data, event.serial, event.time, true);

            // If toplevel is dead, we can't resize it, so we return early.
            if !self.window.alive() {
                return;
            }

            match &self.window {
                WindowElement::Wayland(w) => {
                    let xdg = w.toplevel();
                    xdg.with_pending_state(|state| {
                        state.states.unset(xdg_toplevel::State::Resizing);
                        state.size = Some(self.last_window_size);
                    });

                    xdg.send_pending_configure();

                    ResizeSurfaceState::with(xdg.wl_surface(), |state| {
                        *state = ResizeSurfaceState::WaitingForLastCommit {
                            edges: self.edges,
                            initial_rect: self.initial_rect,
                        };
                    });
                }
                #[cfg(feature = "xwayland")]
                WindowElement::X11(x11) => {
                    x11.configure(Rectangle::from_loc_and_size(
                        x11.geometry().loc,
                        self.last_window_size,
                    ))
                    .unwrap();

                    let Some(surface) = self.window.wl_surface() else {
                        // X11 Window got unmapped, abort
                        return;
                    };
                    ResizeSurfaceState::with(&surface, |state| {
                        *state = ResizeSurfaceState::WaitingForLastCommit {
                            edges: self.edges,
                            initial_rect: self.initial_rect,
                        };
                    });
                }
            }
        }
    }

    fn axis(
        &mut self,
        data: &mut Buddaraysh<BackendData>,
        handle: &mut PointerInnerHandle<'_, Buddaraysh<BackendData>>,
        details: AxisFrame,
    ) {
        handle.axis(data, details)
    }

    fn frame(
        &mut self,
        data: &mut Buddaraysh<BackendData>,
        handle: &mut PointerInnerHandle<'_, Buddaraysh<BackendData>>,
    ) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut Buddaraysh<BackendData>,
        handle: &mut PointerInnerHandle<'_, Buddaraysh<BackendData>>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event)
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut Buddaraysh<BackendData>,
        handle: &mut PointerInnerHandle<'_, Buddaraysh<BackendData>>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event)
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut Buddaraysh<BackendData>,
        handle: &mut PointerInnerHandle<'_, Buddaraysh<BackendData>>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event)
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut Buddaraysh<BackendData>,
        handle: &mut PointerInnerHandle<'_, Buddaraysh<BackendData>>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event)
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut Buddaraysh<BackendData>,
        handle: &mut PointerInnerHandle<'_, Buddaraysh<BackendData>>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event)
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut Buddaraysh<BackendData>,
        handle: &mut PointerInnerHandle<'_, Buddaraysh<BackendData>>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event)
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut Buddaraysh<BackendData>,
        handle: &mut PointerInnerHandle<'_, Buddaraysh<BackendData>>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event)
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut Buddaraysh<BackendData>,
        handle: &mut PointerInnerHandle<'_, Buddaraysh<BackendData>>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event)
    }

    fn start_data(&self) -> &PointerGrabStartData<Buddaraysh<BackendData>> {
        &self.start_data
    }
}

/// State of the resize operation.
///
/// It is stored inside of WlSurface,
/// and can be accessed using [`ResizeSurfaceState::with`]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub enum ResizeSurfaceState {
    #[default]
    Idle,
    Resizing {
        edges: ResizeEdge,
        /// The initial window size and location.
        initial_rect: Rectangle<i32, Logical>,
    },
    /// Resize is done, we are now waiting for last commit, to do the final move
    WaitingForLastCommit {
        edges: ResizeEdge,
        /// The initial window size and location.
        initial_rect: Rectangle<i32, Logical>,
    },
}

impl ResizeSurfaceState {
    fn with<F, T>(surface: &WlSurface, cb: F) -> T
    where
        F: FnOnce(&mut Self) -> T,
    {
        compositor::with_states(surface, |states| {
            states.data_map.insert_if_missing(RefCell::<Self>::default);
            let state = states.data_map.get::<RefCell<Self>>().unwrap();

            cb(&mut state.borrow_mut())
        })
    }

    fn commit(&mut self) -> Option<(ResizeEdge, Rectangle<i32, Logical>)> {
        match *self {
            Self::Resizing {
                edges,
                initial_rect,
            } => Some((edges, initial_rect)),
            Self::WaitingForLastCommit {
                edges,
                initial_rect,
            } => {
                // The resize is done, let's go back to idle
                *self = Self::Idle;

                Some((edges, initial_rect))
            }
            Self::Idle => None,
        }
    }
}

/// Should be called on `WlSurface::commit`
pub fn handle_commit(workspace: &mut Workspace, surface: &WlSurface) -> Option<()> {
    let window = workspace
        .windows()
        .find(|window| window.wl_surface().map(|s| s == *surface).unwrap_or(false))
        .cloned()?;

    let mut window_loc = workspace.window_location(&window)?;
    let geometry = window.geometry();

    let new_loc: Point<Option<i32>, Logical> = ResizeSurfaceState::with(surface, |state| {
        state
            .commit()
            .and_then(|(edges, initial_rect)| {
                // If the window is being resized by top or left, its location must be adjusted
                // accordingly.
                edges.intersects(ResizeEdge::TOP_LEFT).then(|| {
                    let new_x = edges
                        .intersects(ResizeEdge::LEFT)
                        .then_some(initial_rect.loc.x + (initial_rect.size.w - geometry.size.w));

                    let new_y = edges
                        .intersects(ResizeEdge::TOP)
                        .then_some(initial_rect.loc.y + (initial_rect.size.h - geometry.size.h));

                    (new_x, new_y).into()
                })
            })
            .unwrap_or_default()
    });

    if let Some(new_x) = new_loc.x {
        window_loc.x = new_x;
    }
    if let Some(new_y) = new_loc.y {
        window_loc.y = new_y;
    }

    if new_loc.x.is_some() || new_loc.y.is_some() {
        // If TOP or LEFT side of the window got resized, we have to move it
        // TODO: add tiling resizing
        if let Some(_old_location) = workspace.floating_layer.element_location(&window) {
            workspace
                .floating_layer
                .map_element(window, window_loc, false);
        } else {
            // workspace.tiling_layer.unmap_element(&window);
        }
    }

    Some(())
}
