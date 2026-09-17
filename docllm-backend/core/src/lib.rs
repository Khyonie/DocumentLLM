pub mod adapters;
pub mod chat;
pub mod database;
pub mod ingest;
pub mod llm;
pub mod model;
pub mod query;

// Utilities

/// Like `format!()` but for compile-time loaded-strings, like those from `include_str!()`
#[macro_export]
macro_rules! interpolate_str {
    ($template:expr, $($name:ident = $value:expr),* $(,)?) => {{
        let mut result = $template.to_owned();

        $(
            result = result.replace(
                concat!("{", stringify!($name), "}"), &$value.to_string()
            );
        )*

        result
    }};
}
