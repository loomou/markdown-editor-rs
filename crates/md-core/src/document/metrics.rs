#[cfg(feature = "load-metrics")]
use std::cell::RefCell;

#[cfg(feature = "load-metrics")]
thread_local! {
    static COUNTERS: RefCell<[u64; 4]> = const { RefCell::new([0; 4]) };
}

#[cfg(feature = "load-metrics")]
fn bump(slot: usize) {
    COUNTERS.with(|c| c.borrow_mut()[slot] += 1);
}

#[cfg(feature = "load-metrics")]
pub fn snapshot() -> (u64, u64, u64, u64) {
    COUNTERS.with(|c| {
        let c = c.borrow();
        (c[0], c[1], c[2], c[3])
    })
}

#[cfg(feature = "load-metrics")]
pub fn reset() {
    COUNTERS.with(|c| *c.borrow_mut() = [0; 4]);
}

#[cfg(feature = "load-metrics")]
pub(crate) fn note_parser() {
    bump(0);
}

#[cfg(feature = "load-metrics")]
pub(crate) fn note_leaf() {
    bump(1);
}

#[cfg(feature = "load-metrics")]
pub(crate) fn note_construct() {
    bump(3);
}

#[cfg(not(feature = "load-metrics"))]
pub(crate) fn note_parser() {}

#[cfg(not(feature = "load-metrics"))]
pub(crate) fn note_leaf() {}

#[cfg(not(feature = "load-metrics"))]
pub(crate) fn note_construct() {}

#[cfg(all(test, feature = "load-metrics"))]
mod tests {
    use super::*;

    #[test]
    fn counters_are_per_thread() {
        reset();
        note_parser();
        std::thread::spawn(|| {
            reset();
            note_parser();
            note_parser();
            let (parser, _, _, _) = snapshot();
            assert_eq!(parser, 2, "the child thread only sees its own two counts");
        })
        .join()
        .unwrap();
        let (parser, _, _, _) = snapshot();
        assert_eq!(
            parser, 1,
            "the main thread's count is untouched by the child's reset/bump"
        );
    }
}
