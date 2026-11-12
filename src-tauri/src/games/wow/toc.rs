//! Parsing of WoW addon `.toc` files and picking the TOC a client will load.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use super::flavor::{FlavorInfo, ALL_TOC_SUFFIXES};

#[derive(Debug, Default, Clone)]
pub struct Toc {
    pub title: Option<String>,
    pub notes: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub interfaces: Vec<u32>,
    pub dependencies: Vec<String>,
    pub optional_dependencies: Vec<String>,
    pub load_on_demand: bool,
    pub group: Option<String>,
    pub allow_load_game_types: Vec<String>,
    pub exclude_load_game_types: Vec<String>,
    /// `## X-...` metadata with lower-case keys, e.g. `x-curse-project-id`.
    pub extras: BTreeMap<String, String>,
}

impl Toc {
    pub fn extra(&self, key: &str) -> Option<&str> {
        self.extras.get(&key.to_ascii_lowercase()).map(String::as_str).filter(|v| !v.is_empty())
    }

    /// True when this TOC targets the given client: matching interface number and
    /// not excluded by `AllowLoadGameType` / `ExcludeLoadGameType`.
    pub fn is_compatible(&self, info: &FlavorInfo) -> bool {
        let interface_ok = self.interfaces.iter().any(|&v| info.accepts_interface(v));
        let allowed = self.allow_load_game_types.is_empty()
            || self.allow_load_game_types.iter().any(|t| info.game_types.contains(&t.as_str()));
        let excluded = self.exclude_load_game_types.iter().any(|t| info.game_types.contains(&t.as_str()));
        interface_ok && allowed && !excluded
    }
}

static MARKUP: LazyLock<Regex> = LazyLock::new(|| {
    // Colour codes (|cffRRGGBB, |cnNAME:), resets (|r), textures (|T...|t), atlases (|A...|a), newlines (|n).
    Regex::new(r"(?i)\|c[0-9a-f]{8}|\|cn[^:|]*:|\|r|\|t[^|]*\|t|\|a[^|]*\|a|\|n").unwrap()
});

static CONDITIONS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]*\]").unwrap());

/// Removes WoW UI markup and collapses whitespace.
pub fn clean_text(text: &str) -> String {
    let stripped = MARKUP.replace_all(text, " ");
    stripped.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn split_list(value: &str) -> Vec<String> {
    CONDITIONS
        .replace_all(value, "")
        .split(',')
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

pub fn parse_toc(text: &str) -> Toc {
    let mut toc = Toc::default();
    for raw in text.lines() {
        let line = raw.trim_start_matches('\u{feff}').trim();
        let Some(rest) = line.strip_prefix("##") else { continue };
        let Some((key, value)) = rest.split_once(':') else { continue };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        if key.is_empty() {
            continue;
        }
        let text_value = || Some(clean_text(value)).filter(|v| !v.is_empty());
        match key.as_str() {
            "title" => toc.title = text_value(),
            "notes" => toc.notes = text_value(),
            "author" => toc.author = text_value(),
            "version" => toc.version = text_value(),
            "interface" => {
                toc.interfaces = value.split(',').filter_map(|v| v.trim().parse().ok()).collect();
            }
            "loadondemand" => toc.load_on_demand = value == "1",
            "group" => toc.group = Some(value.to_string()).filter(|v| !v.is_empty()),
            "optionaldeps" | "optionaldependencies" => toc.optional_dependencies.extend(split_list(value)),
            "allowloadgametype" => toc
                .allow_load_game_types
                .extend(split_list(value).into_iter().map(|v| v.to_ascii_lowercase())),
            "excludeloadgametype" => toc
                .exclude_load_game_types
                .extend(split_list(value).into_iter().map(|v| v.to_ascii_lowercase())),
            k if k.starts_with("dep") || k == "requireddeps" => toc.dependencies.extend(split_list(value)),
            k if k.starts_with("x-") => {
                toc.extras.insert(k.to_string(), value.to_string());
            }
            _ => {}
        }
    }
    toc
}

pub fn read_toc(path: &Path) -> std::io::Result<Toc> {
    let bytes = std::fs::read(path)?;
    Ok(parse_toc(&String::from_utf8_lossy(&bytes)))
}

/// The TOC a client loads for `folder`, plus the names of the other TOCs present.
pub struct TocChoice {
    pub chosen: Option<PathBuf>,
    pub others: Vec<String>,
}

/// Mirrors the client's lookup: game-type suffix, then family suffix, then `Name.toc`.
pub fn choose_toc(dir: &Path, folder: &str, info: &FlavorInfo) -> TocChoice {
    let tocs: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.to_ascii_lowercase().ends_with(".toc"))
                .collect()
        })
        .unwrap_or_default();

    let mut candidates: Vec<String> = Vec::new();
    for suffix in info.toc_suffixes {
        candidates.push(format!("{folder}_{suffix}.toc"));
        candidates.push(format!("{folder}-{suffix}.toc"));
    }
    candidates.push(format!("{folder}.toc"));

    let chosen = candidates
        .iter()
        .find_map(|candidate| tocs.iter().find(|t| t.eq_ignore_ascii_case(candidate)))
        .cloned();
    let others = tocs.iter().filter(|t| Some(*t) != chosen.as_ref()).cloned().collect();
    TocChoice { chosen: chosen.map(|name| dir.join(name)), others }
}

