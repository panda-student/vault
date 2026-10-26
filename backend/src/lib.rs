//! Tauri-independent credential backend. Public consumers use api and bootstrap.
pub mod adapters;
pub mod api;
pub mod bootstrap;
mod common;
mod data;
mod definitions;
mod platform;
mod services;
