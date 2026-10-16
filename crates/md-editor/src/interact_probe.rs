use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gpui::{
    App, AsyncWindowContext, Keystroke, Modifiers, MouseMoveEvent, Pixels, PlatformInput, Point,
    ScrollDelta, ScrollWheelEvent, TouchPhase, WeakEntity, Window, px,
};

use md_core::doc::Doc;
use md_core::document::{editor_options, load_markdown};

use crate::Shell;

const ENV: &str = "MARKDOWN_EDITOR_RS_INTERACT_ONCE";
const REPS_ENV: &str = "MARKDOWN_EDITOR_RS_INTERACT_REPS";
const DEFAULT_REPS: usize = 25;
const WARMUP_REPS: usize = 3;
const SETTLE: Duration = Duration::from_millis(60);
const START_DELAY: Duration = Duration::from_millis(1200);
const FLUSH: Duration = Duration::from_millis(400);
const WAIT_CAP: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_micros(500);
const COST_CAP: usize = 8192;

static EPOCH: AtomicU64 = AtomicU64::new(0);
static FRAME_ORIGIN: Mutex<Option<(u64, Instant)>> = Mutex::new(None);
static RENDER_END: Mutex<Vec<Cost>> = Mutex::new(Vec::new());
static PREPAINT_START: Mutex<Vec<Cost>> = Mutex::new(Vec::new());
static PREPAINT_END: Mutex<Vec<Cost>> = Mutex::new(Vec::new());
static PAINT_START: Mutex<Vec<Cost>> = Mutex::new(Vec::new());
static PAINT_END: Mutex<Vec<Cost>> = Mutex::new(Vec::new());
static PAINT_DUR: Mutex<Vec<Cost>> = Mutex::new(Vec::new());
static STATUS_DUR: Mutex<Vec<Cost>> = Mutex::new(Vec::new());
static PRE_BUILD: Mutex<Vec<Cost>> = Mutex::new(Vec::new());
static ENGINE_DUR: Mutex<Vec<Cost>> = Mutex::new(Vec::new());
static ASSEMBLE_DUR: Mutex<Vec<Cost>> = Mutex::new(Vec::new());
static GEOMETRY_DUR: Mutex<Vec<Cost>> = Mutex::new(Vec::new());
static CHANGES_DUR: Mutex<Vec<Cost>> = Mutex::new(Vec::new());
static BLOCK_EDIT_DUR: Mutex<Vec<Cost>> = Mutex::new(Vec::new());
static STABILISE_DUR: Mutex<Vec<Cost>> = Mutex::new(Vec::new());
static CARET_DUR: Mutex<Vec<Cost>> = Mutex::new(Vec::new());
static CLIPBOARD_WRITES: AtomicU64 = AtomicU64::new(0);
static CLIPBOARD_BYTES: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
struct Cost {
    epoch: u64,
    ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Stamp {
    bytes: usize,
    revision: u64,
    scroll: i64,
    rebuilds: u64,
    selection: Option<(u32, u32)>,
    clipboard_writes: u64,
    clipboard_bytes: usize,
}

struct Sample {
    op: usize,
    dispatch_ms: f64,
    pickup_ms: f64,
    period_ms: f64,
    pre_draw_ms: f64,
    post_draw_ms: f64,
    epoch: u64,
    moved: bool,
    rebuilds: u64,
    rev_before: u64,
    rev_after: u64,
    clip_writes: u64,
    clip_bytes: usize,
}

struct Marks {
    render: f64,
    to_prepaint: f64,
    prepaint: f64,
    gap: f64,
    paint: f64,
    draw: f64,
}

#[derive(Clone, Copy)]
enum Op {
    Idle,
    InsertChar,
    Newline,
    Backspace,
    Undo,
    Redo,
    ScrollNotch,
    ScrollBurst,
    PageDown,
    ShiftDown,
    ShiftPageDown,
    SelectAll,
    CopySmall,
    PasteSmall,
    CopyLarge,
    PasteLarge,
}

const OPS: [Op; 16] = [
    Op::Idle,
    Op::InsertChar,
    Op::Newline,
    Op::Backspace,
    Op::Undo,
    Op::Redo,
    Op::ScrollNotch,
    Op::ScrollBurst,
    Op::PageDown,
    Op::ShiftDown,
    Op::ShiftPageDown,
    Op::SelectAll,
    Op::CopySmall,
    Op::PasteSmall,
    Op::CopyLarge,
    Op::PasteLarge,
];

impl Op {
    fn name(self) -> &'static str {
        match self {
            Op::Idle => "idle",
            Op::InsertChar => "insert_char",
            Op::Newline => "newline",
            Op::Backspace => "backspace",
            Op::Undo => "undo",
            Op::Redo => "redo",
            Op::ScrollNotch => "scroll_notch",
            Op::ScrollBurst => "scroll_burst",
            Op::PageDown => "page_down",
            Op::ShiftDown => "shift_down",
            Op::ShiftPageDown => "shift_page_down",
            Op::SelectAll => "select_all",
            Op::CopySmall => "copy_small",
            Op::PasteSmall => "paste_small",
            Op::CopyLarge => "copy_large",
            Op::PasteLarge => "paste_large",
        }
    }

