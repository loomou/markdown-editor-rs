use super::ByteLease;
use crate::pixels::{ReadyImage, from_bgra};
use std::io::{Read, Write};
use std::sync::Arc;

/// How an off-screen display raster is stored. Both are lossless and both keep
/// the channel order the pixels already have, so a round trip through either
/// one returns the same bytes a decode would have produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarmCodec {
    Deflate,
    Png,
}

impl WarmCodec {
    pub fn name(self) -> &'static str {
        match self {
            WarmCodec::Deflate => "deflate",
            WarmCodec::Png => "png",
        }
    }
}

/// The codec the caches cool with. Chosen by measuring both on the media
/// corpus rather than by assumption; the other one stays reachable so a
/// benchmark can still compare them.
pub const DEFAULT_WARM_CODEC: WarmCodec = WarmCodec::Deflate;

/// How long an entry has to stay off-screen before replacing its pixels with a
/// compressed copy is worth the encode. A fast scroll leaves an entry off
/// screen for a frame or two; compressing it then would cost an encode and an
/// inflate to save nothing. Under memory pressure this drops to zero, because
/// paying to free bytes is the whole point there.
pub const COOL_GRACE: std::time::Duration = std::time::Duration::from_millis(300);

/// A compressed entry that is not at most this share of its pixels is not worth
/// keeping. Photographs and noise land here, and keeping them hot costs less
/// than encoding and inflating them on every pass.
pub const COOL_MIN_GAIN_PERCENT: usize = 85;

/// Ceiling on what one warm entry may expand to. The dimensions come from our
/// own record, but the compressed payload is what a decoder trusts, so a
/// corrupt length must fail instead of allocating.
const MAX_WARM_RAW: usize = 96 * 1024 * 1024;

/// A display raster that is off-screen, kept as compressed bytes.
///
/// The point is to pay for the *content* rather than for its width: a 1200px
/// photograph costs several megabytes as BGRA and a fraction of that as
/// deflate, while a re-visit only has to inflate it instead of decoding the
/// source and rasterising it again.
#[derive(Clone)]
pub struct WarmImage {
    pub bytes: Arc<Vec<u8>>,
    pub codec: WarmCodec,
    pub px_w: u32,
    pub px_h: u32,
    pub dpr: f32,
    lease: Option<Arc<ByteLease>>,
}

impl std::fmt::Debug for WarmImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WarmImage")
            .field("codec", &self.codec)
            .field("px", &(self.px_w, self.px_h))
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

impl WarmImage {
    pub fn new(bytes: Vec<u8>, codec: WarmCodec, px_w: u32, px_h: u32, dpr: f32) -> Self {
        WarmImage {
            bytes: Arc::new(bytes),
            codec,
            px_w,
            px_h,
            dpr,
            lease: None,
        }
    }

    pub fn resident_bytes(&self) -> usize {
        self.bytes.len()
    }

    /// What this entry costs once inflated. The decode guard compares the
    /// decoder's output against it, so a payload that inflates to something
    /// else is rejected rather than painted.
    pub fn decoded_bytes(&self) -> usize {
        (self.px_w as usize)
            .saturating_mul(self.px_h as usize)
            .saturating_mul(4)
    }

    pub fn with_lease(mut self, lease: Arc<ByteLease>) -> Self {
        self.lease = Some(lease);
        self
    }

    pub fn lease(&self) -> Option<&Arc<ByteLease>> {
        self.lease.as_ref()
    }

    pub fn allocation_id(&self) -> Option<u64> {
        self.lease.as_ref().map(|lease| lease.id())
    }
}

/// Working bytes a cool job holds at its peak: the copy the encoder takes of
/// the pixels plus the output it builds.
pub fn estimate_encode_job(px_w: u32, px_h: u32) -> usize {
    estimate_raster(px_w, px_h)
}

/// Working bytes a thaw job holds at its peak: the inflated buffer plus the
/// decoder's own scratch.
pub fn estimate_decode_job(px_w: u32, px_h: u32) -> usize {
    estimate_raster(px_w, px_h)
}

fn estimate_raster(px_w: u32, px_h: u32) -> usize {
    (px_w as usize)
        .saturating_mul(px_h as usize)
        .saturating_mul(4)
        .saturating_mul(2)
}

pub fn encode(image: &ReadyImage, codec: WarmCodec) -> Option<Vec<u8>> {
    let raw = image.image.as_bytes(0)?;
    if raw.len() < image.bytes {
        return None;
    }
    encode_raw(&raw[..image.bytes], image.px_w, image.px_h, codec)
}

pub fn encode_raw(raw: &[u8], px_w: u32, px_h: u32, codec: WarmCodec) -> Option<Vec<u8>> {
    if px_w == 0 || px_h == 0 {
        return None;
    }
    let expected = (px_w as usize)
        .saturating_mul(px_h as usize)
        .saturating_mul(4);
    if raw.len() != expected {
        return None;
    }
    match codec {
        WarmCodec::Deflate => {
            let mut out = Vec::with_capacity(expected / 4);
            let mut encoder =
                flate2::write::DeflateEncoder::new(&mut out, flate2::Compression::fast());
            encoder.write_all(raw).ok()?;
            encoder.finish().ok()?;
            Some(out)
        }
        WarmCodec::Png => {
            let img = image::RgbaImage::from_raw(px_w, px_h, raw.to_vec())?;
            let mut out = Vec::new();
            let mut cursor = std::io::Cursor::new(&mut out);
            let encoder = image::codecs::png::PngEncoder::new_with_quality(
                &mut cursor,
                image::codecs::png::CompressionType::Fast,
                image::codecs::png::FilterType::Adaptive,
            );
            image::ImageEncoder::write_image(
                encoder,
                img.as_raw(),
                px_w,
                px_h,
                image::ExtendedColorType::Rgba8,
            )
            .ok()?;
            Some(out)
        }
    }
}

