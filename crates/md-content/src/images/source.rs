use super::raster::native_dims;
use super::{DataEncoding, MAX_FETCH, MAX_IMAGE_PIXELS, Resolved, SourceLoad};
use futures::AsyncReadExt;
use gpui::{App, Image, ImageFormat, http_client};
use image::ImageReader;
use std::io::Cursor;
use std::path::{Path, PathBuf};

pub fn resolve(dest: &str, source_path: Option<&Path>) -> Option<Resolved> {
    let dest = dest.trim();
    if dest.is_empty() {
        return None;
    }
    if dest.starts_with("data:") {
        let (format, payload, encoding) = data_url_parts(dest)?;
        if data_payload_too_big(payload, encoding) {
            return None;
        }
        return Some(Resolved::Data {
            payload: payload.to_string(),
            format,
            encoding,
        });
    }
    if is_http_url(dest) {
        return Some(Resolved::Remote(dest.to_string()));
    }
    if starts_with_ascii_case_insensitive(dest, b"file:") {
        let path = parse_file_url(dest)?;
        return Some(Resolved::Local(normalize_local(path)));
    }

    if has_rejected_scheme(dest) {
        return None;
    }
    let path = PathBuf::from(dest);
    if path.is_absolute() || looks_like_windows_path(dest) {
        return Some(Resolved::Local(normalize_local(path)));
    }
    let parent = source_path.and_then(Path::parent)?;
    Some(Resolved::Local(normalize_local(parent.join(dest))))
}

pub fn load_source(resolved: Resolved, allow_remote: bool, cx: &mut App) -> SourceLoad {
    match resolved {
        Resolved::Remote(_) if !allow_remote => Box::pin(async {
            Err(crate::Error::Image(
                md_i18n::Key::ImageRemoteDisabled.into(),
            ))
        }),
        Resolved::Remote(url) => load_remote(url, cx),
        Resolved::Local(path) => load_local(path, cx),
        Resolved::Data {
            payload,
            format,
            encoding,
        } => decode_data(payload, format, encoding, cx),
    }
}

fn load_local(path: PathBuf, cx: &mut App) -> SourceLoad {
    if file_too_big(&path) {
        return Box::pin(async { Err(crate::Error::Image(md_i18n::Key::ImageTooLarge.into())) });
    }

    decode_bytes_from_path(path.clone(), format_from_path(&path), cx)
}

fn load_remote(url: String, cx: &mut App) -> SourceLoad {
    let client = cx.http_client();
    let svg = cx.svg_renderer();
    Box::pin(async move {
        let mut response = client
            .get(&url, ().into(), true)
            .await
            .map_err(|e| crate::Error::Image(e.to_string().into()))?;
        let status = response.status();
        let mut bytes = Vec::new();
        response
            .body_mut()
            .take(MAX_FETCH as u64 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| crate::Error::Image(e.to_string().into()))?;
        if !status.is_success() {
            return Err(crate::Error::Image(
                format!("unexpected http status for {url}: {status}").into(),
            ));
        }
        if bytes.len() > MAX_FETCH {
            return Err(crate::Error::Image(md_i18n::Key::ImageTooLarge.into()));
        }
        let format = detect_format(&bytes)
            .ok_or_else(|| crate::Error::Image("unsupported image format".to_owned().into()))?;
        validate_image_dimensions(format, &bytes)?;
        Image::from_bytes(format, bytes)
            .to_image_data(svg)
            .map_err(|e| crate::Error::Image(e.to_string().into()))
            .and_then(|img| {
                native_dims(img).ok_or_else(|| crate::Error::Image(md_i18n::Key::ImageEmpty.into()))
            })
    })
}

fn decode_bytes_from_path(path: PathBuf, format: Option<ImageFormat>, cx: &mut App) -> SourceLoad {
    let svg = cx.svg_renderer();
    Box::pin(async move {
        let bytes = std::fs::read(&path).map_err(|e| crate::Error::Image(e.to_string().into()))?;
        if bytes.len() > MAX_FETCH {
            return Err(crate::Error::Image(md_i18n::Key::ImageTooLarge.into()));
        }
        let format = format
            .or_else(|| detect_format(&bytes))
            .ok_or_else(|| crate::Error::Image("unsupported image format".to_owned().into()))?;
        validate_image_dimensions(format, &bytes)?;
        Image::from_bytes(format, bytes)
            .to_image_data(svg)
            .map_err(|e| crate::Error::Image(e.to_string().into()))
            .and_then(|img| {
                native_dims(img).ok_or_else(|| crate::Error::Image(md_i18n::Key::ImageEmpty.into()))
            })
    })
}

