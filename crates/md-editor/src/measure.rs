use std::sync::OnceLock;

static ENABLED: OnceLock<bool> = OnceLock::new();

pub fn enabled() -> bool {
    cfg!(feature = "bench-support")
        && *ENABLED.get_or_init(|| std::env::var_os("MARKDOWN_EDITOR_RS_MEASURE").is_some())
}

pub fn emit(line: std::fmt::Arguments<'_>) {
    if enabled() {
        eprintln!("md-measure {line}");
    }
}

#[macro_export]
macro_rules! measure {
    ($($arg:tt)*) => {
        if $crate::measure::enabled() {
            $crate::measure::emit(format_args!($($arg)*));
        }
    };
}
