#![forbid(unsafe_code)]

pub mod app;
pub mod config;
pub mod ui;
pub mod ws_client;

pub use crate::app::{App, AppEvent, ConnectionState, Flow};
pub use crate::config::{DashboardConfig, ReconnectPolicy};