fn decode_data(
    payload: String,
    format: ImageFormat,
    encoding: DataEncoding,
    cx: &mut App,
) -> SourceLoad {
    let svg = cx.svg_renderer();
    Box::pin(async move {
        let bytes = decode_data_payload(&payload, encoding)
            .ok_or_else(|| crate::Error::Image(md_i18n::Key::ImageBadPath.into()))?;
        if bytes.len() > MAX_FETCH {
            return Err(crate::Error::Image(md_i18n::Key::ImageTooLarge.into()));
        }
        validate_image_dimensions(format, &bytes)?;
        Image::from_bytes(format, bytes)
            .to_image_data(svg)
            .map_err(|e| crate::Error::Image(e.to_string().into()))
            .and_then(|img| {
                native_dims(img).ok_or_else(|| crate::Error::Image(md_i18n::Key::ImageEmpty.into()))
            })
    })
}

pub(super) fn detect_format(bytes: &[u8]) -> Option<ImageFormat> {
    if let Ok(format) = image::guess_format(bytes) {
        return match format {
            image::ImageFormat::Png => Some(ImageFormat::Png),
            image::ImageFormat::Jpeg => Some(ImageFormat::Jpeg),
            image::ImageFormat::WebP => Some(ImageFormat::Webp),
            image::ImageFormat::Gif => Some(ImageFormat::Gif),
            image::ImageFormat::Bmp => Some(ImageFormat::Bmp),
            image::ImageFormat::Tiff => Some(ImageFormat::Tiff),
            _ => None,
        };
    }
    if !svg_depth_within_limit(bytes) {
        return None;
    }
    usvg::Tree::from_data(bytes, &usvg::Options::default())
        .ok()
        .map(|_| ImageFormat::Svg)
}

const MAX_SVG_DEPTH: usize = 1024;

fn svg_depth_within_limit(bytes: &[u8]) -> bool {
    let find_from = |haystack: &[u8], needle: &[u8]| -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    };
    let mut depth: usize = 0;
    let mut i = 0;
    while i < bytes.len() {
        let Some(offset) = bytes[i..].iter().position(|&b| b == b'<') else {
            break;
        };
        i += offset + 1;
        let rest = &bytes[i..];
        if rest.starts_with(b"!--") {
            i = match find_from(rest, b"-->") {
                Some(p) => i + p + 3,
                None => break,
            };
        } else if rest.starts_with(b"![CDATA[") {
            i = match find_from(rest, b"]]>") {
                Some(p) => i + p + 3,
                None => break,
            };
        } else if rest.starts_with(b"?") {
            i = match find_from(rest, b"?>") {
                Some(p) => i + p + 2,
                None => break,
            };
        } else if rest.starts_with(b"!") {
            i = match find_from(rest, b">") {
                Some(p) => i + p + 1,
                None => break,
            };
        } else if rest.starts_with(b"/") {
            i = match find_from(rest, b">") {
                Some(p) => i + p + 1,
                None => break,
            };
            depth = depth.saturating_sub(1);
        } else {
            let mut j = 0;
            let mut quote = 0u8;
            let mut self_closed = false;
            let mut closed = false;
            while j < rest.len() {
                let b = rest[j];
                if quote != 0 {
                    if b == quote {
                        quote = 0;
                    }
                } else if b == b'"' || b == b'\'' {
                    quote = b;
                } else if b == b'>' {
                    self_closed = j > 0 && rest[j - 1] == b'/';
                    closed = true;
                    break;
                }
                j += 1;
            }
            if !closed {
                break;
            }
            i += j + 1;
            if !self_closed {
                depth += 1;
                if depth > MAX_SVG_DEPTH {
                    return false;
                }
            }
        }
    }
    true
}

pub(super) fn validate_svg_dimensions(bytes: &[u8]) -> Result<(), crate::Error> {
    if !svg_depth_within_limit(bytes) {
        return Err(crate::Error::Image(md_i18n::Key::ImageTooLarge.into()));
    }
    let tree = usvg::Tree::from_data(bytes, &usvg::Options::default())
        .map_err(|e| crate::Error::Image(e.to_string().into()))?;
    let size = tree.size();
    let pixels = f64::from(size.width()) * f64::from(size.height());
    if pixels > MAX_IMAGE_PIXELS as f64 {
        return Err(crate::Error::Image(md_i18n::Key::ImageTooLarge.into()));
    }
    Ok(())
}

pub(super) fn validate_image_dimensions(
    format: ImageFormat,
    bytes: &[u8],
) -> Result<(), crate::Error> {
    if format == ImageFormat::Svg {
        return validate_svg_dimensions(bytes);
    }
    let (width, height) = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| crate::Error::Image(e.to_string().into()))?
        .into_dimensions()
        .map_err(|e| crate::Error::Image(e.to_string().into()))?;
    if width == 0 || height == 0 {
        return Err(crate::Error::Image(md_i18n::Key::ImageEmpty.into()));
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| crate::Error::Image(md_i18n::Key::ImageTooLarge.into()))?;
    if pixels > MAX_IMAGE_PIXELS {
        return Err(crate::Error::Image(md_i18n::Key::ImageTooLarge.into()));
    }
    Ok(())
}

