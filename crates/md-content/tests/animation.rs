use base64::Engine;
use md_content::images;
use md_content::images::Animation;
use std::sync::Arc;
use std::time::Duration;

const ANIMATED_WEBP: &str = "UklGRhgBAABXRUJQVlA4WAoAAAACAAAAAwAAAwAAQU5JTQYAAAAAAAAAAABBTk1GSgAAAAAAAAAAAAMAAAMAAGQAAAJWUDggMgAAADABAJ0BKgQABAABQCYloAADcAD+8ut///mwP/bz/wR6Af//0uD//pcH//S4P/SkAAAAQU5NRkwAAAAAAAAAAAADAAADAABkAAAAVlA4IDQAAAA0AQCdASoEAAQAAAAmJaAAA3AA/ukiH//3nz//ufP/+58/6M///yn7//I4//8jj/5QIAAAQU5NRkYAAAAAAAAAAAADAAADAABkAAAAVlA4IC4AAAA0AQCdASoEAAQAAAAmJaAAA3AA/vtV4///S4P/+lwf/9Lg/9Lg//rV5Vesq6AA";

const APNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAQAAAAECAYAAACp8Z5+AAAACGFjVEwAAAADAAAAAM7tusAAAAAaZmNUTAAAAAAAAAAEAAAABAAAAAAAAAAAAAEACgAAV3ID4QAAABVJREFUeJxj/M/A8J8BCTAhc4gTAACD0QIGBLwkRwAAABpmY1RMAAAAAQAAAAQAAAAEAAAAAAAAAAAAAQAKAADMAek1AAAAGWZkQVQAAAACeJxjZPjP8J8BCTAhc4gTAACC0gIGSlxWuQAAABpmY1RMAAAAAwAAAAQAAAAEAAAAAAAAAAAAAQAKAAAhlzrcAAAAGWZkQVQAAAAEeJxjZGD4/58BCTAhc4gTAACB0wIGOH6q4AAAAABJRU5ErkJggg==";

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

fn load(cx: &mut gpui::TestAppContext, url: &str) -> images::DecodedSource {
    let resolved = images::resolve(url, None).expect("resolve");
    let future = cx.update(|app| images::load_source(resolved, false, app));
    futures::executor::block_on(future).expect("decode")
}

fn gif(frames: usize, delay_ms: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut buf);
        encoder
            .set_repeat(image::codecs::gif::Repeat::Infinite)
            .expect("repeat");
        for index in 0..frames {
            let color = image::Rgba([index as u8, 0, 0, 255]);
            encoder
                .encode_frame(image::Frame::from_parts(
                    image::RgbaImage::from_pixel(4, 4, color),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(delay_ms, 1),
                ))
                .expect("frame");
        }
    }
    buf
}

#[gpui::test]
fn an_animated_gif_keeps_every_frame_and_its_delay(cx: &mut gpui::TestAppContext) {
    let decoded = load(cx, &data_url("image/gif", &gif(3, 100)));
    assert_eq!(decoded.bitmap().expect("a bitmap source").frame_count(), 3);
    assert_eq!((decoded.width, decoded.height), (4, 4));
    let animation = decoded
        .animation()
        .cloned()
        .expect("a multi-frame GIF is an animation");
    assert_eq!(animation.frame_count(), 3);
    assert_eq!(animation.frame_at(Duration::from_millis(50)), 0);
    assert_eq!(animation.frame_at(Duration::from_millis(150)), 1);
    assert_eq!(animation.frame_at(Duration::from_millis(250)), 2);
    assert_eq!(
        animation.frame_at(Duration::from_millis(300)),
        0,
        "an endless GIF wraps back to the first frame"
    );
}

#[gpui::test]
fn an_animated_webp_keeps_every_frame(cx: &mut gpui::TestAppContext) {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(ANIMATED_WEBP)
        .expect("webp");
    let decoded = load(cx, &data_url("image/webp", &bytes));
    assert_eq!(decoded.bitmap().expect("a bitmap source").frame_count(), 3);
    assert_eq!((decoded.width, decoded.height), (4, 4));
    let animation = decoded
        .animation()
        .cloned()
        .expect("an animated WebP is an animation");
    assert_eq!(animation.frame_count(), 3);
}

#[gpui::test]
fn an_apng_keeps_every_frame(cx: &mut gpui::TestAppContext) {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(APNG)
        .expect("apng");
    let decoded = load(cx, &data_url("image/png", &bytes));
    assert_eq!(decoded.bitmap().expect("a bitmap source").frame_count(), 3);
    assert_eq!((decoded.width, decoded.height), (4, 4));
    let animation = decoded
        .animation()
        .cloned()
        .expect("an APNG is an animation");
    assert_eq!(animation.frame_count(), 3);
}

#[gpui::test]
fn a_still_image_reports_no_animation(cx: &mut gpui::TestAppContext) {
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        4,
        4,
        image::Rgba([9, 8, 7, 255]),
    ))
    .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
    .expect("png");
    let decoded = load(cx, &data_url("image/png", &buf));
    assert!(decoded.animation().is_none());
    assert_eq!(decoded.bitmap().expect("a bitmap source").frame_count(), 1);
}

#[test]
fn a_display_raster_shares_the_animated_source() {
    let frames = vec![
        image::Frame::new(image::RgbaImage::new(8, 8)),
        image::Frame::new(image::RgbaImage::new(8, 8)),
    ];
    let source = images::animation::image_from_frames(frames);
    let animation = Arc::new(Animation::new(
        vec![Duration::from_millis(100), Duration::from_millis(100)],
        None,
    ));
    let key = images::DisplayKey {
        dest: "a.gif".to_owned(),
        width_q: 320,
        height_q: 240,
        dpr_q: images::dpr_q(1.0),
    };
    let ready = images::raster_bitmap(&source, Some(&animation), &key).expect("raster");
    assert_eq!(
        ready.bytes, 0,
        "an animated raster must not copy the frames the source already owns"
    );
    assert!(ready.shared_with_source);
    assert!(Arc::ptr_eq(&ready.image, &source));
    assert_eq!(
        ready.px_w, ready.px_h,
        "a square animation must stay square inside a non-square slot"
    );
    assert_eq!(ready.frame_index(), 0);
}
