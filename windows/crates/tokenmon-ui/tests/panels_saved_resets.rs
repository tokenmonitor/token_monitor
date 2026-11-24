//! Footer pills (upstream 41924464): the "Resets available" pill on the
//! sign-out row of the ChatGPT, Claude and Grok panels, and the "Update
//! available" pill under the panel body.
#![allow(missing_docs, clippy::unwrap_used)]

mod panels_common;

use panels_common::{at, ctx, status};
use tokenmon_core::ProviderId;
use tokenmon_core::update::{AppVersion, AvailableRelease, UpdateState};
use tokenmon_core::usage::SavedResets;
use tokenmon_ui::panels::claude_chatgpt::{
    ChatGptInput, ChatGptSnapshotInput, ClaudeInput, ClaudeSnapshotInput, WindowInput,
    build_chatgpt, build_claude,
};
use tokenmon_ui::panels::common::{FooterVm, SavedResetsVm, UpdatePillVm};
use tokenmon_ui::panels::grok::{GrokInput, GrokSnapshotInput, build as build_grok};

fn window() -> WindowInput {
    WindowInput {
        label: None,
        used_percent: Some(10.0),
        resets_at: None,
    }
}

fn claude_snapshot(saved_resets: Option<SavedResets>) -> ClaudeSnapshotInput {
    ClaudeSnapshotInput {
        saved_resets,
        pool_name: "Mixed".into(),
        five_hour: Some(window()),
        seven_day: Some(window()),
        weekly_label: "Weekly".into(),
        weekly_resets_at: None,
        daily: None,
    }
}

/// With an expiry the pill's tooltip names it in the local zone; the count
/// and provider accent come along.
#[test]
fn test_saved_resets_pill_shows_count_and_next_expiry() {
    let resets = SavedResets::new(2, Some(at(2026, 12, 5, 0, 0))).unwrap();
    let pill = SavedResetsVm::new(&resets, ProviderId::Claude, &ctx());
    assert_eq!(pill.label, "Resets available");
    assert_eq!(pill.count_text, "2");
    assert_eq!(pill.help, "Next expires Sat 05 December 12:00am");
    assert_eq!(pill.accent, ProviderId::Claude.accent());
}

/// Without an expiry the tooltip is the generic text.
#[test]
fn test_saved_resets_pill_without_expiry_has_generic_help() {
    let resets = SavedResets::new(1, None).unwrap();
    let pill = SavedResetsVm::new(&resets, ProviderId::ChatGpt, &ctx());
    assert_eq!(pill.help, "Saved usage-limit resets");
    assert_eq!(pill.count_text, "1");
}

/// The Claude, ChatGPT and Grok footers carry the pill when the snapshot has
/// saved resets, and none when it has not.
#[test]
fn test_claude_chatgpt_and_grok_footers_show_the_pill_only_with_resets() {
    let resets = SavedResets::new(3, None);
    let claude = |r| {
        build_claude(
            &ClaudeInput {
                status: status(),
                snapshot: Some(claude_snapshot(r)),
                code: None,
            },
            &ctx(),
        )
        .panel
        .usage
        .unwrap()
        .footer
    };
    assert_eq!(claude(resets).saved_resets.unwrap().count_text, "3");
    assert!(claude(None).saved_resets.is_none());

    let gpt = |r| {
        build_chatgpt(
            &ChatGptInput {
                status: status(),
                snapshot: Some(ChatGptSnapshotInput {
                    saved_resets: r,
                    plan_name: "Plus".into(),
                    pool_name: "Mixed".into(),
                    primary: Some(window()),
                    secondary: Some(window()),
                    limit_reached: false,
                    allowed: true,
                }),
            },
            &ctx(),
        )
        .usage
        .unwrap()
        .footer
    };
    assert_eq!(gpt(resets).saved_resets.unwrap().count_text, "3");
    assert!(gpt(None).saved_resets.is_none());

    let grok = |r| {
        build_grok(
            &GrokInput {
                status: status(),
                is_signed_in: true,
                snapshot: Some(GrokSnapshotInput {
                    saved_resets: r,
                    used_percent: 10.0,
                    resets_at: None,
                    breakdown_products: Vec::new(),
                    week: None,
                }),
            },
            &ctx(),
        )
        .usage
        .unwrap()
        .footer
    };
    assert_eq!(grok(resets).saved_resets.unwrap().count_text, "3");
    assert!(grok(None).saved_resets.is_none());
}

/// The standard footer has no pill until one is attached.
#[test]
fn test_standard_footer_has_no_pill() {
    let f = FooterVm::standard(&status(), &ctx(), ProviderId::Claude, ProviderId::Claude);
    assert!(f.saved_resets.is_none());
}

fn release(version: &str) -> AvailableRelease {
    AvailableRelease {
        version: AppVersion::parse(version).unwrap(),
        tag: format!("windows-v{version}"),
        page_url: String::new(),
        archive: None,
        installer: None,
        published_at: None,
    }
}

/// No release, no pill.
#[test]
fn test_update_pill_hidden_without_a_release() {
    assert!(UpdatePillVm::from_state(&UpdateState::default()).is_none());
}

/// An available release shows "Update available" with the version in the
/// tooltip, enabled, and passes the status message through.
#[test]
fn test_update_pill_names_the_version_in_its_tooltip() {
    let state = UpdateState {
        available_release: Some(release("1.14.0")),
        status_message: Some("Downloading".into()),
        ..UpdateState::default()
    };
    let pill = UpdatePillVm::from_state(&state).unwrap();
    assert_eq!(pill.label, "Update available");
    assert_eq!(pill.help, "Download, verify, and install TokenMon 1.14.0");
    assert_eq!(pill.status.as_deref(), Some("Downloading"));
    assert!(pill.enabled);
}

/// While installing the label changes and the pill is disabled.
#[test]
fn test_update_pill_while_installing_is_disabled() {
    let state = UpdateState {
        available_release: Some(release("1.14.0")),
        is_installing: true,
        ..UpdateState::default()
    };
    let pill = UpdatePillVm::from_state(&state).unwrap();
    assert_eq!(pill.label, "Installing update\u{2026}");
    assert_eq!(pill.help, "Installing TokenMon 1.14.0");
    assert!(!pill.enabled);
}
