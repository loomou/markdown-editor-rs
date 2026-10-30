pub mod corpus;
pub mod gpui_support;
pub mod harness;
pub mod mem;
pub mod scroll_probe;

#[global_allocator]
static ALLOCATOR: mem::CountingAllocator = mem::CountingAllocator;
