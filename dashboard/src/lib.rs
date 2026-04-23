#![forbid(unsafe_code)]

pub mod app;
pub mod config;
pub mod fmt;
pub mod ui;
pub mod ws_client;

pub use crate::app::{App, AppEvent, ConnectionState, Flow, Liveness};
pub use crate::config::{DashboardConfig, ReconnectPolicy, Thresholds};
