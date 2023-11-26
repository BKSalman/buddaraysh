use crate::{Backend, Buddaraysh};
use smithay::{
    desktop::Window,
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
        GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
        GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData as PointerGrabStartData,
        MotionEvent, PointerGrab, PointerInnerHandle, RelativeMotionEvent,
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point},
};

pub struct MoveSurfaceGrab<BackendData: Backend + 'static> {
    pub start_data: PointerGrabStartData<Buddaraysh<BackendData>>,
    pub window: Window,
    pub initial_window_location: Point<i32, Logical>,
}

impl<BackendData: Backend + 'static> PointerGrab<Buddaraysh<BackendData>>
    for MoveSurfaceGrab<BackendData>
{
    fn motion(
        &mut self,
        data: &mut Buddaraysh<BackendData>,
        handle: &mut PointerInnerHandle<'_, Buddaraysh<BackendData>>,
        _focus: Option<(WlSurface, Point<i32, Logical>)>,
        event: &MotionEvent,
    ) {
        // While the grab is active, no client has pointer focus
        handle.motion(data, None, event);

        let delta = event.location - self.start_data.location;
        let new_location = self.initial_window_location.to_f64() + delta;
        data.space
            .map_element(self.window.clone(), new_location.to_i32_round(), true);
    }

    fn relative_motion(
        &mut self,
        data: &mut Buddaraysh<BackendData>,
        handle: &mut PointerInnerHandle<'_, Buddaraysh<BackendData>>,
        focus: Option<(WlSurface, Point<i32, Logical>)>,
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

        // The button is a button code as defined in the
        // Linux kernel's linux/input-event-codes.h header file, e.g. BTN_LEFT.
        const BTN_LEFT: u32 = 0x110;

        if !handle.current_pressed().contains(&BTN_LEFT) {
            // No more buttons are pressed, release the grab.
            handle.unset_grab(data, event.serial, event.time, true);
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