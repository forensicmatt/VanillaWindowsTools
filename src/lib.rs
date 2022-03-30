/// Import macros for logging (debug!, info!, error!, etc.)
#[macro_use] extern crate log;
/// Module for indexing operations
pub mod index;
/// Module for REST service helpers/operations
pub mod service;
/// Custom errors
pub mod error;
/// Custom tokenizer for indexing
pub mod tokenizer;
/// VanillaWindowsReference helpers/operations
pub mod vanilla;