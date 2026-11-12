//! Reading the `Interface/AddOns` folder and grouping its folders into addons.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};

use super::flavor::FlavorInfo;
use super::toc::{choose_toc, read_toc, Toc};
use crate::model::{Compatibility, FolderView};

#[derive(Debug, Clone)]
pub struct ScannedFolder {
    pub name: String,
    pub toc_file: Option<String>,
    pub toc: Option<Toc>,
    pub other_tocs: Vec<String>,
}

impl ScannedFolder {
    /// Whether the client will load this folder. `client_interface` is the installed
    /// game's own interface number (16001 for 1.60.1); like the game, an addon built
    /// for an earlier patch counts as out of date.
    pub fn compatibility(&self, info: &FlavorInfo, client_interface: Option<u32>) -> Compatibility {
        match &self.toc {
            None => Compatibility::Missing,
            Some(toc) if !toc.is_compatible(info) => Compatibility::OutOfDate,
            Some(toc) => match client_interface {
                Some(current) if !toc.interfaces.iter().any(|&i| info.accepts_interface(i) && i >= current) => {
                    Compatibility::OlderPatch
                }
                _ => Compatibility::Compatible,
            },
        }
    }

    pub fn to_view(&self, info: &FlavorInfo, client_interface: Option<u32>) -> FolderView {
        let toc = self.toc.as_ref();
        FolderView {
            name: self.name.clone(),
            title: toc.and_then(|t| t.title.clone()),
            version: toc.and_then(|t| t.version.clone()),
            interfaces: toc.map(|t| t.interfaces.clone()).unwrap_or_default(),
            toc_file: self.toc_file.clone(),
            other_tocs: self.other_tocs.clone(),
            compatibility: self.compatibility(info, client_interface),
            dependencies: toc.map(|t| t.dependencies.clone()).unwrap_or_default(),
            load_on_demand: toc.map(|t| t.load_on_demand).unwrap_or(false),
        }
    }
}

pub fn scan_folder(dir: &Path, name: &str, info: &FlavorInfo) -> ScannedFolder {
    let choice = choose_toc(dir, name, info);
    let toc = choice.chosen.as_deref().and_then(|p| match read_toc(p) {
        Ok(toc) => Some(toc),
        Err(error) => {
            log::warn!("Couldn't read {}: {error}", p.display());
            None
        }
    });
    ScannedFolder {
        name: name.to_string(),
        toc_file: choice
            .chosen
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned()),
        toc,
        other_tocs: choice.others,
    }
}

/// Every addon folder in `addons_dir`, sorted by name. Missing directory = no addons.
pub fn scan_addons(addons_dir: &Path, info: &FlavorInfo) -> Result<Vec<ScannedFolder>> {
    let entries = match std::fs::read_dir(addons_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("Couldn't read {}", addons_dir.display()))
        }
    };
    let mut folders: Vec<ScannedFolder> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            if name.starts_with('.') {
                return None;
            }
            Some(scan_folder(&e.path(), &name, info))
        })
        .collect();
    folders.sort_by_key(|f| f.name.to_lowercase());
    Ok(folders)
}

/// True when `child` looks like a module of `parent` by name:
/// `BigWigs_Core` of `BigWigs`, or `DBM-StatusBarTimers` of `DBM-Core`.
fn shares_name_root(child: &str, parent: &str) -> bool {
    let c = child.to_ascii_lowercase();
    let p = parent.to_ascii_lowercase();
    if c.len() > p.len() && c.starts_with(&p) {
        return true;
    }
    let common: String = c.chars().zip(p.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a).collect();
    common.len() >= 3 && common.ends_with(['-', '_', ' '])
}

