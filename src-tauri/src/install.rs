//! Installing addon archives into `Interface/AddOns`.
//!
//! Archives are extracted into a staging folder beside AddOns (same drive, so the
//! final step is a rename). Existing folders are moved aside, new ones moved in,
//! and any failure puts the old folders back. Paths that would escape the target
//! ("zip slip") are ignored.

use std::fs::{self, File};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::games::wow::toc::toc_base_name;

const MAX_ENTRIES: usize = 50_000;
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// An addon folder inside an archive: its location there, and its name in AddOns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveRoot {
    pub prefix: PathBuf,
    pub folder: String,
}

/// Finds the addon folders in an archive from its file list. A folder counts when
/// it holds a TOC named after it (`Questie/Questie_Forever.toc`); TOCs nested inside
/// another addon (bundled libraries) are ignored.
pub fn find_addon_roots(files: &[PathBuf]) -> Vec<ArchiveRoot> {
    let tocs: Vec<(&PathBuf, PathBuf, String, String)> = files
        .iter()
        .filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("toc")).unwrap_or(false))
        .filter_map(|p| {
            let stem = p.file_stem()?.to_str()?.to_string();
            let base = toc_base_name(&stem).to_string();
            let dir = p.parent().map(Path::to_path_buf).unwrap_or_default();
            Some((p, dir, stem, base))
        })
        .collect();

    let mut candidates: Vec<ArchiveRoot> = Vec::new();
    for (_, dir, stem, base) in &tocs {
        match dir.file_name().and_then(|n| n.to_str()) {
            Some(folder) if folder.eq_ignore_ascii_case(base) || folder.eq_ignore_ascii_case(stem) => {
                candidates.push(ArchiveRoot { prefix: dir.clone(), folder: folder.to_string() });
            }
            None => candidates.push(ArchiveRoot { prefix: PathBuf::new(), folder: base.clone() }),
            _ => {}
        }
    }
    if candidates.is_empty() {
        // e.g. a GitHub source zip: `Repo-main/MyAddon.toc`. Install the folder under the TOC's name.
        let shallowest = tocs.iter().map(|(_, dir, _, _)| dir.components().count()).min();
        for (_, dir, _, base) in tocs.iter().filter(|(_, dir, _, _)| Some(dir.components().count()) == shallowest) {
            candidates.push(ArchiveRoot { prefix: dir.clone(), folder: base.clone() });
        }
    }

    candidates.sort_by_key(|c| c.prefix.components().count());
    let mut accepted: Vec<ArchiveRoot> = Vec::new();
    for candidate in candidates {
        let nested = accepted.iter().any(|a| candidate.prefix.starts_with(&a.prefix));
        let duplicate = accepted.iter().any(|a| a.folder.eq_ignore_ascii_case(&candidate.folder));
        if !nested && !duplicate {
            accepted.push(candidate);
        }
    }
    accepted
}

fn is_junk(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::Normal(n) if n == "__MACOSX"))
        || path.file_name().map(|n| n == ".DS_Store").unwrap_or(false)
}

/// Extracts the addon folders of `zip_path` into `dest`. Returns their folder names.
pub fn extract_addons(zip_path: &Path, dest: &Path) -> Result<Vec<String>> {
    let file = File::open(zip_path).context("Couldn't open the downloaded file")?;
    let mut archive = zip::ZipArchive::new(file).context("The download isn't a valid zip file")?;
    if archive.len() > MAX_ENTRIES {
        bail!("The archive has too many files to be an addon.");
    }

    let mut entries: Vec<(usize, PathBuf)> = Vec::new();
    let mut total: u64 = 0;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).context("The zip file is damaged")?;
        if entry.is_dir() {
            continue;
        }
        let Some(path) = entry.enclosed_name() else { continue };
        if is_junk(&path) {
            continue;
        }
        total = total.saturating_add(entry.size());
        entries.push((index, path));
    }
    if total > MAX_TOTAL_BYTES {
        bail!("The archive is far too large to be an addon.");
    }

    let files: Vec<PathBuf> = entries.iter().map(|(_, p)| p.clone()).collect();
    let roots = find_addon_roots(&files);
    if roots.is_empty() {
        bail!("This file doesn't contain a WoW addon: no .toc file was found.");
    }

    for (index, path) in &entries {
        let Some(root) = roots.iter().find(|r| path.starts_with(&r.prefix)) else { continue };
        let Ok(relative) = path.strip_prefix(&root.prefix) else { continue };
        let target = dest.join(&root.folder).join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).context("Couldn't create a folder while extracting")?;
        }
        let mut entry = archive.by_index(*index).context("The zip file is damaged")?;
        let mut out = File::create(&target).with_context(|| format!("Couldn't write {}", target.display()))?;
        io::copy(&mut entry, &mut out).context("Couldn't extract the zip file")?;
    }
    Ok(roots.into_iter().map(|r| r.folder).collect())
}

/// Where in-progress installs are staged: `Interface/.hearthforge`, beside AddOns.
pub fn staging_root(addons_dir: &Path) -> PathBuf {
    addons_dir.parent().unwrap_or(addons_dir).join(".hearthforge")
}

