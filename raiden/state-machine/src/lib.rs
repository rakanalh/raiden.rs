#![warn(clippy::missing_docs_in_private_items)]

/// State machine constants.
pub mod constants;
/// State machine errors
pub mod errors;
/// State machine transitioners..
pub mod machine;
/// State machine storage.
#[cfg(feature = "storage")]
pub mod storage;
#[cfg(test)]
pub mod tests;
/// State machine types.
pub mod types;
/// State machine views.
pub mod views;
