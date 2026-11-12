//! Finding World of Warcraft installs: Windows registry, Battle.net's own records,
//! and the usual install locations on every drive.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use regex::Regex;

use super::flavor::{folder_variant, patch_to_interface, Flavor};

pub struct FoundClient {
    pub flavor: Flavor,
    pub label: String,
    pub path: PathBuf,
    pub addon_count: usize,
    /// Client version from Blizzard's `.build.info`, e.g. `1.60.1.70888`.
    pub version: Option<String>,
}

/// What Blizzard's own files say about a client folder.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClientInfo {
    /// Battle.net product code from `.flavor.info`, e.g. `wowf` for WoW Forever.
    pub product: Option<String>,
    pub version: Option<String>,
}

impl ClientInfo {
    /// The interface number the client expects: 16001 for version 1.60.1.
    pub fn interface(&self) -> Option<u32> {
        let version = self.version.as_deref()?;
        let patch: Vec<&str> = version.split('.').take(3).collect();
        patch_to_interface(&patch.join("."))
    }
}

/// Reads `<client>/.flavor.info` and the matching row of `<root>/.build.info`.
pub fn client_info(client_dir: &Path) -> ClientInfo {
    let product = read_flavor_info(client_dir);
    let version = client_dir.parent().and_then(|root| read_build_version(root, product.as_deref()));
    ClientInfo { product, version }
}

/// `.flavor.info` holds a header line (`Product Flavor!STRING:0`) and the product code.
fn read_flavor_info(client_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(client_dir.join(".flavor.info")).ok()?;
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.contains('!'))
        .last()
        .map(str::to_ascii_lowercase)
}

/// `.build.info` is a pipe-separated table whose header names columns like `Version!STRING:0`.
fn read_build_version(root: &Path, product: Option<&str>) -> Option<String> {
    let text = std::fs::read_to_string(root.join(".build.info")).ok()?;
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let header: Vec<String> = lines
        .next()?
        .split('|')
        .map(|column| column.split('!').next().unwrap_or("").trim().to_ascii_lowercase())
        .collect();
    let version_column = header.iter().position(|h| h == "version")?;
    let product_column = header.iter().position(|h| h == "product");
    let active_column = header.iter().position(|h| h == "active");
    lines
        .map(|line| line.split('|').map(str::trim).collect::<Vec<_>>())
        .filter(|row| match (product, product_column) {
            (Some(product), Some(column)) => row.get(column).map(|v| v.eq_ignore_ascii_case(product)).unwrap_or(false),
            _ => true,
        })
        .max_by_key(|row| active_column.and_then(|c| row.get(c)).map(|v| *v == "1").unwrap_or(false))
        .and_then(|row| row.get(version_column).map(|v| v.to_string()))
        .filter(|version| !version.is_empty())
}

/// Mixed separators such as `H:/Games/x\y\` become `H:\Games\x\y` on Windows, so
/// paths display and compare cleanly.
pub fn tidy(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let text = if cfg!(windows) { text.replace('/', "\\") } else { text.into_owned() };
    let trimmed = text.trim_end_matches(['\\', '/']);
    if trimmed.len() == 2 && trimmed.ends_with(':') {
        PathBuf::from(format!("{trimmed}\\"))
    } else if trimmed.is_empty() {
        PathBuf::from(text)
    } else {
        PathBuf::from(trimmed)
    }
}

/// A comparable key for a path: tidied and lower-cased.
pub fn path_key(path: &Path) -> String {
    tidy(path).to_string_lossy().to_lowercase()
}

/// Every WoW client folder found on this machine.
pub fn detect_all() -> Vec<FoundClient> {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for root in candidate_roots() {
        for client in clients_in_root(&root) {
            if seen.insert(path_key(&client.path)) {
                found.push(client);
            }
        }
    }
    found.sort_by_key(|c| (c.flavor != Flavor::Forever, c.label.clone()));
    found
}

/// Interprets a folder the user picked: the WoW folder, a client folder such as
/// `_forever_`, or its `Interface` / `Interface\AddOns` folder.
pub fn resolve_user_path(path: &Path) -> Result<Vec<FoundClient>> {
    let lower = |p: &Path| p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let mut client_dir = path.to_path_buf();
    if lower(&client_dir) == "addons" {
        client_dir = client_dir.parent().map(Path::to_path_buf).unwrap_or(client_dir);
    }
    if lower(&client_dir) == "interface" {
        client_dir = client_dir.parent().map(Path::to_path_buf).unwrap_or(client_dir);
    }
    if let Some(client) = client_at(&client_dir) {
        return Ok(vec![client]);
    }
    let clients = clients_in_root(path);
    if clients.is_empty() {
        bail!(
            "No WoW game folders found there. Choose your World of Warcraft folder, or the _forever_ folder inside it."
        );
    }
    Ok(clients)
}