/// Installs the addon archive, replacing `replace` (the previous version's folders)
/// and any folders the archive brings. Returns the installed folder names.
pub fn install_archive(zip_path: &Path, addons_dir: &Path, replace: &[String]) -> Result<Vec<String>> {
    fs::create_dir_all(addons_dir).context("Couldn't create the AddOns folder")?;
    let work = staging_root(addons_dir).join(uuid::Uuid::new_v4().simple().to_string());
    let new_dir = work.join("new");
    let old_dir = work.join("old");
    fs::create_dir_all(&new_dir).context("Couldn't create a staging folder")?;
    fs::create_dir_all(&old_dir).context("Couldn't create a staging folder")?;

    let result = extract_addons(zip_path, &new_dir)
        .and_then(|folders| swap_in(addons_dir, &new_dir, &old_dir, &folders, replace).map(|_| folders));
    let _ = remove_dir_all_retry(&work);
    cleanup_staging_root(addons_dir);
    result
}

fn swap_in(addons_dir: &Path, new_dir: &Path, old_dir: &Path, folders: &[String], replace: &[String]) -> Result<()> {
    let mut aside: Vec<&String> = Vec::new();
    for name in replace.iter().chain(folders.iter()) {
        if !aside.iter().any(|a| a.eq_ignore_ascii_case(name)) {
            aside.push(name);
        }
    }

    let mut moved_out: Vec<String> = Vec::new();
    for name in aside {
        let current = addons_dir.join(name);
        if !current.exists() {
            continue;
        }
        if let Err(error) = move_dir(&current, &old_dir.join(name)) {
            restore(addons_dir, old_dir, &moved_out);
            return Err(error)
                .with_context(|| format!("Couldn't replace {name}. Close anything using the AddOns folder and try again"));
        }
        moved_out.push(name.clone());
    }

    let mut moved_in: Vec<&String> = Vec::new();
    for name in folders {
        if let Err(error) = move_dir(&new_dir.join(name), &addons_dir.join(name)) {
            for installed in &moved_in {
                let _ = remove_dir_all_retry(&addons_dir.join(installed));
            }
            restore(addons_dir, old_dir, &moved_out);
            return Err(error).with_context(|| format!("Couldn't install {name}"));
        }
        moved_in.push(name);
    }
    Ok(())
}

fn restore(addons_dir: &Path, old_dir: &Path, names: &[String]) {
    for name in names {
        if let Err(error) = move_dir(&old_dir.join(name), &addons_dir.join(name)) {
            log::error!("Couldn't restore {name} after a failed install: {error}");
        }
    }
}

/// Deletes addon folders, all or nothing: they're moved aside first.
pub fn remove_folders(addons_dir: &Path, folders: &[String]) -> Result<()> {
    let work = staging_root(addons_dir).join(uuid::Uuid::new_v4().simple().to_string());
    fs::create_dir_all(&work).context("Couldn't create a staging folder")?;
    let mut moved: Vec<String> = Vec::new();
    for name in folders {
        let path = addons_dir.join(name);
        if !path.exists() {
            continue;
        }
        if let Err(error) = move_dir(&path, &work.join(name)) {
            restore(addons_dir, &work, &moved);
            let _ = fs::remove_dir_all(&work);
            return Err(error)
                .with_context(|| format!("Couldn't remove {name}. Close anything using the AddOns folder and try again"));
        }
        moved.push(name.clone());
    }
    if let Err(error) = remove_dir_all_retry(&work) {
        log::warn!("Leftover files in {}: {error}", work.display());
    }
    cleanup_staging_root(addons_dir);
    Ok(())
}

fn cleanup_staging_root(addons_dir: &Path) {
    // Only removes the folder when it's empty.
    let _ = fs::remove_dir(staging_root(addons_dir));
}

fn is_cross_device(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::CrossesDevices {
        return true;
    }
    #[cfg(windows)]
    let code = 17; // ERROR_NOT_SAME_DEVICE
    #[cfg(not(windows))]
    let code = 18; // EXDEV
    error.raw_os_error() == Some(code)
}

/// Locked or busy files: antivirus scans, Explorer previews, a running indexer.
fn is_transient(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied || matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33))
}

/// Renames a folder, retrying briefly on locks and copying when it crosses drives.
pub fn move_dir(from: &Path, to: &Path) -> io::Result<()> {
    let mut attempt: u64 = 0;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) if is_cross_device(&error) => {
                copy_dir(from, to)?;
                return remove_dir_all_retry(from);
            }
            Err(error) if attempt < 5 && is_transient(&error) => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(120 * attempt));
            }
            Err(error) => return Err(error),
        }
    }
}

pub fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry.map_err(io::Error::other)?;
        let relative = entry.path().strip_prefix(from).map_err(io::Error::other)?;
        let target = to.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// `remove_dir_all` that clears read-only flags and retries on transient locks.
