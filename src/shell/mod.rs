use smithay::utils::{Logical, Rectangle};

use crate::window::WindowElement;

use self::layout::ManagedLayer;

pub mod layout;
mod x11;
pub mod xdg;

// fn place_new_window(
//     space: &mut Space<WindowElement>,
//     pointer_location: Point<f64, Logical>,
//     window: &WindowElement,
//     activate: bool,
// ) {
//     // place the window at a random location on same output as pointer
//     // or if there is not output in a [0;800]x[0;800] square
//     use rand::distributions::{Distribution, Uniform};

//     let output = space
//         .output_under(pointer_location)
//         .next()
//         .or_else(|| space.outputs().next())
//         .cloned();
//     let output_geometry = output
//         .and_then(|o| {
//             let geo = space.output_geometry(&o)?;
//             let map = layer_map_for_output(&o);
//             let zone = map.non_exclusive_zone();
//             Some(Rectangle::from_loc_and_size(geo.loc + zone.loc, zone.size))
//         })
//         .unwrap_or_else(|| Rectangle::from_loc_and_size((0, 0), (800, 800)));

//     // set the initial toplevel bounds
//     #[allow(irrefutable_let_patterns)]
//     if let WindowElement::Wayland(window) = window {
//         window.toplevel().with_pending_state(|state| {
//             state.bounds = Some(output_geometry.size);
//         });
//     }

//     let max_x = output_geometry.loc.x + (((output_geometry.size.w as f32) / 3.0) * 2.0) as i32;
//     let max_y = output_geometry.loc.y + (((output_geometry.size.h as f32) / 3.0) * 2.0) as i32;
//     let x_range = Uniform::new(output_geometry.loc.x, max_x);
//     let y_range = Uniform::new(output_geometry.loc.y, max_y);
//     let mut rng = rand::thread_rng();
//     let x = x_range.sample(&mut rng);
//     let y = y_range.sample(&mut rng);

//     space.map_element(window.clone(), (x, y), activate);
// }

#[derive(Debug, Clone)]
pub struct FullscreenSurface {
    pub window: WindowElement,
    pub previously: Option<(ManagedLayer, usize)>,
    pub original_geometry: Rectangle<i32, Logical>,
}

impl PartialEq for FullscreenSurface {
    fn eq(&self, other: &Self) -> bool {
        self.window == other.window
    }
}

// impl FullscreenSurface {
//     pub fn set(&self, window: WindowElement, workspace_index: usize) {
//         *self.window.borrow_mut() = Some(window);
//         *self.workspace_index.borrow_mut() = Some(workspace_index);
//     }

//     pub fn get(&self) -> (Option<WindowElement>, Option<usize>) {
//         (self.window.borrow().clone(), *self.workspace_index.borrow())
//     }

//     pub fn clear(&self) -> (Option<WindowElement>, Option<usize>) {
//         (self.window.borrow_mut().take(), self.workspace_index.take())
//     }
// }