    fn keys(self) -> &'static [&'static str] {
        match self {
            Op::InsertChar => &["x"],
            Op::Newline => &["enter"],
            Op::Backspace => &["backspace"],
            Op::Undo => &["ctrl-z"],
            Op::Redo => &["ctrl-y"],
            Op::PageDown => &["pagedown"],
            Op::ShiftDown => &["shift-down"],
            Op::ShiftPageDown => &["shift-pagedown"],
            Op::SelectAll => &["ctrl-a"],
            Op::CopySmall => &["ctrl-shift-right", "ctrl-c"],
            Op::PasteSmall | Op::PasteLarge => &["ctrl-v"],
            Op::CopyLarge => &["ctrl-c"],
            _ => &[],
        }
    }

    fn setup(self) -> &'static [&'static str] {
        match self {
            Op::CopyLarge => &["shift-pagedown"],
            _ => &[],
        }
    }

    fn resets(self) -> bool {
        matches!(
            self,
            Op::CopySmall | Op::PasteSmall | Op::CopyLarge | Op::PasteLarge
        )
    }

    fn notches(self) -> usize {
        match self {
            Op::ScrollBurst => 8,
            Op::ScrollNotch => 1,
            _ => 0,
        }
    }
}

fn push(slot: &Mutex<Vec<Cost>>, ms: f64) {
    let Ok(mut costs) = slot.lock() else {
        return;
    };
    if costs.len() >= COST_CAP {
        return;
    }
    costs.push(Cost {
        epoch: EPOCH.load(Ordering::Relaxed),
        ms,
    });
}

fn delta_from_origin(at: Instant) -> Option<f64> {
    let epoch = EPOCH.load(Ordering::Relaxed);
    if epoch == 0 {
        return None;
    }
    let guard = FRAME_ORIGIN.lock().ok()?;
    let (seen, origin) = (*guard)?;
    if seen != epoch {
        return None;
    }
    Some(at.saturating_duration_since(origin).as_secs_f64() * 1000.0)
}

fn mark_now(slot: &Mutex<Vec<Cost>>) {
    if let Some(ms) = delta_from_origin(Instant::now()) {
        push(slot, ms);
    }
}

pub fn mark_frame_start() {
    let epoch = EPOCH.load(Ordering::Relaxed);
    if epoch == 0 {
        return;
    }
    if let Ok(mut slot) = FRAME_ORIGIN.lock() {
        *slot = Some((epoch, Instant::now()));
    }
}

pub fn origin_instant() -> Option<Instant> {
    let epoch = EPOCH.load(Ordering::Relaxed);
    let guard = FRAME_ORIGIN.lock().ok()?;
    match *guard {
        Some((seen, at)) if seen == epoch => Some(at),
        _ => None,
    }
}

pub fn mark_render_end() {
    mark_now(&RENDER_END);
}

pub fn mark_prepaint_start(start: Instant) {
    if let Some(ms) = delta_from_origin(start) {
        push(&PREPAINT_START, ms);
    }
}

pub fn mark_prepaint_end() {
    mark_now(&PREPAINT_END);
}

pub fn mark_paint_start() {
    mark_now(&PAINT_START);
}

pub fn record_paint(paint_ms: f64) {
    mark_now(&PAINT_END);
    push(&PAINT_DUR, paint_ms);
}

