use indexmap::IndexMap;
use smithay::{
    desktop::Space,
    output::Output,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Rectangle},
};

use crate::{
    shell::{
        layout::{self, ManagedLayer, ManagedState},
        FullscreenSurface,
    },
    window::WindowElement,
};

pub struct Workspaces {
    pub sets: IndexMap<Output, WorkspaceSet>,
    pub backup_set: Option<WorkspaceSet>,
    amount: usize,
}

impl Workspaces {
    pub fn new(amount: usize) -> Workspaces {
        Workspaces {
            sets: IndexMap::new(),
            backup_set: None,
            amount,
        }
    }

    pub fn add_output(&mut self, output: &Output) {
        if self.sets.contains_key(output) {
            return;
        }

        let set = self
            .backup_set
            .take()
            .map(|mut set| {
                set.set_output(output);
                set
            })
            .unwrap_or_else(|| WorkspaceSet::new(output, self.amount));

        self.sets.insert(output.clone(), set);
    }
}

pub struct WorkspaceSet {
    pub output: Output,
    workspaces: Vec<Workspace>,
    current: usize,
}

impl WorkspaceSet {
    pub fn new(output: &Output, amount: usize) -> Self {
        Self {
            workspaces: (0..amount)
                .map(|i| {
                    let mut workspace = Workspace::new(i, output);
                    workspace.set_output(output, (0, 0));
                    workspace.refresh();

                    workspace
                })
                .collect(),
            current: 0,
            output: output.clone(),
        }
    }

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

    // pub fn output_under(
    //     &self,
    //     point: impl Into<Point<f64, Logical>>,
    // ) -> impl Iterator<Item = &Output> {
    //     let point: Point<f64, Logical> = point.into();
    //     self.workspaces
    //         .iter()
    //         .flat_map(move |w| w.output_under(point))
    // }

    pub fn workspaces_mut(&mut self) -> Vec<&mut Workspace> {
        self.workspaces.iter_mut().collect()
    }

    pub fn window_for_surface(&self, surface: &WlSurface) -> Option<WindowElement> {
        self.workspaces
            .iter()
            .flat_map(|w| w.window_for_surface(surface))
            .next()
    }

    // pub fn outputs(&self) -> impl Iterator<Item = &Output> {
    //     // should be fine to only get outputs from 1 workspace
    //     // since all outputs are added to all workspaces
    //     self.workspaces[self.current].outputs()
    // }

    pub fn current_workspace_index(&self) -> usize {
        self.current
    }

    pub fn current_workspace(&self) -> &Workspace {
        &self.workspaces[self.current]
    }

    pub fn current_workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.current]
    }

    pub fn set_output(&mut self, output: &Output) {
        for workspace in &mut self.workspaces {
            workspace.set_output(output, (0, 0));
        }
        self.output = output.clone();
    }
}

#[derive(Debug)]
pub struct Workspace {
    pub output: Output,
    pub handle: usize,
    pub tiling_layer: crate::shell::layout::TilingLayout,
    pub floating_layer: crate::shell::layout::FloatingLayout,
    pub fullscreen: Option<WindowElement>,
}

impl Workspace {
    pub fn new(handle: usize, output: &Output) -> Self {
        Self {
            handle,
            output: output.clone(),
            tiling_layer: crate::shell::layout::TilingLayout::default(),
            floating_layer: crate::shell::layout::FloatingLayout::default(),
            fullscreen: None,
        }
    }

    pub fn windows(&self) -> impl DoubleEndedIterator<Item = &WindowElement> {
        self.floating_layer
            .elements()
            .chain(self.tiling_layer.elements())
    }

    pub fn change_window_geometry(
        &mut self,
        window: &WindowElement,
        new_geo: Rectangle<i32, Logical>,
    ) {
        match window {
            WindowElement::Wayland(w) => {
                w.toplevel().with_pending_state(|state| {
                    state.size = Some(new_geo.size);
                });
            }
            WindowElement::X11(x11_surface) => {
                let _ = x11_surface.configure(new_geo);
            }
        }
    }

