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

    println!("total {total} bytes under {}", root.display());
}
