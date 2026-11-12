//! WoW game versions ("flavours") and everything that differs between them:
//! client folder names, TOC suffixes, interface numbers and provider identifiers.
//!
//! WoW Forever (Blizzard's Classic+ client, patch 1.60) is the primary target:
//! client folder `_forever_`, interface 1600x, TOC suffix `_Forever`/`_Camelot`
//! with `_Mainline` as the family fallback, `forever` in BigWigs-packager
//! `release.json` files, CurseForge game version type 88568, and the Battle.net
//! launch code `WoWF`.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Flavor {
    Forever,
    Retail,
    ClassicEra,
    Classic,
    Anniversary,
}

pub struct FlavorInfo {
    pub flavor: Flavor,
    pub name: &'static str,
    /// Client folders inside the World of Warcraft directory.
    pub folders: &'static [&'static str],
    /// TOC suffixes the client looks for, highest priority first (plain `Name.toc` is the last resort).
    pub toc_suffixes: &'static [&'static str],
    /// Game-type names used by `## AllowLoadGameType` / `## ExcludeLoadGameType`.
    pub game_types: &'static [&'static str],
    /// Flavour names in BigWigs-packager `release.json` metadata.
    pub release_flavors: &'static [&'static str],
    /// Words that mark a release asset as built for this flavour, e.g. `MoveAny-1.0-forever.zip`.
    pub asset_keywords: &'static [&'static str],
    pub curseforge_type_id: u32,
    /// Interface numbers this client loads (inclusive).
    pub interface_range: (u32, u32),
    /// `Battle.net.exe --exec="launch <code>"`, when known.
    pub battlenet_code: Option<&'static str>,
}

pub const FLAVORS: &[FlavorInfo] = &[
    FlavorInfo {
        flavor: Flavor::Forever,
        name: "WoW Forever",
        folders: &["_forever_", "_forever_ptr_", "_forever_beta_"],
        toc_suffixes: &["Forever", "Camelot", "Mainline"],
        game_types: &["forever", "camelot", "mainline"],
        release_flavors: &["forever", "camelot"],
        asset_keywords: &["forever", "camelot"],
        curseforge_type_id: 88568,
        interface_range: (16000, 16999),
        battlenet_code: Some("WoWF"),
    },
    FlavorInfo {
        flavor: Flavor::Retail,
        name: "World of Warcraft",
        folders: &["_retail_", "_ptr_", "_xptr_", "_beta_"],
        toc_suffixes: &["Standard", "Mainline"],
        game_types: &["standard", "mainline"],
        release_flavors: &["mainline", "retail"],
        asset_keywords: &["mainline", "retail"],
        curseforge_type_id: 517,
        interface_range: (100000, 199999),
        battlenet_code: Some("WoW"),
    },
    FlavorInfo {
        flavor: Flavor::ClassicEra,
        name: "WoW Classic Era",
        folders: &["_classic_era_", "_classic_era_ptr_", "_classic_era_beta_"],
        toc_suffixes: &["Vanilla", "Classic"],
        game_types: &["vanilla", "classic"],
        release_flavors: &["classic", "vanilla"],
        asset_keywords: &["vanilla", "classic", "era"],
        curseforge_type_id: 67408,
        interface_range: (11300, 11999),
        battlenet_code: None,
    },
    FlavorInfo {
        flavor: Flavor::Classic,
        name: "WoW Classic",
        folders: &["_classic_", "_classic_ptr_", "_classic_beta_"],
        toc_suffixes: &["Mists", "Classic"],
        game_types: &["mists", "classic"],
        release_flavors: &["mists"],
        asset_keywords: &["mists"],
        curseforge_type_id: 79434,
        interface_range: (50000, 59999),
        battlenet_code: Some("WoWC"),
    },
    FlavorInfo {
        flavor: Flavor::Anniversary,
        name: "WoW Anniversary",
        folders: &["_anniversary_", "_anniversary_ptr_"],
        toc_suffixes: &["TBC", "BCC", "Classic"],
        game_types: &["tbc", "classic"],
        release_flavors: &["bcc", "tbc"],
        asset_keywords: &["bcc", "tbc"],
        curseforge_type_id: 73246,
        interface_range: (20000, 29999),
        battlenet_code: None,
    },
];