fn is_http_url(dest: &str) -> bool {
    dest.len() >= 8
        && (starts_with_ascii_case_insensitive(dest, b"http://")
            || starts_with_ascii_case_insensitive(dest, b"https://"))
}

fn starts_with_ascii_case_insensitive(value: &str, prefix: &[u8]) -> bool {
    value
        .as_bytes()
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

fn looks_like_windows_path(dest: &str) -> bool {
    let b = dest.as_bytes();
    if dest.starts_with('\\') {
        return true;
    }
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

fn has_rejected_scheme(dest: &str) -> bool {
    if looks_like_windows_path(dest) {
        return false;
    }
    let Some((scheme, rest)) = dest.split_once(':') else {
        return false;
    };
    if scheme.is_empty() {
        return false;
    }
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
        return false;
    }
    !rest.is_empty()
}

fn normalize_local(path: PathBuf) -> PathBuf {
    match path.canonicalize() {
        Ok(p) => strip_verbatim(p),
        Err(_) => std::path::absolute(&path)
            .map(strip_verbatim)
            .unwrap_or(path),
    }
}

fn strip_verbatim(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        if let Some(unc) = rest.strip_prefix("UNC\\") {
            PathBuf::from(format!(r"\\{unc}"))
        } else {
            PathBuf::from(rest)
        }
    } else {
        path
    }
}

fn format_from_path(path: &Path) -> Option<ImageFormat> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" | "svgz" => "image/svg+xml",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        _ => return None,
    };
    ImageFormat::from_mime_type(mime)
}

pub fn is_image_path(path: &Path) -> bool {
    format_from_path(path).is_some()
}

pub fn markdown_dest(image: &Path, source_path: Option<&Path>) -> String {
    let image = normalize_local(image.to_path_buf());
    let Some(md) = source_path else {
        return path_to_dest(&image);
    };
    let Some(parent) = md.parent().filter(|p| !p.as_os_str().is_empty()) else {
        return path_to_dest(&image);
    };
    let parent = normalize_local(parent.to_path_buf());
    match relative_to(&parent, &image) {
        Some(rel) if !rel.as_os_str().is_empty() => path_to_dest(&rel),
        _ => path_to_dest(&image),
    }
}

fn relative_to(from_dir: &Path, to_file: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let from: Vec<_> = from_dir.components().collect();
    let to: Vec<_> = to_file.components().collect();
    if from.first() != to.first() {
        return None;
    }
    let mut i = 0;
    while i < from.len() && i < to.len() && from[i] == to[i] {
        i += 1;
    }
    let mut out = PathBuf::new();
    for _ in i..from.len() {
        out.push("..");
    }
    for c in &to[i..] {
        match c {
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    Some(out)
}

fn path_to_dest(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn parse_file_url(dest: &str) -> Option<PathBuf> {
    let url = http_client::Url::parse(dest).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    url.to_file_path().ok()
}

fn data_url_parts(dest: &str) -> Option<(ImageFormat, &str, DataEncoding)> {
    let rest = dest.trim().strip_prefix("data:")?;
    let (header, payload) = rest.split_once(',')?;
    let header = header.to_ascii_lowercase();
    let mime = header.split(';').next()?;
    let format = ImageFormat::from_mime_type(mime)?;
    let encoding = if header
        .split(';')
        .skip(1)
        .any(|part| part.trim() == "base64")
    {
        DataEncoding::Base64
    } else {
        DataEncoding::Percent
    };
    Some((format, payload.trim(), encoding))
}

fn data_payload_too_big(payload: &str, encoding: DataEncoding) -> bool {
    let max_encoded = match encoding {
        DataEncoding::Base64 => MAX_FETCH.div_ceil(3).saturating_mul(4),
        DataEncoding::Percent => MAX_FETCH.saturating_mul(3),
    };
    payload.len() > max_encoded
}

pub(super) fn data_cache_key(dest: &str) -> Option<String> {
    let dest = dest.trim();
    data_url_parts(dest)?;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in dest.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Some(format!("data:{hash:016x}"))
}

pub(super) fn decode_data_payload(payload: &str, encoding: DataEncoding) -> Option<Vec<u8>> {
    if encoding == DataEncoding::Percent {
        return Some(percent_encoding::percent_decode_str(payload).collect());
    }
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(payload)
        .ok()
        .or_else(|| {
            base64::engine::general_purpose::STANDARD_NO_PAD
                .decode(payload)
                .ok()
        })
}

fn file_too_big(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|meta| meta.len() > MAX_FETCH as u64)
        .unwrap_or(false)
}
