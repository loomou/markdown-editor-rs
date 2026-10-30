use std::fs;
use std::path::PathBuf;

use md_bench::corpus::{self, Shape};

const SEED: u64 = 0x5eed_0f17;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus");
    fs::create_dir_all(&root).expect("the corpus directory must be creatable");
    let sizes = corpus::sizes();
    println!(
        "{:<18} {:>6} {:>10} {:>18}",
        "shape", "size", "bytes", "fingerprint"
    );
    let mut total = 0usize;
    for shape in Shape::ALL {
        let dir = root.join(shape.name());
        fs::create_dir_all(&dir).expect("the shape directory must be creatable");
        for size in &sizes {
            let text = corpus::generate(shape, *size, SEED);
            let path = dir.join(format!("{}.md", corpus::human(*size)));
            fs::write(&path, &text).expect("the corpus file must be writable");
            println!(
                "{:<18} {:>6} {:>10} {:>18x}",
                shape.name(),
                corpus::human(*size),
                text.len(),
                corpus::fingerprint(&text)
            );
            total += text.len();
        }
    }

    let large = root.join("large");
    fs::create_dir_all(&large).expect("the large corpus directory must be creatable");
    for blocks in corpus::BLOCK_COUNTS {
        let text = corpus::generate_blocks(blocks, corpus::BLOCK_SEED);
        let label = corpus::human_blocks(blocks);
        let path = large.join(format!("{label}.md"));
        fs::write(&path, &text).expect("the large corpus file must be writable");
        println!(
            "{:<18} {:>6} {:>10} {:>18x}",
            "large",
            label,
            text.len(),
            corpus::fingerprint(&text)
        );
        total += text.len();
    }

    let media = root.join("media");
    fs::create_dir_all(&media).expect("the media corpus directory must be creatable");
    let mut images = corpus::media_image_specs();
    images.push(corpus::GIANT_IMAGE);
    for spec in &images {
        let png = corpus::encode_png(&corpus::media_pixels(spec, corpus::MEDIA_SEED));
        let path = media.join(format!("{}.png", spec.name));
        fs::write(&path, &png).expect("the media corpus image must be writable");
        println!(
            "{:<18} {:>5}x{:<5} {:>10} {:>18x}",
            "media-image",
            spec.width,
            spec.height,
            png.len(),
            corpus::fingerprint(&format!("{}x{}", spec.width, spec.height))
        );
        total += png.len();
    }

    let docs = [
        (
            "images.md",
            corpus::media_images_doc(&corpus::MEDIA_IMAGES, true),
        ),
        (
            "giant.md",
            corpus::media_images_doc(&[corpus::GIANT_IMAGE], true),
        ),
        (
            "mermaid.md",
            corpus::media_mermaid_doc(&corpus::MEDIA_MERMAID),
        ),
        (
            "mermaid-repeat.md",
            corpus::media_mermaid_repeat_doc(&corpus::MEDIA_MERMAID[..2], 6),
        ),
        ("text.md", corpus::media_text_doc(12)),
    ];
    for (name, text) in &docs {
        let path = media.join(name);
        fs::write(&path, text).expect("the media corpus document must be writable");
        println!(
            "{:<18} {:>11} {:>10} {:>18x}",
            "media-doc",
            name,
            text.len(),
            corpus::fingerprint(text)
        );
        total += text.len();
    }

    println!("total {total} bytes under {}", root.display());
}