pub fn clients_in_root(root: &Path) -> Vec<FoundClient> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| client_at(&e.path()))
        .collect()
}

fn client_at(dir: &Path) -> Option<FoundClient> {
    let folder = dir.file_name()?.to_string_lossy().into_owned();
    if !(folder.len() > 2 && folder.starts_with('_') && folder.ends_with('_')) {
        return None;
    }
    // Blizzard's product code is authoritative; the folder name is the fallback.
    let info = client_info(dir);
    let flavor = info
        .product
        .as_deref()
        .and_then(Flavor::from_product)
        .or_else(|| Flavor::from_folder(&folder))?;
    if !looks_like_client(dir) {
        return None;
    }
    let label = match folder_variant(&folder) {
        Some(variant) => format!("{} ({variant})", flavor.info().name),
        None => flavor.info().name.to_string(),
    };
    let addon_count = std::fs::read_dir(dir.join("Interface").join("AddOns"))
        .map(|entries| entries.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).count())
        .unwrap_or(0);
    Some(FoundClient { flavor, label, path: tidy(dir), addon_count, version: info.version })
}

fn looks_like_client(dir: &Path) -> bool {
    if dir.join("Interface").is_dir() || dir.join("WTF").is_dir() || dir.join(".flavor.info").is_file() {
        return true;
    }
    std::fs::read_dir(dir)
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                let name = e.file_name().to_string_lossy().to_ascii_lowercase();
                (name.starts_with("wow") && name.ends_with(".exe")) || name.ends_with(".app")
            })
        })
        .unwrap_or(false)
}

/// Candidate "World of Warcraft" folders, most authoritative first.
pub fn candidate_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    roots.extend(registry_roots());
    roots.extend(battlenet_roots());
    roots.extend(common_roots());

    let mut seen = HashSet::new();
    roots
        .into_iter()
        .map(|p| tidy(&strip_client_folder(&p)))
        .filter(|p| p.is_dir())
        .filter(|p| seen.insert(path_key(p)))
        .collect()
}

/// `...\World of Warcraft\_retail_` -> `...\World of Warcraft`.
fn strip_client_folder(path: &Path) -> PathBuf {
    let trimmed = PathBuf::from(path.to_string_lossy().trim_end_matches(['\\', '/']));
    match trimmed.file_name().map(|n| n.to_string_lossy().into_owned()) {
        Some(name) if name.starts_with('_') && name.ends_with('_') => {
            trimmed.parent().map(Path::to_path_buf).unwrap_or(trimmed)
        }
        _ => trimmed,
    }
}

#[cfg(windows)]
fn registry_roots() -> Vec<PathBuf> {
    use windows_registry::LOCAL_MACHINE;
    let mut roots = Vec::new();
    for key in [
        r"SOFTWARE\WOW6432Node\Blizzard Entertainment\World of Warcraft",
        r"SOFTWARE\Blizzard Entertainment\World of Warcraft",
    ] {
        if let Ok(k) = LOCAL_MACHINE.open(key) {
            for value in ["InstallPath", "GamePath"] {
                if let Ok(path) = k.get_string(value) {
                    let path = PathBuf::from(path.trim());
                    roots.push(if path.extension().is_some() {
                        path.parent().map(Path::to_path_buf).unwrap_or(path)
                    } else {
                        path
                    });
                }
            }
        }
    }
    for uninstall in [
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    ] {
        let Ok(parent) = LOCAL_MACHINE.open(uninstall) else { continue };
        let Ok(names) = parent.keys() else { continue };
        for name in names {
            let Ok(app) = parent.open(&name) else { continue };
            let display = app.get_string("DisplayName").unwrap_or_default();
            if display.contains("World of Warcraft") {
                if let Ok(location) = app.get_string("InstallLocation") {
                    roots.push(PathBuf::from(location.trim()));
                }
            }
        }
    }
    roots
}

#[cfg(not(windows))]
fn registry_roots() -> Vec<PathBuf> {
    Vec::new()
}