pub fn record_status(status_ms: f64) {
    push(&STATUS_DUR, status_ms);
}

pub fn record_build(
    frame_start: Instant,
    build_start: Instant,
    engine: Duration,
    assemble: Duration,
    geometry: Duration,
) {
    if let (Some(from), Some(to)) = (
        delta_from_origin(frame_start),
        delta_from_origin(build_start),
    ) {
        push(&PRE_BUILD, (to - from).max(0.0));
    }
    let to_ms = |d: Duration| d.as_secs_f64() * 1000.0;
    push(&ENGINE_DUR, to_ms(engine));
    push(&ASSEMBLE_DUR, to_ms(assemble));
    push(&GEOMETRY_DUR, to_ms(geometry));
}

pub fn record_phases(
    changes: Duration,
    block_edit: Duration,
    stabilise: Duration,
    caret: Duration,
) {
    let to_ms = |d: Duration| d.as_secs_f64() * 1000.0;
    push(&CHANGES_DUR, to_ms(changes));
    push(&BLOCK_EDIT_DUR, to_ms(block_edit));
    push(&STABILISE_DUR, to_ms(stabilise));
    push(&CARET_DUR, to_ms(caret));
}

pub fn note_clipboard_write(bytes: usize) {
    CLIPBOARD_WRITES.fetch_add(1, Ordering::Relaxed);
    CLIPBOARD_BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
}

pub fn spawn(window: &mut Window, shell: WeakEntity<Shell>, cx: &mut App) {
    if std::env::var_os(ENV).is_none() {
        return;
    }
    let wcx = window.to_async(cx);
    let samples = Rc::new(RefCell::new(Vec::new()));
    let op_loop_ms = Rc::new(RefCell::new(Vec::new()));
    wcx.spawn(async move |wcx| {
        run(wcx, shell, samples, op_loop_ms).await;
    })
    .detach();
}

fn emit(line: &str) {
    tracing::info!(target: "md_editor::interact", "{line}");
}

fn stamp(shell: &WeakEntity<Shell>, cx: &App) -> Option<Stamp> {
    let shell = shell.upgrade()?;
    let editor = shell.read(cx).editor().clone();
    let view = editor.read(cx);
    Some(Stamp {
        bytes: view.state.doc.document.source.len(),
        revision: view.state.doc.document.revision(),
        scroll: view.state.scroll.round() as i64,
        rebuilds: view
            .state
            .incremental
            .as_ref()
            .map(|engine| engine.doc_rebuilds())
            .unwrap_or(0),
        selection: view
            .state
            .selection
            .map(|(from, to)| (from.block, to.block)),
        clipboard_writes: CLIPBOARD_WRITES.load(Ordering::Relaxed),
        clipboard_bytes: CLIPBOARD_BYTES.load(Ordering::Relaxed) as usize,
    })
}

fn wheel(position: Point<Pixels>) -> ScrollWheelEvent {
    ScrollWheelEvent {
        position,
        delta: ScrollDelta::Lines(Point { x: 0.0, y: -3.0 }),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    }
}

fn reps_from_env() -> usize {
    std::env::var(REPS_ENV)
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(DEFAULT_REPS)
}

async fn await_frames(state: &Rc<Cell<u8>>, wcx: &mut AsyncWindowContext) {
    let deadline = Instant::now() + WAIT_CAP;
    while state.get() < 2 {
        if Instant::now() >= deadline {
            return;
        }
        wcx.background_executor().timer(POLL).await;
    }
}

async fn next_frame(wcx: &mut AsyncWindowContext) {
    let seen = Rc::new(Cell::new(false));
    let flag = Rc::clone(&seen);
    wcx.on_next_frame(move |_, _| flag.set(true));
    let deadline = Instant::now() + WAIT_CAP;
    while !seen.get() {
        if Instant::now() >= deadline {
            return;
        }
        wcx.background_executor().timer(POLL).await;
    }
}

fn dispatch_keys(wcx: &mut AsyncWindowContext, keys: &[&str]) {
    let _ = wcx.update(|window, cx| {
        for source in keys {
            if let Ok(keystroke) = Keystroke::parse(source) {
                window.dispatch_keystroke(keystroke, cx);
            }
        }
    });
}

