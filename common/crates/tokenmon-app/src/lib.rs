//! TokenMon's composition root, shared by the desktop ports.
//!
//! Everything the executable does lives in this library so it can be driven
//! hermetically with fakes: the [`registry`] of providers, the [`model`] that
//! owns lifecycle behaviour (menu open, sign-in completion, wake handling,
//! settings-driven polling, quit), [`alerts`] for threshold notifications and
//! [`smoke`] for the headless CI run. Each port's binary crate
//! (`tokenmon-windows`, ...) only parses the command line and connects these
//! modules to the OS shell.

pub mod alerts;
pub mod app;
pub mod daily;
pub mod flush;
pub mod logging;
pub mod model;
pub mod observe;
pub mod overview;
pub mod panel;
pub mod panel_host;
pub mod placement;
pub mod poller;
pub mod providers;
pub mod registry;
pub mod session;
pub mod sessions;
pub mod settings_host;
pub mod signin;
pub mod smoke;
pub mod sources;
pub mod startup;
pub mod testing;
pub mod tray;
pub mod updates;
