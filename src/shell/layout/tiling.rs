use smithay::{
    desktop::{layer_map_for_output, Space},
    output::Output,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Rectangle},
};

use crate::{
    utils::geometry::{Global, PointExt, PointGlobalExt, PointLocalExt, RectExt, RectLocalExt},
    window::WindowMapped,
    workspace::Workspace,
    OutputExt,
};

#[derive(Default, Debug)]
pub struct TilingLayout {
    space: Space<WindowMapped>,
    layout: Layout,
}

impl TilingLayout {
    pub fn elements(&self) -> impl DoubleEndedIterator<Item = &WindowMapped> {
        self.space.elements()
    }

    pub fn outputs(&self) -> impl Iterator<Item = &Output> {
        self.space.outputs()
    }

    pub fn element_for_surface(&self, surface: &WlSurface) -> Option<WindowMapped> {
        self.space
            .elements()
            .find(|element| element.wl_surface().map(|s| s == *surface).unwrap_or(false))
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

    pub fn element_bbox(&self, element: &WindowMapped) -> Option<Rectangle<i32, Logical>> {
        self.space.element_bbox(element)
    }

    pub fn output_geometry(&self, output: &Output) -> Option<Rectangle<i32, Logical>> {
        self.space.output_geometry(output)
    }

    pub fn outputs_for_element(&self, element: &WindowMapped) -> Vec<Output> {
        self.space.outputs_for_element(element)
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

    pub fn map_element(&mut self, window: WindowMapped) {
        self.space.map_element(window, Point::from((0, 0)), true);
    }

    pub fn refresh(&mut self) {
        self.space.refresh();
    }

    pub fn space(&self) -> &Space<WindowMapped> {
        &self.space
    }

    pub fn space_mut(&mut self) -> &mut Space<WindowMapped> {
        &mut self.space
    }
}

#[derive(Default, Debug)]
pub enum Layout {
    #[default]
    MasterStack,
}

impl Workspace {
    pub fn tile_windows(&mut self) {
        // TODO: add window weights for size calculation
        let op_geo = self.output.geometry();
        let output = self.output.clone();
        let map = layer_map_for_output(&output);
        let geo = if map.layers().peekable().peek().is_none() {
            // INFO: Sometimes the exclusive zone is some weird number that doesn't match the
            // |     output res, even when there are no layer surfaces mapped. In this case, we
            // |     just return the output geometry.
            op_geo
        } else {
            let zone = map.non_exclusive_zone().as_local().to_global(&output);
            Rectangle::from_loc_and_size(zone.loc, zone.size)
        };

        match self.tiling_layer.layout {
            Layout::MasterStack => {
                let windows = self.tiling_layer.elements().cloned().collect::<Vec<_>>();

                if windows.is_empty() {
                    return;
                }

                let stack_windows_count = windows.len() - 1;
                let mut windows = windows.into_iter();
                let Some(master) = windows.next() else {
                    return;
                };

                if stack_windows_count > 0 {
                    // half width
                    let loc = geo.loc;
                    let size: smithay::utils::Size<i32, Global> =
                        (geo.size.w / 2, geo.size.h).into();
                    let master_geo = Rectangle::from_loc_and_size(loc, size);
                    master.set_geometry(master_geo);
                    master.send_configure();
                    self.tiling_layer
                        .space
                        .map_element(master, loc.as_logical(), true);

                    let stack_window_height = size.h / stack_windows_count as i32;

                    for (i, stack_window) in windows.enumerate() {
                        // half height with each window
                        let stack_window_geo = Rectangle::from_loc_and_size(
                            smithay::utils::Point::from((
                                size.w,
                                geo.loc.y + stack_window_height * i as i32,
                            )),
                            smithay::utils::Size::from((
                                size.w,
                                stack_window_height.max(40 /* minimum tiled window height*/),
                            )),
                        );
                        stack_window.set_geometry(stack_window_geo);
                        stack_window.send_configure();
                        self.tiling_layer.space.map_element(
                            stack_window,
                            stack_window_geo.loc.as_logical(),
                            true,
                        );
                    }
                } else {
                    master.set_geometry(geo);
                    master.send_configure();
                    self.tiling_layer
                        .space
                        .map_element(master, geo.loc.as_logical(), true);
                }
            }
        }
    }

    // fn update_windows(&self, output: &Output) {}
}
