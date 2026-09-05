mod build;
pub(crate) mod caret;
mod chrome;
mod decoration;
mod geometry;
mod inline_code;
mod probe;
mod request;
mod rows;
mod search_rects;
mod selection;

#[cfg(test)]
mod tests;

pub use build::{compose, from_assembly};
pub use caret::caret_logical;
pub use request::{FrameContext, FrameRequest};
pub use selection::{caret_logical_y, selection_vertical_span};
