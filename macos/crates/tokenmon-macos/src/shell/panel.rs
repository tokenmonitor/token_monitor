//! The Slint panel window and its AppKit tuning (route 1 of the M0 panel
//! decision: the winit window Slint creates, tuned through objc2).
//!
//! Tuning: status-bar window level so it floats above normal windows,
//! "can join all spaces" + "full screen auxiliary" so it can open over a
//! full-screen app and on whichever Space is active, and an explicit
//! activate + make-key so focus loss (`Focused(false)`) can close it.
#![allow(unsafe_code)]

use std::rc::Rc;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSApplication, NSView, NSWindow, NSWindowCollectionBehavior};
use slint::ComponentHandle;
use slint::winit_030::{WinitWindowAccessor, winit};
use tokenmon_app::panel::PanelView;
use tokenmon_core::AppTheme;
use tokenmon_ui::panels::switcher::RowVm;
use tokenmon_ui::panels::{bridge, events, ui};

/// `NSStatusWindowLevel`.
const STATUS_WINDOW_LEVEL: isize = 25;

/// Panel width in logical pixels (`PanelWindow` in the UI crate).
pub const PANEL_WIDTH: f32 = 420.0;

/// The one panel window.
pub struct PanelWindow {
    window: ui::PanelWindow,
}

impl PanelWindow {
    /// Creates the window and routes its actions to `handler`.
    pub fn create(handler: &events::PanelHandler) -> Result<Self, slint::PlatformError> {
        let window = ui::PanelWindow::new()?;
        events::connect_all(&window, handler);
        Ok(Self { window })
    }

    /// The Slint window.
    pub fn window(&self) -> &slint::Window {
        self.window.window()
    }

    /// Whether it is on screen.
    pub fn is_visible(&self) -> bool {
        self.window().is_visible()
    }

    /// The layout's preferred size in logical pixels.
    pub fn logical_size(&self) -> (f32, f32) {
        (PANEL_WIDTH, self.window.get_content_height())
    }

    /// Caps the height; taller content scrolls inside the window.
    pub fn set_max_height(&self, dip: f32) {
        self.window.set_panel_max_height(dip);
    }

    /// Light or dark palette.
    pub fn set_theme(&self, theme: AppTheme) {
        bridge::apply_theme(&self.window, theme);
    }

    /// Shows the window.
    pub fn show(&self) {
        let _ = self.window.show();
    }

    /// Hides the window.
    pub fn hide(&self) {
        let _ = self.window.hide();
    }

    /// Fills the window from `view` and selects its body.
    pub fn apply(&self, view: &PanelView, rows: &[RowVm]) {
        let w = &self.window;
        w.set_tabs(bridge::tab_rows(rows));
        match view {
            PanelView::Overview(v) => bridge::apply_overview(&w.global::<ui::OverviewVm>(), v),
            PanelView::Grok(v) => bridge::apply_grok(&w.global::<ui::GrokVm>(), v),
            PanelView::Grokbot(v) => bridge::apply_grokbot(&w.global::<ui::GrokbotVm>(), v),
            PanelView::Cursor(v) => bridge::apply_cursor(&w.global::<ui::CursorVm>(), v),
            PanelView::Claude(v) => bridge::apply_claude(&w.global::<ui::ClaudeVm>(), v),
            PanelView::ChatGpt(v) => bridge::apply_chatgpt(&w.global::<ui::ChatGptVm>(), v),
            PanelView::OpenCode(v) => bridge::apply_opencode(&w.global::<ui::OpenCodeVm>(), v),
            PanelView::OpenRouter(v) => {
                bridge::apply_openrouter(&w.global::<ui::OpenRouterVm>(), v);
            }
        }
        w.set_selected(bridge::provider_tab(view.provider()));
    }

    /// Registers `on_focus` for focus changes and `on_escape` for Escape.
    pub fn watch(&self, on_focus: Rc<dyn Fn(bool)>, on_escape: Rc<dyn Fn()>) {
        use slint::winit_030::EventResult;
        self.window().on_winit_window_event(move |_, event| {
            use winit::event::WindowEvent;
            use winit::keyboard::{Key, NamedKey};
            match event {
                WindowEvent::Focused(focused) => on_focus(*focused),
                WindowEvent::KeyboardInput { event, .. }
                    if event.state.is_pressed()
                        && event.logical_key == Key::Named(NamedKey::Escape) =>
                {
                    on_escape();
                }
                _ => {}
            }
            EventResult::Propagate
        });
    }

    /// The backing `NSWindow`, once the winit window exists.
    pub fn ns_window(&self) -> Option<Retained<NSWindow>> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        self.window().with_winit_window(|w| {
            let RawWindowHandle::AppKit(handle) = w.window_handle().ok()?.as_raw() else {
                return None;
            };
            // SAFETY: winit guarantees `ns_view` is a live `NSView` for as
            // long as the window exists, and we only retain it on the main
            // thread inside this callback.
            let view: Retained<NSView> =
                unsafe { Retained::retain(handle.ns_view.as_ptr().cast()) }?;
            view.window()
        })?
    }

    /// The native window frame in AppKit coordinates, for diagnostics.
    pub fn frame_description(&self) -> Option<(f64, f64, f64, f64)> {
        let f = self.ns_window()?.frame();
        Some((f.origin.x, f.origin.y, f.size.width, f.size.height))
    }

    /// Applies the level / Spaces tuning, then activates the app and makes
    /// the panel key so it receives focus-loss events.
    pub fn tune_and_focus(&self, mtm: MainThreadMarker) -> bool {
        let Some(ns_window) = self.ns_window() else {
            return false;
        };
        ns_window.setLevel(STATUS_WINDOW_LEVEL);
        ns_window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        ns_window.setHidesOnDeactivate(false);
        #[allow(deprecated)]
        NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
        ns_window.makeKeyAndOrderFront(None);
        true
    }
}
