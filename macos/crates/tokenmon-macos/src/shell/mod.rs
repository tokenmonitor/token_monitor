//! AppKit, Slint and WebKit glue (macOS only). Decisions are in the safe
//! modules of the library; this wires them to the OS.

pub mod appkit_status;
pub mod bitmap;
pub mod panel;
pub mod status_item;
mod ui;

use std::process::ExitCode;

use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;
use tokenmon_core::ProviderId;
use tokenmon_ui::tray_icon::BarSegment;

use crate::options::Options;
use crate::skia_status::{self, Chrome};
use crate::{bundle, compare};

/// Entry point of the macOS build.
pub fn run(args: &[String]) -> ExitCode {
    let options = match Options::parse(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("tokenmon-macos: {e}");
            return ExitCode::from(2);
        }
    };
    if options.print_info_plist {
        print!("{}", bundle::info_plist(env!("CARGO_PKG_VERSION")));
        return ExitCode::SUCCESS;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("tokenmon-macos must run on the main thread");
        return ExitCode::FAILURE;
    };
    if let Some(dir) = &options.render_status {
        return render_status_samples(mtm, dir);
    }
    ui::run(mtm, &options)
}

/// The segments the side-by-side comparison draws.
fn sample_segments() -> Vec<BarSegment> {
    vec![
        BarSegment {
            provider: ProviderId::Claude,
            used: Some(63.0),
        },
        BarSegment {
            provider: ProviderId::Grok,
            used: Some(7.0),
        },
        BarSegment {
            provider: ProviderId::OpenRouter,
            used: None,
        },
    ]
}

/// Writes `status-{skia,appkit}-{light,dark}-{1x,2x}.png` into `dir`: the
/// artifacts of the renderer comparison.
fn render_status_samples(mtm: MainThreadMarker, dir: &std::path::Path) -> ExitCode {
    let _ = NSApplication::sharedApplication(mtm);
    if std::fs::create_dir_all(dir).is_err() {
        eprintln!("cannot create {}", dir.display());
        return ExitCode::FAILURE;
    }
    let segments = sample_segments();
    for (chrome, chrome_name, backdrop) in [
        (Chrome::Light, "light", [236, 236, 240]),
        (Chrome::Dark, "dark", [38, 38, 42]),
    ] {
        for scale in [1_u32, 2] {
            let skia = skia_status::render(&segments, chrome, scale).to_png();
            let appkit = appkit_status::render(&segments, chrome, scale as usize)
                .and_then(|r| bitmap::png_of(&r.rep));
            let _ = std::fs::write(
                dir.join(format!("status-skia-{chrome_name}-{scale}x.png")),
                &skia,
            );
            if let Some(png) = &appkit {
                let _ = std::fs::write(
                    dir.join(format!("status-appkit-{chrome_name}-{scale}x.png")),
                    png,
                );
                // Top: tiny-skia, bottom: AppKit; enlarged so pixels are visible.
                if let Some(both) = compare::stack(&[skia, png.clone()], backdrop, 8 / scale, 6) {
                    let _ = std::fs::write(
                        dir.join(format!("compare-{chrome_name}-{scale}x.png")),
                        both,
                    );
                }
            }
        }
    }
    ExitCode::SUCCESS
}
