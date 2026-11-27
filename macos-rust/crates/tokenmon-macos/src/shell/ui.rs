//! The spike's application: wires the shared `tokenmon_app::App` (providers,
//! polling, view models) to a status item and the Slint panel.
//!
//! Threads: a tokio runtime runs polling and IO; the Slint/winit event loop
//! owns the main thread, the status item and the panel.
#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::process::ExitCode;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSScreen};
use slint::ComponentHandle;
use tokenmon_app::app::{App, AppInputs};
use tokenmon_app::overview::{OverviewStore, record_live};
use tokenmon_app::panel::{Action, PanelState, PanelView, ViewEnv, build_view, commit_api_key};
use tokenmon_app::settings_host::{AppHost, AppSettingsBackend, HostParts};
use tokenmon_app::tray::{MenuCommand, context_menu, menu_command};
use tokenmon_app::updates::UpdateService;
use tokenmon_core::alerts::{Notifier, ThresholdAlert};
use tokenmon_core::storage::AppPaths;
use tokenmon_core::update::ReleaseSource;
use tokenmon_core::{AppTheme, Clock, ProviderId, SystemClock};
use tokenmon_platform::secrets::Vault;
use tokenmon_platform::webauth::FakeWebAuthHost;
use tokenmon_provider_opencode::db::DataDirEnv;
use tokenmon_ui::panels::events::{PanelEvent, PanelHandler};
use tokenmon_ui::panels::overview::OverviewCardVm;
use tokenmon_ui::panels::switcher::{RowVm, build_switcher};
use tokenmon_ui::settings::bind::{
    SettingsWindow, apply_theme, create_settings_window, refresh, select_opening_tab,
};
use tokenmon_ui::settings::{NativeSaveDialog, SettingsViewModel};
use tokenmon_ui::tray_icon::MenuEntry;
use tokenmon_ui::tray_icon::{BarSegment, TrayContent, TrayInputs};

use super::keychain_mac::MacKeychain;
use super::panel::PanelWindow;
use super::status_item::{MenuRow, StatusItem};
use super::{appkit_status, bitmap};
use crate::bundle;
use crate::credentials::{self, LoadedKey};
use crate::keychain::seed_from_swift_vault;
use crate::mock::MockOpenRouter;
use crate::options::{Options, StatusRenderer};
use crate::placement::place_under;
use crate::prefs::SwiftPrefs;
use crate::secrets::EphemeralEncryptor;
use crate::services::{BetaDesktopVerb, BetaInstaller, BetaLoginItem, HttpsOpener};
use crate::skia_status::{self, Chrome};

/// How often the UI tick runs (status image, live panel).
const TICK: Duration = Duration::from_secs(2);

/// A focus loss this soon before a status-item click means that click is what
/// took focus away: it should close the panel, not reopen it.
const REOPEN_GUARD: Duration = Duration::from_millis(400);

struct LogNotifier;

impl Notifier for LogNotifier {
    fn deliver(&self, alert: &ThresholdAlert) {
        tracing::info!(provider = %alert.provider, "threshold alert (no notification backend in M0)");
    }
}

struct Ui {
    mtm: MainThreadMarker,
    options: Options,
    app: Rc<App>,
    store: Arc<OverviewStore>,
    version: String,
    status: RefCell<Option<StatusItem>>,
    panel: RefCell<Option<Rc<PanelWindow>>>,
    state: RefCell<PanelState>,
    rows: RefCell<Vec<RowVm>>,
    cards: RefCell<Vec<OverviewCardVm>>,
    last_status_key: RefCell<String>,
    focus_lost_at: Cell<Option<Instant>>,
    /// The panel has been key since it was last shown (winit reports a
    /// spurious `Focused(false)` while the window opens).
    was_focused: Cell<bool>,
    settings_vm: Arc<SettingsViewModel>,
    settings_window: RefCell<Option<Rc<SettingsWindow>>>,
    menu_entries: RefCell<Vec<MenuEntry>>,
    tick: slint::Timer,
    me: RefCell<Weak<Ui>>,
}

