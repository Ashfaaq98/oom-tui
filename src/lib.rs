//! OOM-killer forensics for Linux.
//!
//! Split out as a library so the parser can be exercised directly by fuzzers
//! and integration tests, which cannot reach into a binary-only crate.

pub mod app;
pub mod container;
pub mod model;
pub mod parser;
pub mod report;
pub mod source;
pub mod system;
pub mod timestamp;
pub mod ui;
