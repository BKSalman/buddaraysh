use std::time::Duration;

use smithay::{
    backend::renderer::{
        element::{
            solid::SolidColorRenderElement, surface::WaylandSurfaceRenderElement, AsRenderElements,
        },
        ImportAll, ImportMem, Renderer, Texture,
    },
    desktop::{space::SpaceElement, utils::OutputPresentationFeedback, Window, WindowSurfaceType},
    output::Output,
    reexports::{
        wayland_protocols::wp::presentation_time::server::wp_presentation_feedback,
        wayland_server::protocol::wl_surface::WlSurface,
    },
    render_elements,
    utils::{user_data::UserDataMap, IsAlive, Logical, Physical, Point, Scale},
    wayland::{
        compositor::SurfaceData as WlSurfaceData, dmabuf::DmabufFeedback, seat::WaylandFocus,
        shell::xdg::ToplevelSurface,
    },
    xwayland::X11Surface,
};

#[derive(Debug, Clone, PartialEq)]
pub enum WindowElement {
    Wayland(Window),
    #[cfg(feature = "xwayland")]
    X11(X11Surface),
}

impl WindowElement {
    pub fn on_commit(&self) {
        match self {
            WindowElement::Wayland(window) => window.on_commit(),
            #[cfg(feature = "xwayland")]
            WindowElement::X11(window) => window.on_commit(),
        }
    }

    pub fn surface_under(
        &self,
        location: Point<f64, Logical>,
        window_type: WindowSurfaceType,
    ) -> Option<(WlSurface, Point<i32, Logical>)> {
        match self {
            WindowElement::Wayland(w) => w.surface_under(location, window_type),
            #[cfg(feature = "xwayland")]
            WindowElement::X11(w) => w
                .wl_surface()
                .and_then(|s| under_from_surface_tree(&s, location, (0, 0), window_type)),
        }
    }

    pub fn with_surfaces<F>(&self, processor: F)
    where
        F: FnMut(&WlSurface, &WlSurfaceData),
    {
        match self {
            WindowElement::Wayland(w) => w.with_surfaces(processor),
            #[cfg(feature = "xwayland")]
            WindowElement::X11(w) => {
                if let Some(surface) = w.wl_surface() {
                    let mut processor = processor;
                    with_surfaces_surface_tree(&surface, &mut processor);
                }
            }
        }
    }

    pub fn send_frame<T, F>(
        &self,
        output: &Output,
        time: T,
        throttle: Option<Duration>,
        primary_scan_out_output: F,
    ) where
        T: Into<Duration>,
        F: FnMut(&WlSurface, &WlSurfaceData) -> Option<Output> + Copy,
    {
        match self {
            WindowElement::Wayland(w) => {
                w.send_frame(output, time, throttle, primary_scan_out_output)
            }
            #[cfg(feature = "xwayland")]
            WindowElement::X11(w) => {
                if let Some(surface) = w.wl_surface() {
                    send_frames_surface_tree(
                        &surface,
                        output,
                        time,
                        throttle,
                        primary_scan_out_output,
                    );
                }
            }
        }
    }