/// Groups loose (untracked) folders into addons. Each group's first entry is the
/// main folder; modules join it via `## Group:`, or a required dependency that
/// shares their name (e.g. `ElvUI_Options` depends on `ElvUI`).
pub fn group_folders(folders: &[&ScannedFolder]) -> Vec<Vec<String>> {
    let by_lower: HashMap<String, &ScannedFolder> =
        folders.iter().map(|f| (f.name.to_ascii_lowercase(), *f)).collect();

    let mut parent: HashMap<String, String> = HashMap::new();
    for folder in folders {
        let me = folder.name.to_ascii_lowercase();
        let Some(toc) = &folder.toc else { continue };
        if let Some(group) = toc.group.as_ref().map(|g| g.to_ascii_lowercase()) {
            if group != me && by_lower.contains_key(&group) {
                parent.insert(me, group);
                continue;
            }
        }
        if let Some(dep) = toc
            .dependencies
            .iter()
            .map(|d| d.to_ascii_lowercase())
            .find(|d| *d != me && by_lower.contains_key(d) && shares_name_root(&me, d))
        {
            parent.insert(me, dep);
        }
    }

    let root_of = |name: &str| -> String {
        let mut current = name.to_string();
        let mut seen = HashSet::new();
        while let Some(next) = parent.get(&current) {
            if !seen.insert(current.clone()) {
                break; // dependency cycle: stop where we are
            }
            current = next.clone();
        }
        current
    };

    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for folder in folders {
        let root = root_of(&folder.name.to_ascii_lowercase());
        groups.entry(root).or_default().push(folder.name.clone());
    }
    groups
        .into_iter()
        .map(|(root, mut members)| {
            members.sort_by_key(|m| (m.to_ascii_lowercase() != root, m.to_lowercase()));
            members
        })
        .collect()
}

/// Required dependencies of `members` that aren't installed (Blizzard's own addons excluded).
pub fn missing_dependencies(members: &[&ScannedFolder], installed: &HashSet<String>) -> Vec<String> {
    let mut missing: Vec<String> = Vec::new();
    for folder in members {
        let Some(toc) = &folder.toc else { continue };
        for dep in &toc.dependencies {
            let lower = dep.to_ascii_lowercase();
            if lower.starts_with("blizzard_") || installed.contains(&lower) {
                continue;
            }
            if !missing.iter().any(|m| m.eq_ignore_ascii_case(dep)) {
                missing.push(dep.clone());
            }
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::wow::toc::parse_toc;

    fn folder(name: &str, toc: &str) -> ScannedFolder {
        ScannedFolder {
            name: name.to_string(),
            toc_file: Some(format!("{name}.toc")),
            toc: Some(parse_toc(toc)),
            other_tocs: vec![],
        }
    }

    #[test]
    fn groups_modules_under_their_main_folder() {
        let folders = vec![
            folder("Ace3", "## Interface: 16001"),
            folder("BigWigs", "## Interface: 16001"),
            folder("BigWigs_Core", "## Dependencies: BigWigs"),
            folder("BigWigs_Plugins", "## Dependencies: BigWigs_Core"),
            folder("DBM-Core", "## Interface: 16001"),
            folder("DBM-StatusBarTimers", "## Dependencies: DBM-Core"),
            folder("MyAddon", "## Dependencies: Ace3"),
            folder("MyAddon_Config", "## Group: MyAddon"),
        ];
        let refs: Vec<&ScannedFolder> = folders.iter().collect();
        let groups = group_folders(&refs);
        assert!(groups.contains(&vec!["Ace3".to_string()]));
        assert!(groups.contains(&vec![
            "BigWigs".to_string(),
            "BigWigs_Core".to_string(),
            "BigWigs_Plugins".to_string()
        ]));
        assert!(groups.contains(&vec!["DBM-Core".to_string(), "DBM-StatusBarTimers".to_string()]));
        assert!(groups.contains(&vec!["MyAddon".to_string(), "MyAddon_Config".to_string()]));
        assert_eq!(groups.len(), 4);
    }

    #[test]
    fn judges_compatibility_against_the_installed_patch() {
        let forever = crate::games::wow::flavor::Flavor::Forever.info();
        let current = folder("A", "## Interface: 16001");
        let older = folder("B", "## Interface: 16000");
        let retail = folder("C", "## Interface: 120105");
        assert_eq!(current.compatibility(forever, Some(16001)), Compatibility::Compatible);
        assert_eq!(older.compatibility(forever, Some(16001)), Compatibility::OlderPatch);
        assert_eq!(older.compatibility(forever, None), Compatibility::Compatible, "unknown client version");
        assert_eq!(retail.compatibility(forever, Some(16001)), Compatibility::OutOfDate);
        let none = ScannedFolder { name: "D".into(), toc_file: None, toc: None, other_tocs: vec![] };
        assert_eq!(none.compatibility(forever, Some(16001)), Compatibility::Missing);
    }

    #[test]
    fn reports_missing_dependencies() {
        let folders = vec![folder("Plugin", "## Dependencies: Core, Blizzard_ObjectiveTracker, Ace3")];
        let installed: HashSet<String> = ["plugin".to_string(), "ace3".to_string()].into();
        let refs: Vec<&ScannedFolder> = folders.iter().collect();
        assert_eq!(missing_dependencies(&refs, &installed), vec!["Core".to_string()]);
    }
}