/// Paths recorded by the Battle.net app: its agent database and its config file.
fn battlenet_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        let program_data = std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        files.push(program_data.join("Battle.net").join("Agent").join("product.db"));
    } else {
        files.push(PathBuf::from("/Users/Shared/Battle.net/Agent/product.db"));
    }
    if let Some(config) = dirs::config_dir() {
        files.push(config.join("Battle.net").join("Battle.net.config"));
    }
    let pattern = Regex::new(r#"(?:[A-Za-z]:[\\/]|/)[^\x00-\x1f"<>|*?]{0,240}?World of Warcraft"#).unwrap();
    for file in files {
        let Ok(bytes) = std::fs::read(&file) else { continue };
        let text = String::from_utf8_lossy(&bytes).replace("\\\\", "\\");
        for m in pattern.find_iter(&text) {
            roots.push(PathBuf::from(m.as_str()));
        }
        // Battle.net.config stores the default install folder for new games.
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            collect_install_paths(&json, &mut roots);
        }
    }
    roots
}

fn collect_install_paths(value: &serde_json::Value, out: &mut Vec<PathBuf>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, v) in map {
                if key.to_ascii_lowercase().contains("installpath") {
                    if let Some(s) = v.as_str() {
                        out.push(PathBuf::from(s).join("World of Warcraft"));
                    }
                }
                collect_install_paths(v, out);
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|v| collect_install_paths(v, out)),
        _ => {}
    }
}

fn common_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(windows) {
        let suffixes = [
            r"World of Warcraft",
            r"Program Files (x86)\World of Warcraft",
            r"Program Files\World of Warcraft",
            r"Games\World of Warcraft",
            r"Games\Blizzard\World of Warcraft",
            r"Games\Battle.net\World of Warcraft",
            r"Battle.net\World of Warcraft",
            r"Blizzard\World of Warcraft",
            r"Blizzard Games\World of Warcraft",
            r"Program Files (x86)\Battle.net\World of Warcraft",
        ];
        for letter in b'C'..=b'Z' {
            let drive = PathBuf::from(format!("{}:\\", letter as char));
            if !drive.exists() {
                continue;
            }
            for suffix in suffixes {
                roots.push(drive.join(suffix));
            }
        }
    } else {
        roots.push(PathBuf::from("/Applications/World of Warcraft"));
        if let Some(home) = dirs::home_dir() {
            roots.push(home.join("Applications").join("World of Warcraft"));
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_clients_and_resolves_picked_folders() {
        let root = tempfile::tempdir().unwrap();
        let forever = root.path().join("_forever_");
        std::fs::create_dir_all(forever.join("Interface").join("AddOns").join("Questie")).unwrap();
        std::fs::create_dir_all(root.path().join("_retail_").join("WTF")).unwrap();
        std::fs::create_dir_all(root.path().join("_unknown_").join("WTF")).unwrap();

        let clients = clients_in_root(root.path());
        assert_eq!(clients.len(), 2);

        let picked = resolve_user_path(&forever.join("Interface").join("AddOns")).unwrap();
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].flavor, Flavor::Forever);
        assert_eq!(picked[0].addon_count, 1);
        assert_eq!(picked[0].label, "WoW Forever");

        assert!(resolve_user_path(&root.path().join("_unknown_")).is_err());
        assert_eq!(strip_client_folder(&forever), root.path().to_path_buf());
    }

    #[test]
    fn reads_product_and_version_from_blizzard_files() {
        let root = tempfile::tempdir().unwrap();
        // A client in an unfamiliar folder is still recognised by its product code.
        let client = root.path().join("_renamed_");
        std::fs::create_dir_all(client.join("WTF")).unwrap();
        std::fs::write(client.join(".flavor.info"), "Product Flavor!STRING:0\nwowf\n").unwrap();
        std::fs::write(
            root.path().join(".build.info"),
            "Branch!STRING:0|Active!DEC:1|Build Key!HEX:16|Version!STRING:0|Product!STRING:0\n\
             us|1|00aa|12.1.5.60000|wow\n\
             us|1|00bb|1.60.1.70888|wowf\n",
        )
        .unwrap();

        let info = client_info(&client);
        assert_eq!(info.product.as_deref(), Some("wowf"));
        assert_eq!(info.version.as_deref(), Some("1.60.1.70888"));
        assert_eq!(info.interface(), Some(16001));

        let found = clients_in_root(root.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].flavor, Flavor::Forever);
        assert_eq!(found[0].version.as_deref(), Some("1.60.1.70888"));
    }

    #[cfg(windows)]
    #[test]
    fn tidies_mixed_separators() {
        assert_eq!(
            tidy(Path::new("H:/Games/blizzard/World of Warcraft\\_forever_\\")),
            PathBuf::from(r"H:\Games\blizzard\World of Warcraft\_forever_")
        );
        assert_eq!(tidy(Path::new("C:/")), PathBuf::from(r"C:\"));
        assert_eq!(path_key(Path::new("H:/Games/WoW")), path_key(Path::new(r"h:\games\wow\")));
    }
}