fn env_lookup(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn read_file(path: &std::path::Path) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}

/// Runs the app; returns when the event loop ends.
pub fn run(mtm: MainThreadMarker, options: &Options) -> ExitCode {
    init_tracing();
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        eprintln!("HOME is not set");
        return ExitCode::FAILURE;
    };
    // The mock and its base-URL variable come first: the variable is read when
    // the registry is built, and `set_var` is only sound before other threads.
    let mock = if options.mock_openrouter {
        match MockOpenRouter::start() {
            Ok(m) => {
                // SAFETY: no other thread exists yet (the runtime is built after this).
                unsafe {
                    std::env::set_var(
                        tokenmon_app::providers::OPENROUTER_BASE_URL_ENV,
                        m.base_url(),
                    );
                }
                Some(m)
            }
            Err(e) => {
                eprintln!("mock server failed: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        None
    };
    let key = match key_for_run(options.mock_openrouter) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("tokenmon-macos: {e}");
            return ExitCode::FAILURE;
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    let _enter = runtime.enter();
    let encryptor = match EphemeralEncryptor::random() {
        Ok(e) => Arc::new(e),
        Err(e) => {
            eprintln!("no entropy for the vault key: {e}");
            return ExitCode::FAILURE;
        }
    };
    let paths = AppPaths::new(bundle::data_base_dir(&home));
    let Ok(dir) = paths.directory() else {
        eprintln!("cannot create the data directory");
        return ExitCode::FAILURE;
    };
    // The vault key lives one process: a blob from an earlier run can never be
    // decrypted again and would leave the vault "unavailable".
    for stale in ["vault.bin", "vault_pending_removals.json"] {
        let _ = std::fs::remove_file(dir.join(stale));
    }
    let real = !options.mock_openrouter;
    if real && !options.no_keychain {
        seed_vault_from_swift(&dir, &encryptor);
    }
    let version = env!("CARGO_PKG_VERSION").to_owned();
    let app = match App::build(
        AppInputs {
            paths,
            encryptor: encryptor.clone(),
            web_host: Arc::new(FakeWebAuthHost::default()),
            notifier: Arc::new(LogNotifier),
            clock: Arc::new(SystemClock),
            version: version.clone(),
            // Real mode reads local usage logs (Claude/Codex/OpenCode) like the
            // Swift app; the mock run stays isolated from the developer's data.
            home: real.then(|| home.clone()),
            claude_config_dir: real
                .then(|| std::env::var("CLAUDE_CONFIG_DIR").ok())
                .flatten(),
            opencode_env: if real {
                DataDirEnv::from_process()
            } else {
                DataDirEnv::default()
            },
        },
        runtime.handle().clone(),
    ) {
        Ok(a) => Rc::new(a),
        Err(e) => {
            eprintln!("app assembly failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    if real {
        import_swift_prefs_once(&dir, &app);
        if let Some(p) = options
            .panel_tab
            .as_deref()
            .and_then(|t| t.parse::<ProviderId>().ok())
        {
            app.settings.set_selected_provider(p);
        }
    } else {
        configure_settings(&app, options.panel_tab.as_deref());
    }
    seed_key(&app, key.as_ref());
    let _tasks = app.launch();
    let updates = match UpdateService::with_http_feed(
        ReleaseSource::new("macos-rust-v", "TokenMon-RustBeta"),
        &app.settings,
        &version,
        Arc::new(HttpsOpener::new(Arc::new(|url| {
            let _ = std::process::Command::new("open").arg(url).spawn();
        }))),
        Arc::new(BetaInstaller),
    ) {
        Ok(u) => Arc::new(u),
        Err(e) => {
            eprintln!("update service: {e}");
            return ExitCode::FAILURE;
        }
    };
    let settings_vm = settings_view_model(&app, &updates, &version, runtime.handle());

    let selected = app.settings.snapshot().selected_provider;
    let ui = Rc::new(Ui {
        mtm,
        options: options.clone(),
        store: Arc::new(OverviewStore::open(&dir)),
        app,
        version,
        status: RefCell::new(None),
        panel: RefCell::new(None),
        state: RefCell::new(PanelState::new(selected)),
        rows: RefCell::new(Vec::new()),
        cards: RefCell::new(Vec::new()),
        last_status_key: RefCell::new(String::new()),
        focus_lost_at: Cell::new(None),
        was_focused: Cell::new(false),
        settings_vm,
        settings_window: RefCell::new(None),
        menu_entries: RefCell::new(Vec::new()),
        tick: slint::Timer::default(),
        me: RefCell::new(Weak::new()),
    });
    *ui.me.borrow_mut() = Rc::downgrade(&ui);

    let boot = Rc::clone(&ui);
    slint::Timer::single_shot(Duration::ZERO, move || boot.start());
    let result = slint::run_event_loop_until_quit();
    ui.app.model.quit();
    drop(ui);
    runtime.shutdown_timeout(Duration::from_secs(2));
    if let Some(m) = &mock {
        println!(
            "mock-openrouter: requests={} bearer_seen={}",
            m.requests(),
            m.saw_bearer()
        );
    }
    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("TOKENMON_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .try_init();
}

/// The key to seed: a dummy for the mock, else the one entered by hand.
fn key_for_run(mock: bool) -> Result<Option<LoadedKey>, credentials::CredentialError> {
    if mock {
        // Not a secret: the mock accepts any bearer token.
        return Ok(Some(credentials::LoadedKey::mock(
            "sk-or-v1-mock-not-a-secret",
        )));
    }
    credentials::load_openrouter_key(&env_lookup, &read_file)
}

fn configure_settings(app: &App, tab: Option<&str>) {
    let only: BTreeSet<ProviderId> = [ProviderId::OpenRouter].into_iter().collect();
    let _ = app.settings.set_enabled_providers(&only);
    app.settings.set_selected_provider(ProviderId::OpenRouter);
    app.settings.set_show_selected_provider_in_menu_bar(true);
    if let Some(p) = tab.and_then(|t| t.parse::<ProviderId>().ok()) {
        app.settings.set_selected_provider(p);
    }
}

fn seed_key(app: &App, key: Option<&LoadedKey>) {
    let Some(k) = key else {
        tracing::info!(
            "no credentials: set {} or {} (or use --mock-openrouter)",
            credentials::OPENROUTER_KEY_ENV,
            credentials::OPENROUTER_KEY_FILE_ENV
        );
        return;
    };
    match app.sessions.openrouter.save_api_key(k.expose()) {
        Ok(()) => tracing::info!(source = ?k.source, "OpenRouter key loaded (value not logged)"),
        Err(e) => tracing::error!(error = %e, "OpenRouter key refused"),
    }
}

/// Copies the Swift app's Keychain vault (read-only) into the beta's own vault
/// file, sealed with this process's key. Logs account names and counts only.
fn seed_vault_from_swift(dir: &std::path::Path, encryptor: &Arc<EphemeralEncryptor>) {
    let vault = Vault::new(dir, encryptor.clone());
    match seed_from_swift_vault(&MacKeychain, &vault) {
        Ok(report) if report.found => tracing::info!(
            accounts = ?report.accounts,
            refused = report.refused,
            "read the Swift app's Keychain vault (read-only)"
        ),
        Ok(_) => tracing::info!("no Swift Keychain vault found; starting signed out"),
        Err(e) => {
            tracing::warn!(error = %e, "could not read the Swift Keychain vault; starting signed out");
        }
    }
}

/// The Swift app's `UserDefaults` as JSON, read through `defaults` (read-only).
fn swift_prefs_json() -> Option<String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let exported = Command::new("defaults")
        .args(["export", "com.modelmonitor.app", "-"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let mut convert = Command::new("plutil")
        .args(["-convert", "json", "-o", "-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    convert.stdin.take()?.write_all(&exported.stdout).ok()?;
    let out = convert.wait_with_output().ok()?;
    String::from_utf8(out.stdout).ok()
}

/// First run only: adopt the Swift app's enabled providers, selected tab and
/// menu-bar graphs so the beta opens with the same tabs.
fn import_swift_prefs_once(dir: &std::path::Path, app: &App) {
    let marker = dir.join("swift-prefs-imported");
    if marker.exists() {
        return;
    }
    if let Some(prefs) = swift_prefs_json().and_then(|j| SwiftPrefs::from_json(&j)) {
        prefs.apply(&app.settings);
        tracing::info!(enabled = ?prefs.enabled, "imported the Swift app's provider preferences");
    }
    let _ = std::fs::write(marker, b"");
}

/// The Settings window's view model over the real services.
fn settings_view_model(
    app: &Rc<App>,
    updates: &Arc<UpdateService>,
    version: &str,
    handle: &tokio::runtime::Handle,
) -> Arc<SettingsViewModel> {
    let host = AppHost::new(HostParts {
        settings: Arc::clone(&app.settings),
        model: Arc::clone(&app.model),
        sessions: Arc::clone(&app.sessions),
        signin: Arc::clone(&app.signin),
        updates: Arc::clone(updates),
        login_item: Arc::new(BetaLoginItem),
        desktop_verb: Arc::new(BetaDesktopVerb),
        history: app.model.registry().history(),
        version: version.to_owned(),
        tz: Arc::new(|at| SystemClock.local_offset_at(at)),
        handle: handle.clone(),
        quit: Arc::new(|| {
            let _ = slint::invoke_from_event_loop(|| {
                let _ = slint::quit_event_loop();
            });
        }),
    });
    Arc::new(SettingsViewModel::new(
        Arc::new(AppSettingsBackend::new(Arc::clone(&app.settings))),
        Arc::new(host),
        Arc::new(NativeSaveDialog),
    ))
}

impl Ui {
    fn weak(&self) -> Weak<Ui> {
        self.me.borrow().clone()
    }

    /// Runs inside the event loop once it is up.
    fn start(self: &Rc<Self>) {
        NSApplication::sharedApplication(self.mtm)
            .setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        let weak = self.weak();
        let item = StatusItem::new(
            self.mtm,
            Rc::new(move |right| {
                if let Some(ui) = weak.upgrade() {
                    if right {
                        ui.show_context_menu();
                    } else {
                        ui.toggle();
                    }
                }
            }),
        );
        *self.status.borrow_mut() = item;
        self.refresh_status();
        // The M0 benchmark times process start to this line (CoreGraphics does
        // not list status-item windows on recent macOS).
        tracing::info!("status item ready");
        let weak = self.weak();
        self.tick.start(slint::TimerMode::Repeated, TICK, move || {
            if let Some(ui) = weak.upgrade() {
                ui.on_tick();
            }
        });
        if let Some(secs) = self.options.open_panel_after {
            let weak = self.weak();
            slint::Timer::single_shot(Duration::from_secs_f64(secs), move || {
                if let Some(ui) = weak.upgrade() {
                    ui.show_panel();
                }
            });
        }
        if let Some(secs) = self.options.open_settings_after {
            let weak = self.weak();
            slint::Timer::single_shot(Duration::from_secs_f64(secs), move || {
                if let Some(ui) = weak.upgrade() {
                    ui.open_settings();
                }
            });
        }
        if let Some(secs) = self.options.quit_after {
            slint::Timer::single_shot(Duration::from_secs_f64(secs), || {
                let _ = slint::quit_event_loop();
            });
        }
    }

    fn open_settings(&self) {
        let existing = self.settings_window.borrow().clone();
        let window = match existing {
            Some(w) => w,
            None => match create_settings_window(Arc::clone(&self.settings_vm)) {
                Ok(w) => {
                    *self.settings_window.borrow_mut() = Some(Rc::clone(&w));
                    w
                }
                Err(e) => {
                    tracing::error!(error = %e, "settings window failed");
                    return;
                }
            },
        };
        refresh(&window, &self.settings_vm);
        select_opening_tab(&window, &self.settings_vm);
        apply_theme(&window, self.app_theme());
        let _ = window.show();
        #[allow(deprecated)]
        NSApplication::sharedApplication(self.mtm).activateIgnoringOtherApps(true);
    }

    fn show_context_menu(&self) {
        let entries = context_menu(&self.app.settings.snapshot());
        let rows: Vec<MenuRow> = entries
            .iter()
            .map(|e| match e {
                MenuEntry::Item { label, .. } => MenuRow::Item(label.clone()),
                MenuEntry::Separator => MenuRow::Separator,
            })
            .collect();
        *self.menu_entries.borrow_mut() = entries;
        let weak = self.weak();
        if let Some(item) = self.status.borrow().as_ref() {
            item.show_menu(
                self.mtm,
                &rows,
                Rc::new(move |index| {
                    if let Some(ui) = weak.upgrade() {
                        ui.run_menu_index(index);
                    }
                }),
            );
        }
    }

    fn run_menu_index(&self, index: usize) {
        let action = match self.menu_entries.borrow().get(index) {
            Some(MenuEntry::Item { action, .. }) => *action,
            _ => return,
        };
        match menu_command(action) {
            MenuCommand::Show(p) => {
                self.state.borrow_mut().selected = p;
                self.app.settings.set_selected_provider(p);
                self.show_panel();
            }
            MenuCommand::OpenSettings => self.open_settings(),
            MenuCommand::CheckForUpdates => {
                tracing::info!("updates are not available in the beta");
            }
            MenuCommand::Quit => self.quit(),
        }
    }

    fn quit(&self) {
        record_live(
            &self.store,
            self.app.model.registry(),
            &self.app.settings.snapshot(),
            &self.app.sessions,
            Utc::now(),
            &SystemClock,
        );
        self.app.model.quit();
        let _ = slint::quit_event_loop();
    }

    fn on_tick(&self) {
        self.refresh_status();
        if self.is_panel_visible() {
            self.render(false);
        }
    }

    /// The menu bar chrome: forced by `--chrome`, else the button's appearance.
    fn chrome(&self) -> Chrome {
        self.options.force_chrome.unwrap_or_else(|| {
            let dark = self
                .status
                .borrow()
                .as_ref()
                .is_some_and(|s| s.is_dark(self.mtm));
            if dark { Chrome::Dark } else { Chrome::Light }
        })
    }

    fn app_theme(&self) -> AppTheme {
        // The panel is a normal window: it follows the app (system) appearance,
        // which can differ from the wallpaper-tinted menu bar.
        let app = NSApplication::sharedApplication(self.mtm);
        if super::status_item::appearance_is_dark(&app.effectiveAppearance()) {
            AppTheme::Dark
        } else {
            AppTheme::Light
        }
    }

    fn segments(&self) -> Vec<BarSegment> {
        let settings = self.app.settings.snapshot();
        let used: Vec<(ProviderId, f64)> = self
            .app
            .model
            .registry()
            .entries()
            .iter()
            .filter_map(|e| e.poller.headline_used_percent().map(|v| (e.provider, v)))
            .collect();
        let order = settings.ordered_usage_providers();
        let enabled: Vec<ProviderId> = ProviderId::USAGE_PROVIDERS
            .into_iter()
            .filter(|p| settings.is_provider_enabled(*p))
            .collect();
        let graphs: Vec<ProviderId> = settings.menu_bar_graph_providers.iter().copied().collect();
        let sessions = Arc::clone(&self.app.sessions);
        let content = TrayContent::from_inputs(&TrayInputs {
            selected: settings.selected_provider,
            show_selected: settings.show_selected_provider_in_menu_bar,
            used_percent: &used,
            provider_order: &order,
            menu_bar_graphs: &graphs,
            enabled: &enabled,
            grok_signed_in: sessions.signed_in(ProviderId::Grok),
        });
        match content {
            TrayContent::Selected { provider, used } => vec![BarSegment { provider, used }],
            TrayContent::Bars(bars) => bars,
            TrayContent::Empty | TrayContent::Mark => vec![BarSegment {
                provider: ProviderId::Overview,
                used: None,
            }],
        }
    }

    /// Re-renders the status image when what it shows changed.
    fn refresh_status(&self) {
        let segments = self.segments();
        let chrome = self.chrome();
        let key = format!("{segments:?}-{chrome:?}-{:?}", self.options.renderer);
        if *self.last_status_key.borrow() == key {
            return;
        }
        let image = match self.options.renderer {
            StatusRenderer::Skia => {
                bitmap::status_image(&skia_status::render(&segments, chrome, 2))
            }
            StatusRenderer::AppKit => appkit_status::render(&segments, chrome, 2).map(|r| r.image),
        };
        if let (Some(image), Some(item)) = (image, self.status.borrow().as_ref()) {
            item.set_image(self.mtm, &image);
            *self.last_status_key.borrow_mut() = key;
        }
    }

    fn handler(&self) -> PanelHandler {
        let weak = self.weak();
        Rc::new(move |event| {
            if let Some(ui) = weak.upgrade() {
                ui.on_event(&event);
            }
        })
    }

    fn on_event(&self, event: &PanelEvent) {
        let actions = {
            let (rows, cards) = (self.rows.borrow(), self.cards.borrow());
            self.state.borrow_mut().handle(event, &rows, &cards)
        };
        for action in actions {
            match action {
                Action::Redraw => self.render(false),
                Action::Show(p) => {
                    self.state.borrow_mut().selected = p;
                    self.app.settings.set_selected_provider(p);
                    self.render(false);
                }
                Action::OpenSettings => {
                    self.hide_panel();
                    self.open_settings();
                }
                Action::SignIn(p) => {
                    // No web sign-in in the beta (M2): the Swift app's session is
                    // read from its Keychain instead.
                    self.app.sessions.set_sign_in_error(
                        p,
                        Some(
                            "Sign-in is not available in the Rust beta yet. Sign in with the TokenMon (Swift) app; the beta reads its credentials."
                                .to_owned(),
                        ),
                    );
                    self.render(false);
                }
                Action::SignOut(p) => {
                    self.app.signin.sign_out(p);
                    self.render(false);
                }
                Action::ToggleSection(section) => {
                    let expanded = self.app.settings.snapshot().is_expanded(section);
                    self.app.settings.set_expanded(!expanded, section);
                    self.render(false);
                }
                Action::SubmitApiKey(key) => {
                    let accepted =
                        commit_api_key(&mut self.state.borrow_mut(), &self.app.sessions, &key);
                    if accepted {
                        self.app.model.sign_in_completed(ProviderId::OpenRouter);
                    }
                    self.render(false);
                }
                Action::InstallUpdate => tracing::info!("updates are not available in the beta"),
            }
        }
    }

    fn panel_window(&self) -> Option<Rc<PanelWindow>> {
        if let Some(p) = self.panel.borrow().clone() {
            return Some(p);
        }
        let window = match PanelWindow::create(&self.handler()) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                tracing::error!(error = %e, "panel window failed");
                return None;
            }
        };
        let (focus_ui, esc_ui) = (self.weak(), self.weak());
        window.watch(
            Rc::new(move |focused| {
                if let Some(ui) = focus_ui.upgrade() {
                    ui.on_focus(focused);
                }
            }),
            Rc::new(move || {
                if let Some(ui) = esc_ui.upgrade() {
                    ui.hide_panel();
                }
            }),
        );
        *self.panel.borrow_mut() = Some(Rc::clone(&window));
        Some(window)
    }

    fn on_focus(&self, focused: bool) {
        tracing::debug!(focused, "panel focus changed");
        if focused {
            self.was_focused.set(true);
            return;
        }
        if !self.was_focused.replace(false) {
            return;
        }
        self.focus_lost_at.set(Some(Instant::now()));
        if !self.options.keep_open {
            self.hide_panel();
        }
    }

    fn is_panel_visible(&self) -> bool {
        self.panel.borrow().as_ref().is_some_and(|p| p.is_visible())
    }

    fn toggle(&self) {
        let just_lost = self
            .focus_lost_at
            .get()
            .is_some_and(|t| t.elapsed() < REOPEN_GUARD);
        if self.is_panel_visible() || just_lost {
            self.hide_panel();
        } else {
            self.show_panel();
        }
    }

    fn hide_panel(&self) {
        if self.is_panel_visible() {
            tracing::info!("panel hidden");
        }
        if let Some(p) = self.panel.borrow().as_ref().filter(|p| p.is_visible()) {
            p.hide();
        }
        self.app.model.sync_menu_open(false);
    }

    fn show_panel(&self) {
        self.render(true);
    }

    fn render(&self, reveal: bool) {
        let provider = self.state.borrow().selected;
        let view = {
            let state = self.state.borrow();
            build_view(
                &ViewEnv {
                    model: &self.app.model,
                    sessions: &self.app.sessions,
                    overview: &self.store,
                    version: &self.version,
                    now: Utc::now(),
                    clock: &SystemClock,
                },
                &state,
                provider,
            )
        };
        let rows = build_switcher(
            &self.app.settings.snapshot().visible_usage_providers(),
            provider,
        );
        let Some(window) = self.panel_window() else {
            return;
        };
        if let PanelView::Overview(vm) = &view {
            self.cards.borrow_mut().clone_from(&vm.cards);
        }
        window.set_theme(self.app_theme());
        window.apply(&view, &rows);
        *self.rows.borrow_mut() = rows;
        if reveal || window.is_visible() {
            self.place(&window);
        }
        if reveal {
            window.show();
            let tuned = window.tune_and_focus(self.mtm);
            tracing::info!(tuned, "panel shown");
            // The native window only exists after the first show; place again
            // on the next turn (the Windows port learned this the hard way).
            let weak = self.weak();
            slint::Timer::single_shot(Duration::ZERO, move || {
                if let Some(ui) = weak.upgrade()
                    && let Some(w) = ui.panel.borrow().clone().filter(|w| w.is_visible())
                {
                    ui.place(&w);
                    let tuned = w.tune_and_focus(ui.mtm);
                    tracing::info!(tuned, frame = ?w.frame_description(), anchor = ?ui.status.borrow().as_ref().and_then(|s| s.anchor(ui.mtm)), "panel placed");
                }
            });
            self.app.model.set_menu_open(true);
        }
    }

    fn place(&self, window: &PanelWindow) {
        let status = self.status.borrow();
        let Some(anchor) = status.as_ref().and_then(|s| s.anchor(self.mtm)) else {
            return;
        };
        let screens = NSScreen::screens(self.mtm);
        let primary_height = screens
            .firstObject()
            .map_or(1080.0, |s| s.frame().size.height);
        let visible = NSScreen::mainScreen(self.mtm).map_or_else(
            || crate::placement::AppKitRect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: primary_height,
            },
            |s| {
                let f = s.visibleFrame();
                crate::placement::AppKitRect {
                    x: f.origin.x,
                    y: f.origin.y,
                    width: f.size.width,
                    height: f.size.height,
                }
            },
        );
        let placed = place_under(
            anchor,
            visible,
            primary_height,
            f64::from(super::panel::PANEL_WIDTH),
        );
        window.set_max_height(placed.max_height as f32);
        window.window().set_position(slint::LogicalPosition::new(
            placed.x as f32,
            placed.y as f32,
        ));
    }
}
