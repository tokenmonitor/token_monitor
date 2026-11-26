//! Port of `ProviderSwitcherLayoutTests.testGridEdgesMatchCardEdgesForEveryProviderCount`:
//! the tab grid's outer border lines up with a panel card's edges at the panel
//! content width (396), for every provider count and display scale. Rendered
//! offscreen with Slint's software renderer.
//!
//! Slint platform state is per process, so everything runs in one test.
#![allow(missing_docs, clippy::unwrap_used)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::fontique_011::{fontique, shared_collection};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, PhysicalSize, Rgb8Pixel};
use tokenmon_core::ProviderId;
use tokenmon_ui::panels::bridge::tab_rows;
use tokenmon_ui::panels::switcher::build_switcher;
use tokenmon_ui::panels::ui;

const MARGIN: f32 = 8.0;
const WIDTH: u32 = 412;

type Last = Rc<RefCell<Option<Rc<MinimalSoftwareWindow>>>>;

struct TestPlatform {
    last: Last,
}

impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let w = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        *self.last.borrow_mut() = Some(w.clone());
        Ok(w)
    }
}

/// Leftmost and rightmost drawn x, in logical px relative to the content box
/// (the first differing pixel to the last, over every row).
fn ink_extent<C: ComponentHandle>(comp: &C, last: &Last, height: u32, scale: f32) -> (f32, f32) {
    comp.show().unwrap();
    let win = last.borrow().clone().unwrap();
    win.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    let (pw, ph) = (
        (WIDTH as f32 * scale) as u32,
        (height as f32 * scale) as u32,
    );
    win.set_size(PhysicalSize::new(pw, ph));
    slint::platform::update_timers_and_animations();
    let mut buf = vec![Rgb8Pixel::default(); (pw * ph) as usize];
    win.draw_if_needed(|r| {
        r.render(&mut buf, pw as usize);
    });
    let bg = buf[0];
    let (mut min, mut max) = (usize::MAX, 0);
    for row in buf.chunks(pw as usize) {
        for (x, p) in row.iter().enumerate() {
            if (p.r, p.g, p.b) != (bg.r, bg.g, bg.b) {
                min = min.min(x);
                max = max.max(x + 1);
            }
        }
    }
    assert!(min < max, "nothing drawn");
    (min as f32 / scale - MARGIN, max as f32 / scale - MARGIN)
}

#[test]
fn test_grid_edges_match_card_edges_for_every_provider_count() {
    let last: Last = Rc::new(RefCell::new(None));
    slint::platform::set_platform(Box::new(TestPlatform { last: last.clone() })).unwrap();
    let mut fonts = shared_collection();
    for bytes in [
        &include_bytes!("fonts/Inter-Regular.ttf")[..],
        &include_bytes!("fonts/Inter-SemiBold.ttf")[..],
    ] {
        fonts.register_fonts(fontique::Blob::new(Arc::new(bytes.to_vec())), None);
    }

    let mut checked = 0;
    for scale in [1.0_f32, 2.0] {
        let card = {
            let w = ui::SnapCardEdges::new().unwrap();
            w.global::<ui::Typo>().set_family("Inter".into());
            ink_extent(&w, &last, 80, scale)
        };
        // [overview] + the usage providers: 1..=8 tabs.
        for count in 1..=ProviderId::USAGE_PROVIDERS.len() + 1 {
            let rows = build_switcher(
                &ProviderId::USAGE_PROVIDERS[..count - 1],
                ProviderId::Overview,
            );
            let height = 16 + 28 * rows.len() as u32;
            let w = ui::SnapSwitcherEdges::new().unwrap();
            w.global::<ui::Typo>().set_family("Inter".into());
            w.set_rows(tab_rows(&rows));
            let grid = ink_extent(&w, &last, height, scale);
            assert!(
                (grid.0 - card.0).abs() < 0.01,
                "left edge, {count} tabs @{scale}x: {} vs {}",
                grid.0,
                card.0
            );
            assert!(
                (grid.1 - card.1).abs() < 0.01,
                "right edge, {count} tabs @{scale}x: {} vs {}",
                grid.1,
                card.1
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 16, "1..=8 tabs at two scales");
}
