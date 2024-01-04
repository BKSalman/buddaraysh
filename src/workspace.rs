use indexmap::IndexMap;
use smithay::{
    desktop::space::SpaceElement,
    output::Output,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Rectangle},
};

use crate::{
    shell::{
        layout::{self, ManagedLayer, ManagedState},
        FullscreenSurface,
    },
    utils::geometry::{Global, Local, PointExt, PointGlobalExt, PointLocalExt, RectExt},
    window::WindowElement,
    OutputExt,
};

pub struct Workspaces {
    pub sets: IndexMap<Output, WorkspaceSet>,
    pub backup_set: Option<WorkspaceSet>,
    pub amount: usize,
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

    pub fn workspaces_mut(&mut self) -> impl Iterator<Item = &mut Workspace> {
        self.sets
            .values_mut()
            .flat_map(|set| set.workspaces.iter_mut())
    }

    pub fn workspaces(&self) -> impl Iterator<Item = &Workspace> {
        self.sets.values().flat_map(|set| set.workspaces.iter())
    }

    /// Sets the current workspace, and returns Some if the provided index exists, or None if it doesn't exist
    pub fn set_current_workspace(
        &mut self,
        output: &Output,
        workspace_index: usize,
    ) -> Option<&Workspace> {
        if let Some(workspace) = self.sets.get_mut(output).or(self.backup_set.as_mut()) {
            return workspace.set_current_workspace(workspace_index);
        }

        None
    }

    /// Sets the current workspace, and returns Some if the provided index exists, or None if it doesn't exist
    pub fn set_current_workspace_mut(
        &mut self,
        output: &Output,
        workspace_index: usize,
    ) -> Option<&mut Workspace> {
        if let Some(workspace) = self.sets.get_mut(output).or(self.backup_set.as_mut()) {
            return workspace.set_current_workspace_mut(workspace_index);
        }

        None
    }

    pub fn workspacesets_mut(&mut self) -> impl Iterator<Item = &mut WorkspaceSet> {
        self.sets.values_mut()
    }

