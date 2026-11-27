//! Command-line options of the spike binary (developer and benchmark knobs;
//! the shipped beta will have none of them).

use std::path::PathBuf;

use crate::skia_status::Chrome;

/// Which renderer draws the status-item image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusRenderer {
    /// The shared tiny-skia renderer.
    #[default]
    Skia,
    /// AppKit / Core Text through objc2.
    AppKit,
}

/// Parsed command line.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Options {
    /// Print the `Info.plist` for the bundling script and exit.
    pub print_info_plist: bool,
    /// Render both status renderers into this directory and exit.
    pub render_status: Option<PathBuf>,
    /// Serve a loopback OpenRouter mock and use it with a dummy key.
    pub mock_openrouter: bool,
    /// Open the panel this many seconds after start.
    pub open_panel_after: Option<f64>,
    /// Open the Settings window this many seconds after start.
    pub open_settings_after: Option<f64>,
    /// Quit this many seconds after start.
    pub quit_after: Option<f64>,
    /// Draw the status image for this chrome instead of following the system.
    pub force_chrome: Option<Chrome>,
    /// The status-item renderer.
    pub renderer: StatusRenderer,
    /// Do not read the Swift app's Keychain vault (no Keychain prompt).
    pub no_keychain: bool,
    /// Keep the panel open when it loses focus (for screenshots).
    pub keep_open: bool,
    /// Open the panel on this provider's tab (a provider id).
    pub panel_tab: Option<String>,
}

/// A command-line problem, naming the offending flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsError(pub String);

impl std::fmt::Display for OptionsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OptionsError {}

impl Options {
    /// Parses `args` (the first element is the program name).
    ///
    /// ```
    /// use tokenmon_macos::options::{Options, StatusRenderer};
    /// let args: Vec<String> = ["x", "--mock-openrouter", "--renderer", "appkit"]
    ///     .iter().map(ToString::to_string).collect();
    /// let o = Options::parse(&args).unwrap();
    /// assert!(o.mock_openrouter);
    /// assert_eq!(o.renderer, StatusRenderer::AppKit);
    /// ```
    pub fn parse(args: &[String]) -> Result<Self, OptionsError> {
        let mut options = Self::default();
        let mut it = args.iter().skip(1);
        while let Some(flag) = it.next() {
            let mut value = |name: &str| {
                it.next()
                    .cloned()
                    .ok_or_else(|| OptionsError(format!("{name} needs a value")))
            };
            match flag.as_str() {
                "--print-info-plist" => options.print_info_plist = true,
                "--mock-openrouter" => options.mock_openrouter = true,
                "--keep-open" => options.keep_open = true,
                "--no-keychain" => options.no_keychain = true,
                "--render-status" => {
                    options.render_status = Some(PathBuf::from(value("--render-status")?));
                }
                "--open-panel-after" => {
                    options.open_panel_after = Some(seconds(
                        "--open-panel-after",
                        &value("--open-panel-after")?,
                    )?);
                }
                "--open-settings-after" => {
                    options.open_settings_after = Some(seconds(
                        "--open-settings-after",
                        &value("--open-settings-after")?,
                    )?);
                }
                "--quit-after" => {
                    options.quit_after = Some(seconds("--quit-after", &value("--quit-after")?)?);
                }
                "--chrome" => {
                    options.force_chrome = Some(match value("--chrome")?.as_str() {
                        "light" => Chrome::Light,
                        "dark" => Chrome::Dark,
                        other => {
                            return Err(OptionsError(format!("--chrome: unknown value {other}")));
                        }
                    });
                }
                "--renderer" => {
                    options.renderer = match value("--renderer")?.as_str() {
                        "skia" => StatusRenderer::Skia,
                        "appkit" => StatusRenderer::AppKit,
                        other => {
                            return Err(OptionsError(format!("--renderer: unknown value {other}")));
                        }
                    };
                }
                "--panel-tab" => options.panel_tab = Some(value("--panel-tab")?),
                other => return Err(OptionsError(format!("unknown option {other}"))),
            }
        }
        Ok(options)
    }
}

fn seconds(name: &str, raw: &str) -> Result<f64, OptionsError> {
    raw.parse::<f64>()
        .ok()
        .filter(|s| s.is_finite() && *s >= 0.0)
        .ok_or_else(|| OptionsError(format!("{name}: expected seconds, got {raw}")))
}
