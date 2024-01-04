use smithay::utils::Point;

use crate::{
    utils::geometry::{Local, PointExt},
    window::WindowMapped,
    workspace::Workspace,
};

impl Workspace {
    pub fn tiled_window_down(
        &self,
        window: &WindowMapped,
    ) -> Vec<(&WindowMapped, Point<i32, Local>)> {
        let window_location = self.tiling_layer.element_location(window).unwrap();
        self.tiling_layer
            .elements()
            .flat_map(|e| {
                self.tiling_layer
                    .element_location(e)
                    .map(|l| (e, l.as_local()))
            })
            .filter(|(_, l)| l.y > window_location.y)
            .collect()
    }
    pub fn tiled_window_up(
        &self,
        window: &WindowMapped,
    ) -> Vec<(&WindowMapped, Point<i32, Local>)> {
        let window_location = self.tiling_layer.element_location(window).unwrap();
        self.tiling_layer
            .elements()
            .flat_map(|e| {
                self.tiling_layer
                    .element_location(e)
                    .map(|l| (e, l.as_local()))
            })
            .filter(|(_, l)| l.y < window_location.y)
            .collect()
    }
    pub fn tiled_window_right(
        &self,
        window: &WindowMapped,
    ) -> Vec<(&WindowMapped, Point<i32, Local>)> {
        let window_location = self.tiling_layer.element_location(window).unwrap();
        self.tiling_layer
            .elements()
            .flat_map(|e| {
                self.tiling_layer
                    .element_location(e)
                    .map(|l| (e, l.as_local()))
            })
            .filter(|(_, l)| l.x > window_location.x)
            .collect()
    }
    pub fn tiled_window_left(
        &self,
        window: &WindowMapped,
    ) -> Vec<(&WindowMapped, Point<i32, Local>)> {
        let window_location = self.tiling_layer.element_location(window).unwrap();
        self.tiling_layer
            .elements()
            .flat_map(|e| {
                self.tiling_layer
                    .element_location(e)
                    .map(|l| (e, l.as_local()))
            })
            .filter(|(_, l)| l.x < window_location.x)
            .collect()
    }
}