    pub fn workspacesets(&self) -> impl Iterator<Item = &WorkspaceSet> {
        self.sets.values()
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

    /// Sets the current workspace, and returns Some if the provided index exists, or None if it doesn't exist
    pub fn set_current_workspace_mut(&mut self, workspace_index: usize) -> Option<&mut Workspace> {
        if let Some(workspace) = self.workspaces.get_mut(workspace_index) {
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

    pub fn refresh(&mut self) {
        self.workspaces[self.current].refresh()
    }

    pub fn workspace_for(&self, window: &WindowElement) -> Option<&Workspace> {
        self.workspaces
            .iter()
            .find(|w| w.windows().any(|e| e == window))
    }

    pub fn workspace_for_mut(&mut self, window: &WindowElement) -> Option<&mut Workspace> {
        self.workspaces
            .iter_mut()
            .find(|w| w.windows().any(|e| e == window))
    }
}

#[derive(Debug)]
pub struct Workspace {
    pub output: Output,
    pub handle: usize,
    pub tiling_layer: crate::shell::layout::TilingLayout,
    pub floating_layer: crate::shell::layout::FloatingLayout,
    pub fullscreen: Option<FullscreenSurface>,
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

    pub fn set_output(&mut self, output: &Output, location: impl Into<Point<i32, Logical>>) {
        let old_output = self.tiling_layer.outputs().next().cloned();
        if let Some(old_output) = old_output {
            self.tiling_layer.unmap_output(&old_output);
        }
        let old_output = self.floating_layer.outputs().next().cloned();
        if let Some(old_output) = old_output {
            self.floating_layer.unmap_output(&old_output);
        }

        let location: Point<i32, Logical> = location.into();

        self.tiling_layer.map_output(output, location);
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

    pub fn window_for_element(&self, window: &WindowElement) -> Option<WindowElement> {
        self.floating_layer
            .elements()
            .chain(self.tiling_layer.elements())
            .find(|w| *w == window)
            .cloned()
    }

    pub fn outputs(&self) -> impl Iterator<Item = &Output> {
        self.floating_layer.outputs()
    }

    pub fn window_under(
        &self,
        location: Point<f64, Global>,
    ) -> Option<(WindowElement, Point<i32, Global>)> {
        let location = location.to_local(&self.output);
        self.floating_layer
            .element_under(location.as_logical())
            .map(|(window, p)| (window.clone(), p.as_local()))
            .or_else(|| {
                self.tiling_layer
                    .element_under(location.as_logical())
                    .map(|(window, p)| (window.clone(), p.as_local()))
            })
            .map(|(w, p)| (w, p.to_global(&self.output)))
    }

    pub fn window_bbox(&self, window: &WindowElement) -> Option<Rectangle<i32, Logical>> {
        self.floating_layer
            .element_bbox(window)
            .or(self.tiling_layer.element_bbox(window))
    }

    pub fn outputs_for_window(&self, window: &WindowElement) -> Vec<Output> {
        let mut outputs = self.floating_layer.outputs_for_element(window);

        if outputs.is_empty() {
            outputs = self.tiling_layer.outputs_for_element(window);
        }

        outputs
    }

    pub fn map_window(&mut self, window: WindowElement) {
        if layout::should_be_floating(&window) {
            let output_size = self.output.geometry().size;
            let window_size = window.geometry().size;
            self.floating_layer.map_element(
                window,
                Point::from((
                    (output_size.w / 2) - (window_size.w / 2),
                    (output_size.h / 2) - (window_size.h / 2),
                )),
                true,
            );
        } else {
            self.tiling_layer.map_element(window);
        }
    }

    pub fn unmap_window(&mut self, window: &WindowElement) -> Option<ManagedState> {
        let was_floating = self.floating_layer.unmap_element(window);
        let was_tiled = self.tiling_layer.unmap_element(window);

        if was_floating || was_tiled {
            assert!(was_floating != was_tiled);
        }

        if was_floating {
            Some(ManagedState {
                layer: ManagedLayer::Floating,
                was_fullscreen: self.fullscreen.clone(),
            })
        } else if was_tiled {
            Some(ManagedState {
                layer: ManagedLayer::Tiling,
                was_fullscreen: self.fullscreen.clone(),
            })
        } else {
            None
        }
    }

    pub fn window_location(&self, window: &WindowElement) -> Option<Point<i32, Local>> {
        self.floating_layer
            .element_location(window)
            .or(self.tiling_layer.element_location(window))
            .map(Point::as_local)
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

    pub fn fullscreen_request(
        &mut self,
        window: &WindowElement,
        previously: Option<(ManagedLayer, usize)>,
    ) {
        let output = self.output.clone();
        let geometry = output.geometry();
        let original_geometry = window.geometry();

        window.set_fullscreen(true);
        window.set_ssd(false);
        window.set_geometry(geometry);
        tracing::trace!("Fullscreening: {:?}", window);
        self.fullscreen = Some(FullscreenSurface {
            window: window.clone(),
            previously,
            original_geometry,
        });
    }

    pub fn unfullscreen_request(
        &mut self,
        window: &WindowElement,
    ) -> Option<(ManagedLayer, usize)> {
        if let Some(f) = self.fullscreen.clone().filter(|f| &f.window == window) {
            window.set_fullscreen(false);
            window.set_geometry(f.original_geometry.as_global());
            window.send_configure();

            f.previously
        } else {
            None
        }
    }

    pub fn is_tiled(&self, window: &WindowElement) -> bool {
        self.tiling_layer.elements().any(|w| w == window)
    }

    pub fn toggle_floating(&mut self, window: &WindowElement) {
        // TODO: deal with maximized windows when it's added
        // if window.is_maximized(false) {
        //     self.unmaximize_request(&window.active_window());
        // }

        if self.is_tiled(window) {
            let window_location = self.window_location(window).unwrap_or(Point::from((0, 0)));
            self.tiling_layer.unmap_element(window);
            self.floating_layer
                .map_element(window.clone(), window_location, true);
        } else {
            self.floating_layer.unmap_element(window);
            self.tiling_layer.map_element(window.clone());
        }

        self.refresh();
        self.tile_windows();
    }
}
