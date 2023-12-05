//! wlr_layer_shell

use smithay::{
    delegate_layer_shell,
    desktop::layer_map_for_output,
    output::Output,
    reexports::wayland_server::protocol::wl_output::WlOutput,
    utils::SERIAL_COUNTER,
    wayland::shell::wlr_layer::{Layer as WlrLayer, LayerSurface, WlrLayerShellHandler},
};

use crate::{Backend, Buddaraysh};

impl<BackendData: Backend + 'static> WlrLayerShellHandler for Buddaraysh<BackendData> {
    fn shell_state(&mut self) -> &mut smithay::wayland::shell::wlr_layer::WlrLayerShellState {
        &mut self.wlr_layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: LayerSurface,
        wl_output: Option<WlOutput>,
        layer: WlrLayer,
        namespace: String,
    ) {
        let output = wl_output
            .as_ref()
            .and_then(Output::from_resource)
            .unwrap_or_else(|| self.workspaces.outputs().next().unwrap().clone());
        let layer_surface = smithay::desktop::LayerSurface::new(surface, namespace);

        {
            let mut map = layer_map_for_output(&output);
            map.map_layer(&layer_surface).unwrap();
        }

        match layer {
            WlrLayer::Top | WlrLayer::Overlay => {
                if let Some(keyboard) = self.seat.get_keyboard() {
                    keyboard.set_focus(
                        self,
                        Some(layer_surface.clone().into()),
                        SERIAL_COUNTER.next_serial(),
                    );
                }
            }
            _ => {}
        }
    }

    fn layer_destroyed(&mut self, surface: LayerSurface) {
        if let Some((mut map, layer)) = self.workspaces.outputs().find_map(|o| {
            let map = layer_map_for_output(o);
            let layer = map
                .layers()
                .find(|&layer| layer.layer_surface() == &surface)
                .cloned();
            layer.map(|layer| (map, layer))
        }) {
            map.unmap_layer(&layer);
        }
    }
}
delegate_layer_shell!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);
