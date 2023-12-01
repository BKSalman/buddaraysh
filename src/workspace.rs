use smithay::{
    desktop::Space,
    output::Output,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Rectangle},
};

use crate::window::WindowElement;

pub struct Workspaces {
    workspaces: Vec<Workspace>,
    current: usize,
}

impl Default for Workspaces {
    fn default() -> Self {
        Self {
            workspaces: vec![
                Workspace::default(),
                Workspace::default(),
                Workspace::default(),
                Workspace::default(),
                Workspace::default(),
                Workspace::default(),
                Workspace::default(),
                Workspace::default(),
                Workspace::default(),
            ],
            current: 0,
        }
    }
}

impl Workspaces {
    pub fn get(&mut self, workspace_index: usize) -> Option<&Workspace> {
        self.workspaces.get(workspace_index)
    }

    pub fn get_mut(&mut self, workspace_index: usize) -> Option<&mut Workspace> {
        self.workspaces.get_mut(workspace_index)
    }
    /// Sets the current workspace, and returns Some if the provided index exists, or None if it doesn't exist
    pub fn set_current_workspace(&mut self, workspace_index: usize) -> Option<&Workspace> {
        if let Some(workspace) = self.workspaces.get(workspace_index) {
            self.current = workspace_index;
            return Some(workspace);
        }

        None
    }

    pub fn workspaces(&self) -> Vec<&Workspace> {
        self.workspaces.iter().collect()
    }

    pub fn output_under(
        &self,
        point: impl Into<Point<f64, Logical>>,
    ) -> impl Iterator<Item = &Output> {
        let point: Point<f64, Logical> = point.into();
        self.workspaces
            .iter()
            .flat_map(move |w| w.output_under(point.clone()))
    }

    pub fn workspaces_mut(&mut self) -> Vec<&mut Workspace> {
        self.workspaces.iter_mut().collect()
    }

    pub fn window_for_surface(&self, surface: &WlSurface) -> Option<WindowElement> {
        self.workspaces
            .iter()
            .flat_map(|w| w.window_for_surface(surface))
            .next()
    }

    pub fn outputs(&self) -> impl Iterator<Item = &Output> {
        // should be fine to only get outputs from 1 workspace
        // since all outputs are added to all workspaces
        self.workspaces[self.current].outputs()
    }

    pub fn current_workspace_index(&self) -> usize {
        self.current
    }

    pub fn current_workspace(&self) -> &Workspace {
        &self.workspaces[self.current]
    }

    pub fn current_workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.current]
    }
}

#[derive(Default, PartialEq, Debug)]
pub struct Workspace {
    space: Space<WindowElement>,
}

impl Workspace {
    pub fn windows(&self) -> impl DoubleEndedIterator<Item = &WindowElement> {
        self.space.elements()
    }

    pub fn space(&self) -> &Space<WindowElement> {
        &self.space
    }

    pub fn space_mut(&mut self) -> &mut Space<WindowElement> {
        &mut self.space
    }

    pub fn output_geometry(&self, output: &Output) -> Option<Rectangle<i32, Logical>> {
        self.space.output_geometry(output)
    }

    pub fn add_window(
        &mut self,
        window: WindowElement,
        location: impl Into<Point<i32, Logical>>,
        activate: bool,
    ) {
        self.space.map_element(window, location, activate);
    }

    pub fn add_output(&mut self, output: &Output, location: impl Into<Point<i32, Logical>>) {
        self.space.map_output(output, location);
    }

    pub fn window_for_surface(&self, surface: &WlSurface) -> Option<WindowElement> {
        self.space
            .elements()
            .find(|window| window.wl_surface().map(|s| s == *surface).unwrap_or(false))
            .cloned()
    }

    pub fn outputs(&self) -> impl Iterator<Item = &Output> {
        self.space.outputs()
    }

    pub fn window_under(
        &self,
        point: impl Into<Point<f64, Logical>>,
    ) -> Option<(&WindowElement, Point<i32, Logical>)> {
        self.space.element_under(point)
    }

    pub fn window_bbox(&self, window: &WindowElement) -> Option<Rectangle<i32, Logical>> {
        self.space.element_bbox(window)
    }

    pub fn outputs_for_window(&self, window: &WindowElement) -> Vec<Output> {
        self.space.outputs_for_element(window)
    }

    pub fn map_window(
        &mut self,
        window: WindowElement,
        location: impl Into<Point<i32, Logical>>,
        activate: bool,
    ) {
        self.space.map_element(window, location, activate);
    }

    pub fn unmap_window(&mut self, window: &WindowElement) {
        self.space.unmap_elem(window);
    }

    pub fn window_location(&self, window: &WindowElement) -> Option<Point<i32, Logical>> {
        self.space.element_location(window)
    }

    pub fn raise_window(&mut self, window: &WindowElement, activate: bool) {
        self.space.raise_element(window, activate);
    }

    pub fn output_under(
        &self,
        point: impl Into<Point<f64, Logical>>,
    ) -> impl Iterator<Item = &Output> {
        self.space.output_under(point)
    }
}
