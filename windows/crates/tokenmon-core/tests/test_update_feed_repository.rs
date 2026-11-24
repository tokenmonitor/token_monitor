//! Port of `ReleaseFeedTests.testLatestReleaseURLIsTheOrganizationRepository`:
//! the Windows update feed reads releases from the `tokenmonitor` organization
//! repository, the same repository the macOS app reads (upstream #58).

use tokenmon_core::update::feed::{DEFAULT_REPOSITORY, ReleaseSource, parse_repository};

/// The compiled-in default is the organization repository, and its feed URL is
/// that repository's release list. (`TOKENMON_UPDATE_REPO` can override it at
/// build time, so the default is checked directly.)
#[test]
fn test_latest_release_url_is_the_organization_repository() {
    assert_eq!(DEFAULT_REPOSITORY, "tokenmonitor/token_monitor");
    let (owner, repository) = parse_repository(DEFAULT_REPOSITORY).expect("a valid slug");
    let source = ReleaseSource {
        owner: owner.to_owned(),
        repository: repository.to_owned(),
        ..ReleaseSource::default()
    };
    assert_eq!(
        source.releases_url(),
        "https://api.github.com/repos/tokenmonitor/token_monitor/releases?per_page=30"
    );
}
