use smithay::{
    desktop::{layer_map_for_output, Space},
    output::Output,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Rectangle},
    wayland::compositor,
};

use crate::{window::WindowElement, workspace::Workspace, Backend, Buddaraysh};

#[derive(Default, Debug)]
pub struct TilingLayout {
    space: Space<WindowElement>,
    layout: Layout,
}

impl TilingLayout {
    pub fn elements(&self) -> impl DoubleEndedIterator<Item = &WindowElement> {
        self.space.elements()
    }

    pub fn outputs(&self) -> impl Iterator<Item = &Output> {
        self.space.outputs()
    }

    pub fn element_for_surface(&self, surface: &WlSurface) -> Option<WindowElement> {
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
    ) -> Option<(&WindowElement, Point<i32, Logical>)> {
        self.space.element_under(pos)
    }

    pub fn element_bbox(&self, element: &WindowElement) -> Option<Rectangle<i32, Logical>> {
        self.space.element_bbox(element)
    }

    pub fn output_geometry(&self, output: &Output) -> Option<Rectangle<i32, Logical>> {
        self.space.output_geometry(output)
    }

    pub fn outputs_for_element(&self, element: &WindowElement) -> Vec<Output> {
        self.space.outputs_for_element(element)
    }

    pub fn unmap_element(&mut self, window: &WindowElement) -> bool {
        let was_mapped = self.space.elements().any(|e| e == window);
        self.space.unmap_elem(window);

        was_mapped
    }

    pub fn output_under(
        &self,
        point: impl Into<Point<f64, Logical>>,
    ) -> impl Iterator<Item = &Output> {
        self.space.output_under(point)
    }

    pub fn element_location(&self, window: &WindowElement) -> Option<Point<i32, Logical>> {
        self.space.element_location(window)
    }

    pub fn map_element(&mut self, window: WindowElement) {
        self.space.map_element(window, Point::from((0, 0)), true);
    }

    pub fn refresh(&mut self) {
        self.space.refresh();
    }

    pub fn space(&self) -> &Space<WindowElement> {
        &self.space
    }
}

#[derive(Default, Debug)]
pub enum Layout {
    #[default]
    MasterStack,
}

impl Workspace {
    pub fn tile_windows(&mut self, output: &Output) {
        let Some(geo) = self.output_geometry(output).map(|op_geo| {
            let map = layer_map_for_output(output);
            if map.layers().peekable().peek().is_none() {
                // INFO: Sometimes the exclusive zone is some weird number that doesn't match the
                // |     output res, even when there are no layer surfaces mapped. In this case, we
                // |     just return the output geometry.
                op_geo
            } else {
                let zone = map.non_exclusive_zone();
                tracing::debug!("non_exclusive_zone is {zone:?}");
                Rectangle::from_loc_and_size(op_geo.loc + zone.loc, zone.size)
            }
        }) else {
            return;
        };

        match self.tiling_layer.layout {
            Layout::MasterStack => {
                let windows = self.windows().cloned().collect::<Vec<_>>();

                if windows.is_empty() {
                    return;
                }

                let stack_windows_count = windows.len() - 1;
                let mut windows = windows.into_iter();
                let Some(master) = windows.next() else {
                    return;
                };
                tracing::info!(
                    "{}, {}, {}",
                    stack_windows_count,
                    master.title(),
                    master.app_id()
                );

                tracing::info!(?stack_windows_count);
                if stack_windows_count > 0 {
                    // half width
                    let loc = geo.loc;
                    let size: smithay::utils::Size<i32, smithay::utils::Logical> =
                        (geo.size.w / 2, geo.size.h).into();
                    let master_geo = Rectangle::from_loc_and_size(loc, size);
                    master.change_geometry(master_geo);
                    tracing::info!("master geo: {master_geo:#?}");
                    self.tiling_layer.space.map_element(master, loc, true);

                    let stack_window_height = size.h / stack_windows_count as i32;

                    for (i, stack_window) in windows.enumerate() {
                        // half height with each window
                        let stack_window_geo = Rectangle::from_loc_and_size(
                            smithay::utils::Point::from((size.w, stack_window_height * i as i32)),
                            smithay::utils::Size::from((
                                size.w,
                                stack_window_height.max(40 /* minimum tiled window height*/),
                            )),
                        );
                        stack_window.change_geometry(stack_window_geo);
                        self.tiling_layer.space.map_element(
                            stack_window,
                            stack_window_geo.loc,
                            true,
                        );
                    }
                } else {
                    master.change_geometry(geo);
                    self.tiling_layer.space.map_element(master, geo.loc, true);
                }
            }
        }
    }

    fn update_windows(&self, output: &Output) {}
}