fn reset_document(
    wcx: &mut AsyncWindowContext,
    shell: &WeakEntity<Shell>,
    source: &str,
    path: &Option<PathBuf>,
) {
    let _ = wcx.update(|_, cx| {
        let Some(shell) = shell.upgrade() else {
            return;
        };
        let editor = shell.read(cx).editor().clone();
        let doc = Doc::with_path(load_markdown(source, editor_options()), path.clone());
        editor.update(cx, |view, cx| {
            view.replace_document(doc, cx);
            cx.notify();
        });
    });
}

async fn run(
    wcx: &mut AsyncWindowContext,
    shell: WeakEntity<Shell>,
    samples: Rc<RefCell<Vec<Sample>>>,
    op_loop_ms: Rc<RefCell<Vec<(usize, f64)>>>,
) {
    wcx.background_executor().timer(START_DELAY).await;
    let reps = reps_from_env();
    let doc = wcx.update(|_, cx| stamp(&shell, cx)).ok().flatten();
    emit(&format!(
        "interact start reps={reps} ops={} doc={doc:?}",
        OPS.len()
    ));

    let origin = wcx
        .update(|_, cx| {
            let shell = shell.upgrade()?;
            let editor = shell.read(cx).editor().clone();
            let view = editor.read(cx);
            Some((
                view.state.doc.document.source.clone(),
                view.state.doc.source_path.clone(),
            ))
        })
        .ok()
        .flatten();
    let Some((source, source_path)) = origin else {
        return;
    };

    let Ok(centre) = wcx.update(|window, _| {
        let bounds = window.bounds();
        Point {
            x: px(f32::from(bounds.size.width) * 0.5),
            y: px(f32::from(bounds.size.height) * 0.5),
        }
    }) else {
        return;
    };

    let _ = wcx.update(|window, cx| {
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: centre,
                pressed_button: None,
                modifiers: Modifiers::none(),
            }),
            cx,
        );
    });
    wcx.background_executor().timer(SETTLE).await;

    for (op_index, op) in OPS.iter().copied().enumerate() {
        let op_started = Instant::now();
        let _ = wcx.update(|_, cx| cx.activate(true));
        for rep in 0..(reps + WARMUP_REPS) {
            let warm = rep < WARMUP_REPS;
            if op.resets() {
                reset_document(wcx, &shell, source.as_ref(), &source_path);
                next_frame(wcx).await;
            }
            for &step in op.setup() {
                dispatch_keys(wcx, &[step]);
                next_frame(wcx).await;
            }
            wcx.background_executor().timer(SETTLE).await;
            let before = wcx.update(|_, cx| stamp(&shell, cx)).ok().flatten();

            let dispatched = wcx.update(|window, cx| {
                let epoch = EPOCH.fetch_add(1, Ordering::Relaxed) + 1;
                let t0 = Instant::now();
                match op {
                    Op::Idle => {}
                    Op::ScrollNotch | Op::ScrollBurst => {
                        for _ in 0..op.notches() {
                            window.dispatch_event(PlatformInput::ScrollWheel(wheel(centre)), cx);
                        }
                    }
                    _ => {
                        for source in op.keys() {
                            if let Ok(keystroke) = Keystroke::parse(source) {
                                window.dispatch_keystroke(keystroke, cx);
                            }
                        }
                    }
                }
                (t0, Instant::now(), epoch)
            });
            let Ok((t0, t_dispatch, epoch)) = dispatched else {
                continue;
            };
            let dispatch_ms = t_dispatch.saturating_duration_since(t0).as_secs_f64() * 1000.0;

            let state = Rc::new(Cell::new(0u8));
            let sink = Rc::clone(&samples);
            let frame_shell = shell.clone();
            let response = Rc::clone(&state);
            wcx.on_next_frame(move |window, _| {
                let t1 = Instant::now();
                let next = Rc::clone(&response);
                let sink = Rc::clone(&sink);
                window.on_next_frame(move |_, cx| {
                    let t2 = Instant::now();
                    let origin = crate::interact_probe::origin_instant();
                    let after = stamp(&frame_shell, cx);
                    if !warm {
                        let rebuilds = match (before, after) {
                            (Some(before), Some(after)) => {
                                after.rebuilds.saturating_sub(before.rebuilds)
                            }
                            _ => 0,
                        };
                        let pre_draw_ms = origin
                            .map(|at| at.saturating_duration_since(t1).as_secs_f64() * 1000.0)
                            .unwrap_or(f64::NAN);
                        let post_draw_ms = origin
                            .map(|at| t2.saturating_duration_since(at).as_secs_f64() * 1000.0)
                            .unwrap_or(f64::NAN);
                        sink.borrow_mut().push(Sample {
                            op: op_index,
                            dispatch_ms,
                            pickup_ms: (t1 - t0).as_secs_f64() * 1000.0,
                            period_ms: (t2 - t1).as_secs_f64() * 1000.0,
                            pre_draw_ms,
                            post_draw_ms,
                            epoch,
                            moved: before.is_some() && after.is_some() && before != after,
                            rebuilds,
                            rev_before: before.map(|s| s.revision).unwrap_or(0),
                            rev_after: after.map(|s| s.revision).unwrap_or(0),
                            clip_writes: match (before, after) {
                                (Some(before), Some(after)) => after
                                    .clipboard_writes
                                    .saturating_sub(before.clipboard_writes),
                                _ => 0,
                            },
                            clip_bytes: match (before, after) {
                                (Some(before), Some(after)) => after
                                    .clipboard_bytes
                                    .saturating_sub(before.clipboard_bytes),
                                _ => 0,
                            },
                        });
                    }
                    next.set(2);
                });
                response.set(1);
            });

            await_frames(&state, wcx).await;
        }
        op_loop_ms
            .borrow_mut()
            .push((op_index, op_started.elapsed().as_secs_f64() * 1000.0));
    }

    wcx.background_executor().timer(SETTLE).await;
    {
        let render = RENDER_END.lock().unwrap_or_else(|e| e.into_inner());
        let prepaint_start = PREPAINT_START.lock().unwrap_or_else(|e| e.into_inner());
        let prepaint_end = PREPAINT_END.lock().unwrap_or_else(|e| e.into_inner());
        let paint_start = PAINT_START.lock().unwrap_or_else(|e| e.into_inner());
        let paint_end = PAINT_END.lock().unwrap_or_else(|e| e.into_inner());
        let paint_dur = PAINT_DUR.lock().unwrap_or_else(|e| e.into_inner());
        let status = STATUS_DUR.lock().unwrap_or_else(|e| e.into_inner());
        let pre_build = PRE_BUILD.lock().unwrap_or_else(|e| e.into_inner());
        let engine = ENGINE_DUR.lock().unwrap_or_else(|e| e.into_inner());
        let assemble = ASSEMBLE_DUR.lock().unwrap_or_else(|e| e.into_inner());
        let geometry = GEOMETRY_DUR.lock().unwrap_or_else(|e| e.into_inner());
        let changes = CHANGES_DUR.lock().unwrap_or_else(|e| e.into_inner());
        let block_edit = BLOCK_EDIT_DUR.lock().unwrap_or_else(|e| e.into_inner());
        let stabilise = STABILISE_DUR.lock().unwrap_or_else(|e| e.into_inner());
        let caret = CARET_DUR.lock().unwrap_or_else(|e| e.into_inner());
        report(
            &samples.borrow(),
            &op_loop_ms.borrow(),
            &render,
            &prepaint_start,
            &prepaint_end,
            &paint_start,
            &paint_end,
            &paint_dur,
            &status,
            &pre_build,
            &engine,
            &assemble,
            &geometry,
            &changes,
            &block_edit,
            &stabilise,
            &caret,
        );
    }
    wcx.background_executor().timer(FLUSH).await;
    std::process::exit(0);
}