/// The addon folder name a TOC file belongs to: `Questie_Forever.toc` -> `Questie`.
pub fn toc_base_name(file_stem: &str) -> &str {
    for suffix in ALL_TOC_SUFFIXES {
        for sep in ['_', '-'] {
            let tail = format!("{sep}{suffix}");
            if file_stem.len() > tail.len() && file_stem.to_ascii_lowercase().ends_with(&tail.to_ascii_lowercase()) {
                return &file_stem[..file_stem.len() - tail.len()];
            }
        }
    }
    file_stem
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::wow::flavor::Flavor;

    const SAMPLE: &str = "\u{feff}## Interface: 120105, 16001, 11509\n\
## Title: |cff00ccffBig|r Wigs |TInterface\\Icons\\x:16|t\n\
## Notes: Boss mods\n\
## Author: Funkeh\n\
## Version: v427.9\n\
## Dependencies: BigWigs_Core, [AllowLoadGameType camelot]LibStub\n\
## OptionalDeps: Ace3\n\
## X-Curse-Project-ID: 2382\n\
## X-WoWI-ID: 5086\n\
## LoadOnDemand: 1\n\
Core.lua\n";

    #[test]
    fn parses_metadata_and_strips_markup() {
        let toc = parse_toc(SAMPLE);
        assert_eq!(toc.title.as_deref(), Some("Big Wigs"));
        assert_eq!(toc.interfaces, vec![120105, 16001, 11509]);
        assert_eq!(toc.dependencies, vec!["BigWigs_Core", "LibStub"]);
        assert_eq!(toc.optional_dependencies, vec!["Ace3"]);
        assert_eq!(toc.extra("X-Curse-Project-ID"), Some("2382"));
        assert!(toc.load_on_demand);
        assert!(toc.is_compatible(Flavor::Forever.info()));
        assert!(toc.is_compatible(Flavor::ClassicEra.info()));
        assert!(!toc.is_compatible(Flavor::Classic.info()));
    }

    #[test]
    fn honours_game_type_restrictions() {
        let toc = parse_toc("## Interface: 16001\n## ExcludeLoadGameType: camelot\n");
        assert!(!toc.is_compatible(Flavor::Forever.info()));
        let toc = parse_toc("## Interface: 16001\n## AllowLoadGameType: standard\n");
        assert!(!toc.is_compatible(Flavor::Forever.info()));
    }

    #[test]
    fn strips_toc_suffixes() {
        assert_eq!(toc_base_name("Questie_Forever"), "Questie");
        assert_eq!(toc_base_name("DBM-Core_Mainline"), "DBM-Core");
        assert_eq!(toc_base_name("Grid2-Camelot"), "Grid2");
        assert_eq!(toc_base_name("Classic"), "Classic");
    }

    #[test]
    fn chooses_the_toc_the_client_would_load() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["Addon.toc", "Addon_Mainline.toc", "Addon_Vanilla.toc", "Addon_Camelot.toc"] {
            std::fs::write(dir.path().join(name), "## Interface: 1\n").unwrap();
        }
        let pick = |flavor: Flavor| {
            choose_toc(dir.path(), "Addon", flavor.info())
                .chosen
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        };
        assert_eq!(pick(Flavor::Forever).as_deref(), Some("Addon_Camelot.toc"));
        assert_eq!(pick(Flavor::Retail).as_deref(), Some("Addon_Mainline.toc"));
        assert_eq!(pick(Flavor::ClassicEra).as_deref(), Some("Addon_Vanilla.toc"));
        assert_eq!(pick(Flavor::Classic).as_deref(), Some("Addon.toc"));
    }
}
