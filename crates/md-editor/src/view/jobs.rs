use super::{DisplayJob, ImagePopover, ImageSchedule, MathPopover};
use md_content::shaper::ShapePart;
use md_content::{images, math};
use md_core::block::{BlockId, BlockKind};
use md_theme::DocumentTheme;
use std::collections::HashMap;

pub(crate) fn extend_link_dests(
    doc: &md_core::doc::Doc,
    from: usize,
    link_dests: &mut HashMap<u32, String>,
    link_raw: &mut HashMap<u32, (String, String)>,
    data_source_links: &mut HashMap<String, u32>,
) -> usize {
    let path = doc.source_path.as_deref();
    let links = &doc.document.links;
    for (i, link) in links.iter().enumerate().skip(from) {
        let id = i as u32;
        let key = images::cache_key(&link.dest, path);
        if link.dest.trim().starts_with("data:") && key != link.dest {
            data_source_links.insert(key.clone(), id);
        }
        link_dests.insert(id, key);
        link_raw.insert(id, (link.dest.clone(), link.title.clone()));
    }
    links.len()
}

pub(crate) fn extra_snapshots(
    doc: &md_core::doc::Doc,
) -> (HashMap<u32, String>, HashMap<u32, String>) {
    let path = doc.source_path.as_deref();
    let mut block_image_dest = HashMap::new();
    let mut block_code_lang = HashMap::new();
    doc.document.for_each_extra(|index, extra| {
        if let Some(dest_id) = extra.image_dest() {
            let dest = doc.document.link_dest(dest_id).unwrap_or("");
            block_image_dest.insert(index, images::cache_key(dest, path));
        } else if let Some(lang_id) = extra.code_fence_lang() {
            let lang = doc
                .document
                .lang(lang_id)
                .unwrap_or("")
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            block_code_lang.insert(index, lang);
        }
    });
    (block_image_dest, block_code_lang)
}
pub(super) fn collect_image_jobs(
    art: &md_content::shaper::ShapeArtifact,
    block: BlockId,
    sched: &mut ImageSchedule<'_>,
) {
    for band in &art.bands {
        for part in &band.parts {
            let ShapePart::Image {
                dest,
                slot_w,
                slot_h,
                fallback,
                ..
            } = part
            else {
                continue;
            };
            if dest.is_empty() {
                continue;
            }
            let retry = sched.cache.source_retry_due(dest);
            if fallback.is_some() && !retry {
                continue;
            }
            let dkey = images::display_key(dest, *slot_w as f32, *slot_h as f32, sched.dpr);
            sched.protect.push(dkey.clone());
            if !sched.cache.source_contains(dest) || retry {
                let entry = sched.source_jobs.entry(dest.clone()).or_default();
                if !entry.contains(&block) {
                    entry.push(block);
                }
            } else if let Some(source) = sched.cache.source_image(dest) {
                let entry = sched
                    .display_jobs
                    .entry(dkey)
                    .or_insert_with(|| DisplayJob {
                        source,
                        blocks: Vec::new(),
                    });
                if !entry.blocks.contains(&block) {
                    entry.blocks.push(block);
                }
            }
        }
    }
}

pub(super) fn collect_popover_job(popover: &ImagePopover, sched: &mut ImageSchedule<'_>) {
    let dest = &popover.dest;
    let retry = sched.cache.source_retry_due(dest);
    if !sched.cache.source_contains(dest) || retry {
        sched.source_jobs.entry(dest.clone()).or_default();
        return;
    }
    let Some(plan) = popover.plan.as_ref() else {
        return;
    };
    sched.protect.push(plan.key.clone());
    if let Some(source) = sched.cache.source_image(dest) {
        sched
            .display_jobs
            .entry(plan.key.clone())
            .or_insert_with(|| DisplayJob {
                source,
                blocks: Vec::new(),
            });
    }
}

pub(super) fn collect_math_jobs(
    art: &md_content::shaper::ShapeArtifact,
    kind: BlockKind,
    block: BlockId,
    theme: &DocumentTheme,
    dpr: f64,
    jobs: &mut HashMap<math::MathKey, (String, math::RasterSpec, Vec<BlockId>)>,
    protect: &mut Vec<math::MathKey>,
) {
    let color = math::color_for(theme, kind);
    for band in &art.bands {
        for part in &band.parts {
            let ShapePart::Math {
                latex,
                display,
                em,
                fallback,
                ..
            } = part
            else {
                continue;
            };
            if fallback.is_some() {
                continue;
            }
            let key = math::key_for(latex, *display, *em, color, dpr);
            protect.push(key.clone());
            let spec = math::spec_from_key(&key, color);
            let entry = jobs
                .entry(key)
                .or_insert_with(|| (latex.clone(), spec, Vec::new()));
            if !entry.2.contains(&block) {
                entry.2.push(block);
            }
        }
    }
}

pub(super) fn collect_math_popover_job(
    popover: &MathPopover,
    jobs: &mut HashMap<math::MathKey, (String, math::RasterSpec, Vec<BlockId>)>,
    protect: &mut Vec<math::MathKey>,
) {
    let Some(plan) = popover.plan.as_ref() else {
        return;
    };
    protect.push(plan.key.clone());
    let spec = math::spec_from_key(&plan.key, plan.color);
    jobs.entry(plan.key.clone())
        .or_insert_with(|| (plan.key.latex.clone(), spec, Vec::new()));
}