fn percentile(values: &mut [f64], p: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let index = (((values.len() - 1) as f64) * p).round() as usize;
    values[index]
}

fn push_stats(out: &mut String, name: &str, values: &mut [f64]) {
    out.push_str(&format!(
        " {name}_p50={:.3} {name}_p95={:.3} {name}_p99={:.3} {name}_max={:.3}",
        percentile(values, 0.5),
        percentile(values, 0.95),
        percentile(values, 0.99),
        percentile(values, 1.0)
    ));
}

fn nth_at(costs: &[Cost], epoch: u64, ordinal: usize) -> Option<f64> {
    costs
        .iter()
        .filter(|cost| cost.epoch == epoch)
        .nth(ordinal)
        .map(|cost| cost.ms)
}

fn marks_for(epoch: u64, spans: &Spans<'_>) -> Option<Marks> {
    let render = nth_at(spans.render, epoch, 0)?;
    let prepaint_start = nth_at(spans.prepaint_start, epoch, 0)?;
    let prepaint_end = nth_at(spans.prepaint_end, epoch, 0)?;
    let paint_start = nth_at(spans.paint_start, epoch, 0)?;
    let paint_end = nth_at(spans.paint_end, epoch, 0)?;
    let paint = nth_at(spans.paint_dur, epoch, 0)?;
    if prepaint_start < render || prepaint_end < prepaint_start || paint_start < prepaint_end {
        return None;
    }
    Some(Marks {
        render,
        to_prepaint: prepaint_start - render,
        prepaint: prepaint_end - prepaint_start,
        gap: paint_start - prepaint_end,
        paint,
        draw: paint_end,
    })
}

