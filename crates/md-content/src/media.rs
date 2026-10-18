use gpui::{App, RenderImage};
use std::sync::Arc;

pub(crate) fn drop_image_deferred(cx: &mut App, image: Arc<RenderImage>) {
    cx.defer(move |cx| cx.drop_image(image, None));
}

#[cfg(test)]
mod tests {
    use super::drop_image_deferred;
    use gpui::TestAppContext;
    use image::Frame;
    use smallvec::smallvec;
    use std::sync::Arc;

    #[gpui::test]
    fn evicted_media_releases_its_arc_only_after_the_deferred_flush(cx: &mut TestAppContext) {
        let image = Arc::new(gpui::RenderImage::new(smallvec![Frame::new(
            image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 255]))
        )]));
        let baseline = Arc::strong_count(&image);

        cx.update(|cx| {
            drop_image_deferred(cx, image.clone());
            assert_eq!(Arc::strong_count(&image), baseline + 1);
        });

        assert_eq!(Arc::strong_count(&image), baseline);
    }
}