/// Every TOC suffix any client knows, used to find the base name of a TOC file.
pub const ALL_TOC_SUFFIXES: &[&str] = &[
    "Mainline", "Classic", "Vanilla", "TBC", "BCC", "Wrath", "WOTLKC", "Cata", "Mists", "Camelot",
    "Forever", "Standard", "Titan", "Plunderstorm", "WoWLabs", "WoWHack",
];

impl Flavor {
    pub fn info(self) -> &'static FlavorInfo {
        FLAVORS.iter().find(|f| f.flavor == self).expect("every flavour has an entry")
    }

    /// Maps a Battle.net product code, as written in a client's `.flavor.info`, to its
    /// flavour: `wowf` is WoW Forever, `wow` retail, `wow_classic_era` Classic Era.
    pub fn from_product(code: &str) -> Option<Flavor> {
        let code = code.trim().to_ascii_lowercase();
        if code.starts_with("wowf") {
            Some(Flavor::Forever)
        } else if code.starts_with("wow_classic_era") {
            Some(Flavor::ClassicEra)
        } else if code.starts_with("wow_anniversary") {
            Some(Flavor::Anniversary)
        } else if code.starts_with("wow_classic") {
            Some(Flavor::Classic)
        } else if matches!(code.as_str(), "wow" | "wowt" | "wowxptr" | "wowdev") || code.starts_with("wow_beta") {
            Some(Flavor::Retail)
        } else {
            None
        }
    }

    /// Maps a client folder name such as `_forever_` to its flavour.
    pub fn from_folder(name: &str) -> Option<Flavor> {
        let lower = name.to_ascii_lowercase();
        FLAVORS
            .iter()
            .find(|f| f.folders.iter().any(|folder| *folder == lower))
            .map(|f| f.flavor)
    }
}

impl FlavorInfo {
    pub fn accepts_interface(&self, interface: u32) -> bool {
        interface >= self.interface_range.0 && interface <= self.interface_range.1
    }
}

/// Game versions in the order they're listed on an addon's details.
const GAME_LABELS: [&str; 8] = [
    "WoW Forever",
    "World of Warcraft",
    "Classic Era",
    "Burning Crusade Classic",
    "Wrath Classic",
    "Cataclysm Classic",
    "Mists of Pandaria Classic",
    "Titan Reforged",
];

/// Display name for a BigWigs-packager `release.json` flavour or a release-file keyword.
pub fn release_flavor_label(flavor: &str) -> Option<&'static str> {
    match flavor.to_ascii_lowercase().as_str() {
        "forever" | "camelot" => Some(GAME_LABELS[0]),
        "mainline" | "retail" => Some(GAME_LABELS[1]),
        "classic" | "vanilla" => Some(GAME_LABELS[2]),
        "bcc" | "tbc" => Some(GAME_LABELS[3]),
        "wrath" | "wotlkc" => Some(GAME_LABELS[4]),
        "cata" => Some(GAME_LABELS[5]),
        "mists" => Some(GAME_LABELS[6]),
        "titan" => Some(GAME_LABELS[7]),
        _ => None,
    }
}

/// Display name for an interface number, e.g. 16001 -> "WoW Forever".
pub fn interface_label(interface: u32) -> Option<&'static str> {
    match interface {
        16000..=16999 => Some(GAME_LABELS[0]),
        100000..=199999 => Some(GAME_LABELS[1]),
        11300..=11999 => Some(GAME_LABELS[2]),
        20000..=29999 => Some(GAME_LABELS[3]),
        30000..=37999 => Some(GAME_LABELS[4]),
        38000..=38999 => Some(GAME_LABELS[7]),
        40000..=49999 => Some(GAME_LABELS[5]),
        50000..=59999 => Some(GAME_LABELS[6]),
        _ => None,
    }
}

