use gpui::{App, RenderImage};
use std::sync::Arc;

pub fn drop_image_deferred(cx: &mut App, image: Arc<RenderImage>) {
    cx.defer(move |cx| cx.drop_image(image, None));
}
