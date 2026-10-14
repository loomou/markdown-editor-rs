use super::{DisplayJob, ImagePopover, ImageSchedule, MathPopover};
use md_content::shaper::ShapePart;
use md_content::{images, math};
use md_core::block::{BlockId, BlockKind, FrontMatterMarker, NodeExtra};
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
    let mut block_image_dest = HashMap::new();
    let mut block_code_lang = HashMap::new();
    doc.document.for_each_extra(|index, extra| {
        insert_extra(
            doc,
            index,
            extra,
            &mut block_image_dest,
            &mut block_code_lang,
        );
    });
    (block_image_dest, block_code_lang)
}

fn image_value(doc: &md_core::doc::Doc, dest_id: u32) -> String {
    let dest = doc.document.link_dest(dest_id).unwrap_or("");
    images::cache_key(dest, doc.source_path.as_deref())
}

fn lang_value(doc: &md_core::doc::Doc, extra: NodeExtra) -> Option<String> {
    if let NodeExtra::CodeFence { lang, .. } = extra {
        return Some(
            lang.and_then(|lang_id| doc.document.lang(lang_id))
                .unwrap_or("")
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase(),
        );
    }
    extra
        .front_matter_marker()
        .map(|marker| front_matter_lang(marker).to_string())
}

fn has_lang(extra: NodeExtra) -> bool {
    matches!(extra, NodeExtra::CodeFence { .. }) || extra.front_matter_marker().is_some()
}

fn insert_extra(
    doc: &md_core::doc::Doc,
    index: u32,
    extra: NodeExtra,
    block_image_dest: &mut HashMap<u32, String>,
    block_code_lang: &mut HashMap<u32, String>,
) {
    if let Some(dest_id) = extra.image_dest() {
        block_image_dest.insert(index, image_value(doc, dest_id));
    } else if let Some(lang) = lang_value(doc, extra) {
        block_code_lang.insert(index, lang);
    }
}

fn write_extra(
    doc: &md_core::doc::Doc,
    index: u32,
    extra: NodeExtra,
    block_image_dest: &mut HashMap<u32, String>,
    block_code_lang: &mut HashMap<u32, String>,
) {
    block_image_dest.remove(&index);
    block_code_lang.remove(&index);
    insert_extra(doc, index, extra, block_image_dest, block_code_lang);
}

fn visit_extra_subtree(
    doc: &md_core::doc::Doc,
    root: md_core::document::NodeId,
    mut visit: impl FnMut(u32, NodeExtra),
) {
    let arena = &doc.document.arena;
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let Some(node) = arena.get(id) else {
            continue;
        };
        visit(id.index, node.extra);
        let mut child = node.last_child;
        while let Some(c) = child {
            stack.push(c);
            child = arena.get(c).and_then(|n| n.prev_sibling);
        }
    }
}

fn prune_dead(
    doc: &md_core::doc::Doc,
    block_image_dest: &mut HashMap<u32, String>,
    block_code_lang: &mut HashMap<u32, String>,
) {
    block_image_dest.retain(|index, _| {
        doc.document
            .live_id(*index)
            .is_some_and(|id| doc.document.extra(id).image_dest().is_some())
    });
    block_code_lang.retain(|index, _| {
        doc.document
            .live_id(*index)
            .is_some_and(|id| has_lang(doc.document.extra(id)))
    });
}

fn refresh_image_values(doc: &md_core::doc::Doc, block_image_dest: &mut HashMap<u32, String>) {
    for (index, value) in block_image_dest.iter_mut() {
        let fresh = doc
            .document
            .live_id(*index)
            .map(|id| doc.document.extra(id))
            .and_then(|extra| extra.image_dest())
            .map(|dest_id| image_value(doc, dest_id));
        if let Some(fresh) = fresh {
            *value = fresh;
        }
    }
}

pub(crate) fn fold_extras(
    doc: &md_core::doc::Doc,
    pending: &md_core::document::ChangeSet,
    block_image_dest: &mut std::rc::Rc<HashMap<u32, String>>,
    block_code_lang: &mut std::rc::Rc<HashMap<u32, String>>,
) -> bool {
    use md_core::document::DocChange;

    let mut removed = false;
    let mut dests_changed = false;
    for change in &pending.changes {
        match change {
            DocChange::DocumentReplaced => return false,
            DocChange::ReferenceDefsChanged { .. } => dests_changed = true,
            DocChange::TreeSpliced { removed: gone, .. } => removed |= !gone.is_empty(),
            _ => {}
        }
    }

    let block_image_dest = std::rc::Rc::make_mut(block_image_dest);
    let block_code_lang = std::rc::Rc::make_mut(block_code_lang);

    for change in &pending.changes {
        match change {
            DocChange::AttrsChanged {
                node, new_extra, ..
            } => {
                if doc.document.live_id(node.index).is_some() {
                    write_extra(
                        doc,
                        node.index,
                        *new_extra,
                        block_image_dest,
                        block_code_lang,
                    );
                }
            }
            DocChange::TreeSpliced { inserted, .. } => {
                for node in inserted {
                    visit_extra_subtree(doc, *node, |index, extra| {
                        write_extra(doc, index, extra, block_image_dest, block_code_lang);
                    });
                }
            }
            _ => {}
        }
    }

    if removed {
        prune_dead(doc, block_image_dest, block_code_lang);
    }
    if dests_changed {
        refresh_image_values(doc, block_image_dest);
    }
    true
}

fn front_matter_lang(marker: FrontMatterMarker) -> &'static str {
    match marker {
        FrontMatterMarker::Dash => "yaml",
        FrontMatterMarker::Plus => "toml",
    }
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