/// Display name for a CurseForge game version type id.
pub fn curseforge_type_label(id: u32) -> Option<&'static str> {
    match id {
        88568 => Some(GAME_LABELS[0]),
        517 => Some(GAME_LABELS[1]),
        67408 => Some(GAME_LABELS[2]),
        73246 => Some(GAME_LABELS[3]),
        73713 => Some(GAME_LABELS[4]),
        77522 => Some(GAME_LABELS[5]),
        79434 => Some(GAME_LABELS[6]),
        81212 => Some(GAME_LABELS[7]),
        _ => None,
    }
}

/// De-duplicates game labels and puts them in the standard order.
pub fn ordered_labels(labels: impl IntoIterator<Item = &'static str>) -> Vec<String> {
    let mut unique: Vec<&'static str> = Vec::new();
    for label in labels {
        if !unique.contains(&label) {
            unique.push(label);
        }
    }
    unique.sort_by_key(|label| GAME_LABELS.iter().position(|l| l == label).unwrap_or(usize::MAX));
    unique.into_iter().map(str::to_string).collect()
}

/// "PTR" / "Beta" label for test-realm client folders.
pub fn folder_variant(folder: &str) -> Option<&'static str> {
    let lower = folder.to_ascii_lowercase();
    if lower.contains("xptr") {
        Some("Experimental PTR")
    } else if lower.contains("ptr") {
        Some("PTR")
    } else if lower.contains("beta") {
        Some("Beta")
    } else {
        None
    }
}

/// Converts a patch string such as `1.60.1` to an interface number (`16001`).
pub fn patch_to_interface(patch: &str) -> Option<u32> {
    let mut parts = patch.trim().split('.').map(|p| p.parse::<u32>().ok());
    let major = parts.next()??;
    let minor = parts.next().flatten().unwrap_or(0);
    let fix = parts.next().flatten().unwrap_or(0);
    if minor > 99 || fix > 99 {
        return None;
    }
    Some(major * 10000 + minor * 100 + fix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_client_folders() {
        assert_eq!(Flavor::from_folder("_forever_"), Some(Flavor::Forever));
        assert_eq!(Flavor::from_folder("_FOREVER_PTR_"), Some(Flavor::Forever));
        assert_eq!(Flavor::from_folder("_retail_"), Some(Flavor::Retail));
        assert_eq!(Flavor::from_folder("_classic_era_"), Some(Flavor::ClassicEra));
        assert_eq!(Flavor::from_folder("_something_"), None);
    }

    #[test]
    fn labels_game_versions_in_a_stable_order() {
        let labels = ordered_labels(
            ["mists", "forever", "classic", "mainline", "forever", "nonsense"]
                .into_iter()
                .filter_map(release_flavor_label),
        );
        assert_eq!(labels, vec!["WoW Forever", "World of Warcraft", "Classic Era", "Mists of Pandaria Classic"]);
        assert_eq!(interface_label(16001), Some("WoW Forever"));
        assert_eq!(interface_label(38002), Some("Titan Reforged"));
        assert_eq!(curseforge_type_label(88568), Some("WoW Forever"));
    }

    #[test]
    fn maps_battlenet_product_codes() {
        assert_eq!(Flavor::from_product("wowf\n"), Some(Flavor::Forever));
        assert_eq!(Flavor::from_product("wow"), Some(Flavor::Retail));
        assert_eq!(Flavor::from_product("wowt"), Some(Flavor::Retail));
        assert_eq!(Flavor::from_product("wow_classic_era"), Some(Flavor::ClassicEra));
        assert_eq!(Flavor::from_product("wow_classic_ptr"), Some(Flavor::Classic));
        assert_eq!(Flavor::from_product("diablo3"), None);
    }

    #[test]
    fn converts_patches_to_interface_numbers() {
        assert_eq!(patch_to_interface("1.60.1"), Some(16001));
        assert_eq!(patch_to_interface("12.1.5"), Some(120105));
        assert_eq!(patch_to_interface("1.15.9"), Some(11509));
        assert_eq!(patch_to_interface("5.5.4"), Some(50504));
        assert!(Flavor::Forever.info().accepts_interface(16001));
        assert!(!Flavor::Forever.info().accepts_interface(11509));
        assert!(!Flavor::Forever.info().accepts_interface(120105));
    }
}
