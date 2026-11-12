//! Network tests against the real addon sources. Ignored by default:
//! `cargo test -- --ignored live_` (no API keys needed).

use crate::games::wow::flavor::Flavor;
use crate::games::wow::scan::scan_addons;
use crate::http;
use crate::install;
use crate::model::{Channel, Compatibility, ProviderId, SourceRef};
use crate::providers::{self, SearchScope};
use crate::state::AppState;

#[tokio::test]
#[ignore]
async fn live_installs_forever_builds_from_github_and_tukui() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let tmp = tempfile::tempdir().unwrap();
    let state = AppState::for_tests(tmp.path());
    let addons_dir = tmp.path().join("_forever_").join("Interface").join("AddOns");

    let cases = [
        (ProviderId::Github, "Questie/Questie", "Questie"),
        (ProviderId::Github, "BigWigsMods/BigWigs", "BigWigs"),
        (ProviderId::Github, "michaelnpsp/Grid2", "Grid2"),
        (ProviderId::Github, "d4kir92/MoveAny", "MoveAny"),
        (ProviderId::Tukui, "elvui", "ElvUI"),
    ];
    for (provider, project, main_folder) in cases {
        let source = SourceRef::new(provider, project);
        let release = providers::resolve(&state, &source, Flavor::Forever, Channel::Stable)
            .await
            .unwrap_or_else(|e| panic!("resolve {project}: {e:#}"));
        println!("{project}: {} -> {}", release.version, release.download_url);

        let zip = tmp.path().join(format!("{}.zip", main_folder));
        let bytes = http::download(&state.http, &release.download_url, &zip, Default::default(), |_, _| {})
            .await
            .unwrap_or_else(|e| panic!("download {project}: {e:#}"));
        assert!(bytes > 1000, "{project} download is suspiciously small");

        let folders = install::install_archive(&zip, &addons_dir, &[]).unwrap_or_else(|e| panic!("install {project}: {e:#}"));
        assert!(folders.iter().any(|f| f == main_folder), "{project} installed {folders:?}");
    }

    let scanned = scan_addons(&addons_dir, Flavor::Forever.info()).unwrap();
    for (_, project, main_folder) in cases {
        let folder = scanned.iter().find(|f| f.name == main_folder).unwrap();
        assert_eq!(
            folder.compatibility(Flavor::Forever.info(), Some(16001)),
            Compatibility::Compatible,
            "{project}: {main_folder} should load on WoW Forever (toc {:?})",
            folder.toc_file
        );
    }
    println!("Installed {} folders", scanned.len());
}

#[tokio::test]
#[ignore]
async fn live_search_finds_forever_addons() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let tmp = tempfile::tempdir().unwrap();
    let state = AppState::for_tests(tmp.path());

    let outcome = providers::search(&state, "questie", Flavor::Forever, SearchScope::All).await;
    for hit in &outcome.hits {
        println!("{:?} {} supports={:?}", hit.source.provider, hit.source.project_id, hit.supports_flavor);
    }
    assert!(outcome.warnings.is_empty(), "warnings: {:?}", outcome.warnings);
    let questie = outcome.hits.iter().find(|h| h.source.project_id == "Questie/Questie").expect("Questie found");
    assert_eq!(questie.supports_flavor, Some(true));

    let elvui = providers::search(&state, "elvui", Flavor::Forever, SearchScope::Tukui).await;
    assert!(elvui.hits.iter().any(|h| h.source.project_id == "elvui" && h.supports_flavor == Some(true)));
}

#[test]
#[ignore]
fn live_detects_installs_on_this_machine() {
    let roots = crate::games::wow::detect::candidate_roots();
    println!("candidate roots: {roots:#?}");
    let found = crate::games::wow::detect::detect_all();
    for client in &found {
        println!("found {:?} {} {:?} at {} ({} addons)", client.flavor, client.label, client.version, client.path.display(), client.addon_count);
        let info = crate::games::wow::detect::client_info(&client.path);
        println!("  product {:?}, interface {:?}", info.product, info.interface());
    }
    assert!(!found.is_empty(), "no WoW clients detected");
}

/// Writes each detected client's icon to `HF_ICON_OUT` (a folder) for a visual check.
#[test]
#[ignore]
fn live_extracts_game_icons() {
    let out = std::env::var("HF_ICON_OUT").expect("set HF_ICON_OUT to a folder");
    for client in crate::games::wow::detect::detect_all() {
        let exe = crate::games::wow::launch::find_game_exe(&client.path).expect("game executable");
        let (bytes, mime) = crate::games::wow::icon::exe_icon(&exe).expect("icon");
        let small = crate::games::wow::icon::thumbnail(&bytes, mime, 128).expect("thumbnail");
        let name = format!("{:?}-128.png", client.flavor);
        std::fs::write(std::path::Path::new(&out).join(&name), &small).unwrap();
        println!("{} -> {name} ({} bytes from {} {mime})", exe.display(), small.len(), bytes.len());
        assert!(crate::games::wow::icon::client_icon_data_url(&client.path).is_some());
    }
}

#[tokio::test]
#[ignore]
async fn live_fetches_addon_details() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let tmp = tempfile::tempdir().unwrap();
    let state = AppState::for_tests(tmp.path());
    for (provider, project) in [
        (ProviderId::Github, "Questie/Questie"),
        (ProviderId::Github, "openmap-world/openmap"),
        (ProviderId::Github, "Krelock/NodeSight"),
        (ProviderId::Tukui, "elvui"),
    ] {
        let details = providers::details(&state, &SourceRef::new(provider, project), Flavor::Forever)
            .await
            .unwrap_or_else(|e| panic!("details {project}: {e:#}"));
        println!(
            "{project}: v{:?} {:?} supports={:?} games={:?} size={:?} stars={:?} licence={:?} desc={} chars notes={} chars shots={}",
            details.latest_version,
            details.latest_date.map(|d| d.date_naive()),
            details.supports_flavor,
            details.game_versions,
            details.download_size,
            details.stars,
            details.license,
            details.description.as_deref().map(str::len).unwrap_or(0),
            details.release_notes.as_deref().map(str::len).unwrap_or(0),
            details.screenshots.len()
        );
        assert!(details.description.is_some(), "{project} has a description");
        assert!(details.latest_version.is_some(), "{project} has a release");
    }
}