struct Spans<'a> {
    render: &'a [Cost],
    prepaint_start: &'a [Cost],
    prepaint_end: &'a [Cost],
    paint_start: &'a [Cost],
    paint_end: &'a [Cost],
    paint_dur: &'a [Cost],
    status: &'a [Cost],
    pre_build: &'a [Cost],
    engine: &'a [Cost],
    assemble: &'a [Cost],
    geometry: &'a [Cost],
    changes: &'a [Cost],
    block_edit: &'a [Cost],
    stabilise: &'a [Cost],
    caret: &'a [Cost],
}

#[allow(clippy::too_many_arguments)]
fn report(
    samples: &[Sample],
    op_loop_ms: &[(usize, f64)],
    render: &[Cost],
    prepaint_start: &[Cost],
    prepaint_end: &[Cost],
    paint_start: &[Cost],
    paint_end: &[Cost],
    paint_dur: &[Cost],
    status: &[Cost],
    pre_build: &[Cost],
    engine: &[Cost],
    assemble: &[Cost],
    geometry: &[Cost],
    changes: &[Cost],
    block_edit: &[Cost],
    stabilise: &[Cost],
    caret: &[Cost],
) {
    let spans = Spans {
        render,
        prepaint_start,
        prepaint_end,
        paint_start,
        paint_end,
        paint_dur,
        status,
        pre_build,
        engine,
        assemble,
        geometry,
        changes,
        block_edit,
        stabilise,
        caret,
    };

    for (op_index, op) in OPS.iter().copied().enumerate() {
        let mut dispatch: Vec<f64> = Vec::new();
        let mut pickup: Vec<f64> = Vec::new();
        let mut period: Vec<f64> = Vec::new();
        let mut render_ms: Vec<f64> = Vec::new();
        let mut to_prepaint_ms: Vec<f64> = Vec::new();
        let mut prepaint_ms: Vec<f64> = Vec::new();
        let mut gap_ms: Vec<f64> = Vec::new();
        let mut paint_ms: Vec<f64> = Vec::new();
        let mut draw_ms: Vec<f64> = Vec::new();
        let mut pre_draw_ms: Vec<f64> = Vec::new();
        let mut after_paint_ms: Vec<f64> = Vec::new();
        let mut status_ms: Vec<f64> = Vec::new();
        let mut pre_build_ms: Vec<f64> = Vec::new();
        let mut engine_ms: Vec<f64> = Vec::new();
        let mut assemble_ms: Vec<f64> = Vec::new();
        let mut geometry_ms: Vec<f64> = Vec::new();
        let mut changes_ms: Vec<f64> = Vec::new();
        let mut block_edit_ms: Vec<f64> = Vec::new();
        let mut stabilise_ms: Vec<f64> = Vec::new();
        let mut caret_ms: Vec<f64> = Vec::new();
        let mut moved = 0usize;
        let mut rebuilt = 0usize;
        let mut marks = 0usize;
        let mut rev_start = None;
        let mut rev_end = None;
        let mut clip_writes = 0u64;
        let mut clip_bytes = 0usize;
        for sample in samples.iter().filter(|sample| sample.op == op_index) {
            dispatch.push(sample.dispatch_ms);
            pickup.push(sample.pickup_ms);
            period.push(sample.period_ms);
            clip_writes += sample.clip_writes;
            clip_bytes = clip_bytes.max(sample.clip_bytes);
            if rev_start.is_none() {
                rev_start = Some(sample.rev_before);
            }
            rev_end = Some(sample.rev_after);
            if sample.moved {
                moved += 1;
            }
            if sample.rebuilds > 0 {
                rebuilt += 1;
            }
            if let Some(mark) = marks_for(sample.epoch, &spans) {
                marks += 1;
                render_ms.push(mark.render);
                to_prepaint_ms.push(mark.to_prepaint);
                prepaint_ms.push(mark.prepaint);
                gap_ms.push(mark.gap);
                paint_ms.push(mark.paint);
                draw_ms.push(mark.draw);
                if sample.post_draw_ms.is_finite() {
                    after_paint_ms.push(sample.post_draw_ms - mark.draw);
                }
            }
            if sample.pre_draw_ms.is_finite() {
                pre_draw_ms.push(sample.pre_draw_ms);
            }
            if let Some(ms) = nth_at(spans.status, sample.epoch, 0) {
                status_ms.push(ms);
            }
            if let Some(ms) = nth_at(spans.pre_build, sample.epoch, 0) {
                pre_build_ms.push(ms);
            }
            if let Some(ms) = nth_at(spans.engine, sample.epoch, 0) {
                engine_ms.push(ms);
            }
            if let Some(ms) = nth_at(spans.assemble, sample.epoch, 0) {
                assemble_ms.push(ms);
            }
            if let Some(ms) = nth_at(spans.geometry, sample.epoch, 0) {
                geometry_ms.push(ms);
            }
            if let Some(ms) = nth_at(spans.changes, sample.epoch, 0) {
                changes_ms.push(ms);
            }
            if let Some(ms) = nth_at(spans.block_edit, sample.epoch, 0) {
                block_edit_ms.push(ms);
            }
            if let Some(ms) = nth_at(spans.stabilise, sample.epoch, 0) {
                stabilise_ms.push(ms);
            }
            if let Some(ms) = nth_at(spans.caret, sample.epoch, 0) {
                caret_ms.push(ms);
            }
        }

        let reps = pickup.len();
        let loop_ms = op_loop_ms
            .iter()
            .find(|(index, _)| *index == op_index)
            .map(|(_, ms)| *ms)
            .unwrap_or(f64::NAN);
        let mut line = format!(
            "interact op={} reps={reps} moved={moved}/{reps} rebuilt={rebuilt}/{reps} loop_ms={loop_ms:.0} rev={}->{} clip={clip_writes}w/{clip_bytes}b",
            op.name(),
            rev_start.unwrap_or(0),
            rev_end.unwrap_or(0),
        );
        push_stats(&mut line, "dispatch", &mut dispatch);
        push_stats(&mut line, "pickup", &mut pickup);
        push_stats(&mut line, "period", &mut period);
        push_stats(&mut line, "pre_draw", &mut pre_draw_ms);
        push_stats(&mut line, "render", &mut render_ms);
        push_stats(&mut line, "to_prepaint", &mut to_prepaint_ms);
        push_stats(&mut line, "prepaint", &mut prepaint_ms);
        push_stats(&mut line, "gap", &mut gap_ms);
        push_stats(&mut line, "paint", &mut paint_ms);
        push_stats(&mut line, "draw", &mut draw_ms);
        push_stats(&mut line, "after_paint", &mut after_paint_ms);
        push_stats(&mut line, "status", &mut status_ms);
        push_stats(&mut line, "pre_build", &mut pre_build_ms);
        push_stats(&mut line, "engine", &mut engine_ms);
        push_stats(&mut line, "assemble", &mut assemble_ms);
        push_stats(&mut line, "geometry", &mut geometry_ms);
        push_stats(&mut line, "changes", &mut changes_ms);
        push_stats(&mut line, "block_edit", &mut block_edit_ms);
        push_stats(&mut line, "stabilise", &mut stabilise_ms);
        push_stats(&mut line, "caret", &mut caret_ms);
        emit(&format!(
            "{line} frames={reps} marks={marks} status={}",
            status_ms.len()
        ));
    }
}
