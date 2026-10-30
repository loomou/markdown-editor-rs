use super::{EditorElement, PrepaintState};
use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, Style, Window,
};

impl IntoElement for EditorElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.0).into();
        style.size.height = gpui::relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _rl: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.prepaint_impl(bounds, window, cx)
    }

    #[allow(clippy::too_many_arguments)]
    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _rl: &mut Self::RequestLayoutState,
        st: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        #[cfg(feature = "bench-support")]
        let started = std::time::Instant::now();
        #[cfg(feature = "bench-support")]
        crate::interact_probe::mark_paint_start();
        md_content::media::MediaUsage::begin_frame(cx, window);
        self.paint_impl(bounds, st, window, cx);
        md_content::media::MediaUsage::commit_frame(cx, window);
        #[cfg(feature = "bench-support")]
        crate::interact_probe::record_paint(started.elapsed().as_secs_f64() * 1000.0);
    }
}