    pub fn send_dmabuf_feedback<'a, P, F>(
        &self,
        output: &Output,
        primary_scan_out_output: P,
        select_dmabuf_feedback: F,
    ) where
        P: FnMut(&WlSurface, &WlSurfaceData) -> Option<Output> + Copy,
        F: Fn(&WlSurface, &WlSurfaceData) -> &'a DmabufFeedback + Copy,
    {
        match self {
            WindowElement::Wayland(w) => {
                w.send_dmabuf_feedback(output, primary_scan_out_output, select_dmabuf_feedback)
            }
            #[cfg(feature = "xwayland")]
            WindowElement::X11(w) => {
                if let Some(surface) = w.wl_surface() {
                    send_dmabuf_feedback_surface_tree(
                        &surface,
                        output,
                        primary_scan_out_output,
                        select_dmabuf_feedback,
                    )
                }
            }
        }
    }

    pub fn take_presentation_feedback<F1, F2>(
        &self,
        output_feedback: &mut OutputPresentationFeedback,
        primary_scan_out_output: F1,
        presentation_feedback_flags: F2,
    ) where
        F1: FnMut(&WlSurface, &WlSurfaceData) -> Option<Output> + Copy,
        F2: FnMut(&WlSurface, &WlSurfaceData) -> wp_presentation_feedback::Kind + Copy,
    {
        match self {
            WindowElement::Wayland(w) => w.take_presentation_feedback(
                output_feedback,
                primary_scan_out_output,
                presentation_feedback_flags,
            ),
            #[cfg(feature = "xwayland")]
            WindowElement::X11(w) => {
                if let Some(surface) = w.wl_surface() {
                    take_presentation_feedback_surface_tree(
                        &surface,
                        output_feedback,
                        primary_scan_out_output,
                        presentation_feedback_flags,
                    );
                }
            }
        }
    }

    #[cfg(feature = "xwayland")]
    pub fn is_x11(&self) -> bool {
        matches!(self, WindowElement::X11(_))
    }

    pub fn is_wayland(&self) -> bool {
        matches!(self, WindowElement::Wayland(_))
    }

    pub fn wl_surface(&self) -> Option<WlSurface> {
        match self {
            WindowElement::Wayland(w) => w.wl_surface(),
            #[cfg(feature = "xwayland")]
            WindowElement::X11(w) => w.wl_surface(),
        }
    }

    pub fn toplevel(&self) -> &ToplevelSurface {
        match self {
            WindowElement::Wayland(window) => window.toplevel(),
            #[cfg(feature = "xwayland")]
            WindowElement::X11(window) => window.toplevel(),
        }
    }

    pub fn toplevel_wl_surface(&self) -> &WlSurface {
        self.toplevel().wl_surface()
    }

    pub fn user_data(&self) -> &UserDataMap {
        match self {
            WindowElement::Wayland(w) => w.user_data(),
            #[cfg(feature = "xwayland")]
            WindowElement::X11(w) => w.user_data(),
        }
    }
}

impl IsAlive for WindowElement {
    fn alive(&self) -> bool {
        match self {
            WindowElement::Wayland(w) => w.alive(),
            #[cfg(feature = "xwayland")]
            WindowElement::X11(w) => w.alive(),
        }
    }
}

impl SpaceElement for WindowElement {
    fn bbox(&self) -> smithay::utils::Rectangle<i32, Logical> {
        match self {
            WindowElement::Wayland(window) => window.bbox(),
        }

        // XXX: not really needed
        // if there are server side decorations add it to bounding box
        // if self.decoration_state().is_ssd {
        //     bbox.size.h += HEADER_BAR_HEIGHT;
        // }
    }

    fn is_in_input_region(&self, point: &Point<f64, Logical>) -> bool {
        match self {
            WindowElement::Wayland(window) => window.is_in_input_region(point),
        }
    }

    fn set_activate(&self, activated: bool) {
        match self {
            WindowElement::Wayland(window) => window.set_activate(activated),
        }
    }

    fn output_enter(&self, output: &Output, overlap: smithay::utils::Rectangle<i32, Logical>) {
        match self {
            WindowElement::Wayland(window) => window.output_enter(output, overlap),
        }
    }

    fn output_leave(&self, output: &Output) {
        match self {
            WindowElement::Wayland(window) => window.output_leave(output),
        }
    }
}

render_elements!(
    pub WindowRenderElement<R> where R: ImportAll + ImportMem;
    Window=WaylandSurfaceRenderElement<R>,
    Decoration=SolidColorRenderElement,
);

impl<R> AsRenderElements<R> for WindowElement
where
    R: Renderer + ImportAll + ImportMem,
    <R as Renderer>::TextureId: Texture + 'static,
{
    type RenderElement = WindowRenderElement<R>;

    fn render_elements<C: From<Self::RenderElement>>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
    ) -> Vec<C> {
        match self {
            WindowElement::Wayland(xdg) => AsRenderElements::<R>::render_elements::<
                WindowRenderElement<R>,
            >(xdg, renderer, location, scale, alpha),
            #[cfg(feature = "xwayland")]
            WindowElement::X11(x11) => AsRenderElements::<R>::render_elements::<
                WindowRenderElement<R>,
            >(x11, renderer, location, scale, alpha),
        }
        .into_iter()
        .map(C::from)
        .collect()
    }
}