    pub fn output_geometry(&self, output: &Output) -> Option<Rectangle<i32, Logical>> {
        self.floating_layer
            .output_geometry(output)
            .or_else(|| self.tiling_layer.output_geometry(output))
    }

    pub fn set_output(&mut self, output: &Output, location: impl Into<Point<i32, Logical>>) {
        let location: Point<i32, Logical> = location.into();
        let old_output = self.tiling_layer.outputs().next().unwrap();
        self.tiling_layer.unmap_output(old_output);
        self.tiling_layer.map_output(output, location);
        let old_output = self.floating_layer.outputs().next().unwrap();
        self.floating_layer.unmap_output(old_output);
        self.floating_layer.map_output(output, location);

        // TODO: might need to update
        //     the locations of windows on the new output and stuff

        self.refresh();
    }

    pub fn window_for_surface(&self, surface: &WlSurface) -> Option<WindowElement> {
        self.floating_layer
            .elements()
            .chain(self.tiling_layer.elements())
            .find(|window| window.wl_surface().map(|s| s == *surface).unwrap_or(false))
            .cloned()
    }

    pub fn outputs(&self) -> impl Iterator<Item = &Output> {
        self.floating_layer.outputs()
    }

    pub fn window_under(
        &self,
        point: impl Into<Point<f64, Logical>>,
    ) -> Option<(&WindowElement, Point<i32, Logical>)> {
        let point: Point<f64, Logical> = point.into();
        self.floating_layer
            .element_under(point)
            .or_else(|| self.tiling_layer.element_under(point))
    }

    pub fn window_bbox(&self, window: &WindowElement) -> Option<Rectangle<i32, Logical>> {
        self.floating_layer
            .element_bbox(window)
            .or_else(|| self.floating_layer.element_bbox(window))
    }

    pub fn outputs_for_window(&self, window: &WindowElement) -> Vec<Output> {
        let mut outputs = self.floating_layer.outputs_for_element(window);

        if outputs.is_empty() {
            outputs.extend(self.tiling_layer.outputs_for_element(window));
        }

        outputs
    }

    pub fn map_window(&mut self, window: WindowElement, pointer_location: Point<f64, Logical>) {
        if layout::should_be_floating(&window) {
            self.floating_layer
                .map_element(window, Point::from((0, 0)), true);
        } else {
            self.tiling_layer.map_element(window);
            let output = { self.output_under(pointer_location).next().cloned().unwrap() };
            self.tile_windows(&output);
        }
    }

    pub fn unmap_window(&mut self, window: &WindowElement) -> Option<ManagedState> {
        let was_floating = self.floating_layer.unmap_element(window);
        let was_tiled = self.floating_layer.unmap_element(window);

        // TODO: store the fullscreen surface in the Workspace struct

        if was_floating {
            Some(ManagedState {
                layer: ManagedLayer::Floating,
                was_fullscreen: None,
            })
        } else if was_tiled {
            Some(ManagedState {
                layer: ManagedLayer::Tiling,
                was_fullscreen: None,
            })
        } else {
            None
        }
    }

    pub fn window_location(&self, window: &WindowElement) -> Option<Point<i32, Logical>> {
        self.floating_layer
            .element_location(window)
            .or_else(|| self.tiling_layer.element_location(window))
    }

    pub fn raise_window(&mut self, window: &WindowElement, activate: bool) {
        self.floating_layer.raise_element(window, activate);
    }

    pub fn output_under(
        &self,
        point: impl Into<Point<f64, Logical>>,
    ) -> impl Iterator<Item = &Output> {
        let point: Point<f64, Logical> = point.into();
        let mut output = self.floating_layer.output_under(point).collect::<Vec<_>>();

        if output.is_empty() {
            output.extend(self.tiling_layer.output_under(point));
        }

        output.into_iter()
    }

    pub fn refresh(&mut self) {
        self.tiling_layer.refresh();
        self.floating_layer.refresh();
    }
}