pub fn remove_dir_all_retry(path: &Path) -> io::Result<()> {
    let mut attempt: u64 = 0;
    loop {
        match fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) if attempt < 5 && is_transient(&error) => {
                attempt += 1;
                clear_readonly(path);
                std::thread::sleep(Duration::from_millis(120 * attempt));
            }
            Err(error) => return Err(error),
        }
    }
}

fn clear_readonly(path: &Path) {
    for entry in walkdir::WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        if let Ok(metadata) = entry.metadata() {
            let mut permissions = metadata.permissions();
            if permissions.readonly() {
                #[allow(clippy::permissions_set_readonly_false)]
                permissions.set_readonly(false);
                let _ = fs::set_permissions(entry.path(), permissions);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn make_zip(dir: &Path, files: &[(&str, &str)]) -> PathBuf {
        let path = dir.join(format!("{}.zip", uuid::Uuid::new_v4().simple()));
        let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, content) in files {
            writer.start_file(*name, SimpleFileOptions::default()).unwrap();
            writer.write_all(content.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
        path
    }

    fn roots(files: &[&str]) -> Vec<String> {
        let paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
        find_addon_roots(&paths).into_iter().map(|r| r.folder).collect()
    }

    #[test]
    fn finds_addon_folders_in_common_layouts() {
        assert_eq!(
            roots(&["Questie/Questie.toc", "Questie/Questie_Forever.toc", "Questie/Libs/LibStub/LibStub.toc"]),
            vec!["Questie"]
        );
        assert_eq!(
            roots(&["BigWigs/BigWigs.toc", "BigWigs_Core/BigWigs_Core_Mainline.toc", "BigWigs_Core/x.lua"]),
            vec!["BigWigs", "BigWigs_Core"]
        );
        assert_eq!(roots(&["MyAddon.toc", "core.lua"]), vec!["MyAddon"]);
        assert_eq!(roots(&["Pack-1.0/AddonA/AddonA.toc", "Pack-1.0/AddonB/AddonB.toc"]), vec!["AddonA", "AddonB"]);
        assert_eq!(roots(&["Repo-main/Addon.toc", "Repo-main/core.lua"]), vec!["Addon"]);
        assert!(roots(&["readme.txt"]).is_empty());
    }

    #[test]
    fn installs_replaces_and_cleans_up() {
        let tmp = tempfile::tempdir().unwrap();
        let addons = tmp.path().join("Interface").join("AddOns");
        fs::create_dir_all(addons.join("Questie")).unwrap();
        fs::write(addons.join("Questie").join("old.lua"), "old").unwrap();
        fs::create_dir_all(addons.join("Questie_Legacy")).unwrap();
        fs::create_dir_all(addons.join("Unrelated")).unwrap();

        let zip = make_zip(
            tmp.path(),
            &[
                ("Questie/Questie.toc", "## Interface: 16001\n## Version: 2.0"),
                ("Questie/Questie.lua", "new"),
                ("__MACOSX/Questie/._Questie.lua", "junk"),
                ("README.md", "top-level files are ignored"),
            ],
        );
        let folders = install_archive(&zip, &addons, &["Questie".into(), "Questie_Legacy".into()]).unwrap();
        assert_eq!(folders, vec!["Questie"]);
        assert!(addons.join("Questie").join("Questie.lua").exists());
        assert!(!addons.join("Questie").join("old.lua").exists());
        assert!(!addons.join("Questie_Legacy").exists());
        assert!(addons.join("Unrelated").exists());
        assert!(!staging_root(&addons).exists(), "staging folder should be cleaned up");
    }

    #[test]
    fn rejects_archives_without_addons_and_keeps_existing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let addons = tmp.path().join("Interface").join("AddOns");
        fs::create_dir_all(addons.join("Keep")).unwrap();
        let zip = make_zip(tmp.path(), &[("notes.txt", "hello")]);
        let error = install_archive(&zip, &addons, &["Keep".into()]).unwrap_err();
        assert!(format!("{error:#}").contains("no .toc file"));
        assert!(addons.join("Keep").exists());
    }

    #[test]
    fn ignores_paths_that_escape_the_target() {
        let tmp = tempfile::tempdir().unwrap();
        let addons = tmp.path().join("Interface").join("AddOns");
        let zip = make_zip(
            tmp.path(),
            &[("Evil/Evil.toc", "## Interface: 16001"), ("Evil/../../../escaped.txt", "nope")],
        );
        install_archive(&zip, &addons, &[]).unwrap();
        assert!(addons.join("Evil").join("Evil.toc").exists());
        assert!(!tmp.path().join("escaped.txt").exists());
        assert!(!tmp.path().join("Interface").join("escaped.txt").exists());
    }

    #[test]
    fn removes_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let addons = tmp.path().join("AddOns");
        fs::create_dir_all(addons.join("A").join("sub")).unwrap();
        fs::create_dir_all(addons.join("B")).unwrap();
        remove_folders(&addons, &["A".into(), "Missing".into()]).unwrap();
        assert!(!addons.join("A").exists());
        assert!(addons.join("B").exists());
    }
}