pub fn decode(warm: &WarmImage) -> Option<ReadyImage> {
    let raw = decode_raw(&warm.bytes, warm.px_w, warm.px_h, warm.codec)?;
    let img = image::RgbaImage::from_raw(warm.px_w, warm.px_h, raw)?;
    from_bgra(img, warm.dpr)
}

pub fn decode_raw(
    bytes: &[u8],
    px_w: u32,
    px_h: u32,
    codec: WarmCodec,
) -> Option<Vec<u8>> {
    if px_w == 0 || px_h == 0 {
        return None;
    }
    let expected = (px_w as usize)
        .saturating_mul(px_h as usize)
        .saturating_mul(4);
    if expected == 0 || expected > MAX_WARM_RAW {
        return None;
    }
    match codec {
        WarmCodec::Deflate => {
            let mut out = Vec::with_capacity(expected);
            let mut decoder = flate2::read::DeflateDecoder::new(bytes);
            let limit = (expected as u64).saturating_add(1);
            decoder.by_ref().take(limit).read_to_end(&mut out).ok()?;
            (out.len() == expected).then_some(out)
        }
        WarmCodec::Png => {
            let img = image::load_from_memory(bytes).ok()?.into_rgba8();
            if img.dimensions() != (px_w, px_h) {
                return None;
            }
            Some(img.into_raw())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_WARM_CODEC, WarmCodec, WarmImage, decode, decode_raw, encode, encode_raw};
    use crate::pixels::{ReadyImage, from_bgra};
    use image::RgbaImage;
    use std::sync::Arc;

    fn raster(w: u32, h: u32) -> ReadyImage {
        let mut img = RgbaImage::new(w, h);
        for (x, y, px) in img.enumerate_pixels_mut() {
            let v = ((x * 7 + y * 13) % 251) as u8;
            *px = image::Rgba([v, v.wrapping_add(3), v.wrapping_add(9), 255]);
        }
        from_bgra(img, 1.5).expect("raster")
    }

    fn warm(image: &ReadyImage, codec: WarmCodec) -> WarmImage {
        let bytes = encode(image, codec).expect("encode");
        WarmImage {
            bytes: Arc::new(bytes),
            codec,
            px_w: image.px_w,
            px_h: image.px_h,
            dpr: image.dpr,
            lease: None,
        }
    }

    #[test]
    fn both_codecs_round_trip_the_exact_pixels() {
        let original = raster(37, 19);
        let expected = original.image.as_bytes(0).expect("pixels")[..original.bytes].to_vec();
        for codec in [WarmCodec::Deflate, WarmCodec::Png] {
            let stored = warm(&original, codec);
            let restored = decode(&stored).expect("decode");
            assert_eq!(restored.px_w, original.px_w);
            assert_eq!(restored.px_h, original.px_h);
            assert_eq!(restored.dpr, original.dpr);
            assert_eq!(
                restored.image.as_bytes(0).expect("pixels")[..restored.bytes],
                expected[..],
                "{} must not change a channel",
                codec.name()
            );
        }
    }

    #[test]
    fn a_flat_raster_compresses_to_a_fraction_of_its_pixels() {
        let flat = from_bgra(
            RgbaImage::from_pixel(200, 200, image::Rgba([30, 30, 30, 255])),
            1.0,
        )
        .expect("raster");
        for codec in [WarmCodec::Deflate, WarmCodec::Png] {
            let stored = warm(&flat, codec);
            assert!(
                stored.resident_bytes() * 8 < flat.bytes,
                "{} kept {} of {} bytes",
                codec.name(),
                stored.resident_bytes(),
                flat.bytes
            );
        }
    }

    #[test]
    fn a_payload_that_inflates_to_the_wrong_size_is_rejected() {
        let original = raster(8, 8);
        let stored = warm(&original, DEFAULT_WARM_CODEC);
        assert!(
            decode_raw(&stored.bytes, 8, 9, DEFAULT_WARM_CODEC).is_none(),
            "a decoder that produces more bytes than the record says must not be painted"
        );
        assert!(decode_raw(&stored.bytes, 0, 8, DEFAULT_WARM_CODEC).is_none());
        assert!(decode_raw(b"", 8, 8, DEFAULT_WARM_CODEC).is_none());
    }

    #[test]
    fn a_truncated_payload_does_not_decode() {
        let original = raster(16, 16);
        let stored = warm(&original, DEFAULT_WARM_CODEC);
        let cut = &stored.bytes[..stored.bytes.len() / 2];
        assert!(decode_raw(cut, 16, 16, DEFAULT_WARM_CODEC).is_none());
    }

    #[test]
    fn encoding_refuses_a_buffer_that_is_not_the_declared_size() {
        assert!(encode_raw(&[0; 12], 2, 2, WarmCodec::Deflate).is_none());
        assert!(encode_raw(&[0; 16], 0, 2, WarmCodec::Deflate).is_none());
    }

    #[test]
    fn a_warm_entry_charges_its_compressed_bytes_not_its_pixels() {
        let original = raster(64, 64);
        let stored = warm(&original, DEFAULT_WARM_CODEC);
        assert_eq!(stored.decoded_bytes(), original.bytes);
        assert!(stored.resident_bytes() <= stored.decoded_bytes());
    }
}
