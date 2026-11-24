//! Grok panel (`GrokPanelView`, spec 02 section 1.7).

use chrono::{DateTime, Utc};
use tokenmon_core::usage::SavedResets;
use tokenmon_core::{ProviderId, Srgb};

use super::common::{
    DailyBudgetInput, DailyBudgetVm, FooterVm, HeaderVm, PanelContext, PanelKind, PanelVm,
    ProviderStatus, SignInAction, TrackVm, build_daily_budget, geometry, waiting_message,
};
use super::format;
use crate::kit::breakdown::{BreakdownCardVm, BreakdownItem, build_card};

/// A Grok product percentage (already filtered by the visibility settings).
#[derive(Debug, Clone, PartialEq)]
pub struct ProductInput {
    /// Short name (`Chat`, `Build`, ...).
    pub short_name: String,
    /// Percent of the pool.
    pub percent: f64,
    /// Product color.
    pub color: Srgb,
}

/// Grok snapshot as the panel needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct GrokSnapshotInput {
    /// Weekly used percent.
    pub used_percent: f64,
    /// Reset instant.
    pub resets_at: Option<DateTime<Utc>>,
    /// `breakdownProducts(...)`: the Usage Breakdown card's products, most
    /// used first; empty hides the card.
    pub breakdown_products: Vec<ProductInput>,
    /// Daily chart for the selected week, if any.
    pub week: Option<DailyBudgetInput>,
    /// Saved SuperGrok limit resets (loaded by the poller, not the snapshot).
    pub saved_resets: Option<SavedResets>,
}

/// Grok panel input.
#[derive(Debug, Clone, PartialEq)]
pub struct GrokInput {
    /// Auth and poller state.
    pub status: ProviderStatus,
    /// `auth.isSignedIn`.
    pub is_signed_in: bool,
    /// Latest snapshot.
    pub snapshot: Option<GrokSnapshotInput>,
}

/// Grok usage body.
#[derive(Debug, Clone, PartialEq)]
pub struct GrokUsage {
    /// `Usage Pool Weekly`.
    pub pool_title: String,
    /// `N% Used`.
    pub pool_used_text: String,
    /// The weekly used % as one filled bar in the Grok accent (label hidden),
    /// with the reset caption.
    pub track: TrackVm,
    /// Usage Breakdown card; `None` when there is nothing to split.
    pub breakdown: Option<BreakdownCardVm>,
    /// Daily chart.
    pub daily: Option<DailyBudgetVm>,
    /// Footer controls.
    pub footer: FooterVm,
}

/// Builds the Grok panel.
pub fn build(input: &GrokInput, ctx: &PanelContext) -> PanelVm<GrokUsage> {
    let header = HeaderVm::new(ProviderId::Grok, "SuperGrok");
    let s = &input.status;
    if s.needs_sign_in && s.has_auth_error {
        return PanelVm::frame(
            header,
            PanelKind::SessionExpired,
            Some(
                s.last_error
                    .clone()
                    .unwrap_or_else(|| "Session expired. Sign in again to load usage.".to_owned()),
            ),
            Some(SignInAction::again(ProviderId::Grok)),
        );
    }
    let Some(snap) = &input.snapshot else {
        if !input.is_signed_in {
            return PanelVm::frame(
                header,
                PanelKind::SignedOut,
                Some("Sign in to load your usage.".to_owned()),
                Some(SignInAction {
                    label: "Sign In…".to_owned(),
                    provider: ProviderId::Grok,
                }),
            );
        }
        return PanelVm::frame(
            header,
            PanelKind::Waiting,
            Some(waiting_message(s, "Signed in — waiting for usage data.")),
            s.needs_sign_in
                .then(|| SignInAction::again(ProviderId::Grok)),
        );
    };
    let items: Vec<BreakdownItem> = snap
        .breakdown_products
        .iter()
        .map(|p| BreakdownItem {
            id: p.short_name.clone(),
            label: p.short_name.clone(),
            color: p.color,
            amount: p.percent,
            value_text: None,
        })
        .collect();
    let track = TrackVm::new(
        "",
        snap.used_percent,
        ProviderId::Grok.panel_accent(),
        ctx.limit_reset_caption(snap.resets_at, snap.used_percent),
    )
    .without_label();
    PanelVm::with_usage(
        header,
        GrokUsage {
            pool_title: "Usage Pool Weekly".to_owned(),
            pool_used_text: format::percent_used(snap.used_percent),
            track,
            breakdown: (!items.is_empty()).then(|| build_card(&items, geometry::CARD_INNER_WIDTH)),
            daily: snap.week.as_ref().and_then(|w| build_daily_budget(w, ctx)),
            footer: FooterVm::standard(s, ctx, ProviderId::Grok, ProviderId::Grok)
                .with_saved_resets(snap.saved_resets, ProviderId::Grok, ctx),
        },
    )
}
