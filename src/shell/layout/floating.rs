use smithay::{
    desktop::Space,
    output::Output,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Rectangle},
};

use crate::{
    utils::geometry::{Local, PointLocalExt},
    window::WindowMapped,
};

#[derive(Default, Debug)]
pub struct FloatingLayout {
    space: Space<WindowMapped>,
}

impl FloatingLayout {
    pub fn elements(&self) -> impl DoubleEndedIterator<Item = &WindowMapped> {
        self.space.elements()
    }

    pub fn outputs(&self) -> impl Iterator<Item = &Output> {
        self.space.outputs()
    }

    pub fn element_for_surface(&self, surface: &WlSurface) -> Option<WindowMapped> {
        self.space
            .elements()
            .find(|window| window.wl_surface().map(|s| s == *surface).unwrap_or(false))
            .cloned()
    }

    pub fn map_output(&mut self, output: &Output, location: impl Into<Point<i32, Logical>>) {
        self.space.map_output(output, location);
    }

    pub fn unmap_output(&mut self, output: &Output) {
        self.space.unmap_output(output);
    }

    pub fn element_under(
        &self,
        pos: impl Into<Point<f64, Logical>>,
    ) -> Option<(&WindowMapped, Point<i32, Logical>)> {
        self.space.element_under(pos)
    }

    pub fn element_bbox(&self, window: &WindowMapped) -> Option<Rectangle<i32, Logical>> {
        self.space.element_bbox(window)
    }

    pub fn output_geometry(&self, output: &Output) -> Option<Rectangle<i32, Logical>> {
        self.space.output_geometry(output)
    }

    pub fn outputs_for_element(&self, window: &WindowMapped) -> Vec<Output> {
        self.space.outputs_for_element(window)
    }

    pub fn unmap_element(&mut self, window: &WindowMapped) -> bool {
        let was_unmaped = self.space.elements().any(|e| e == window);
        self.space.unmap_elem(window);

        was_unmaped
    }

    pub fn output_under(
        &self,
        point: impl Into<Point<f64, Logical>>,
    ) -> impl Iterator<Item = &Output> {
        self.space.output_under(point)
    }

    pub fn element_location(&self, window: &WindowMapped) -> Option<Point<i32, Logical>> {
        self.space.element_location(window)
    }

    pub fn raise_element(&mut self, window: &WindowMapped, activate: bool) {
        self.space.raise_element(window, activate);
    }

    pub fn map_element(
        &mut self,
        window: WindowMapped,
        location: Point<i32, Local>,
        activate: bool,
    ) {
        self.space
            .map_element(window, location.as_logical(), activate);
    }

    pub fn refresh(&mut self) {
        self.space.refresh();
    }

    pub fn space(&self) -> &Space<WindowMapped> {
        &self.space
    }
}
