//! Backups of the `WTF` folder: keybindings, macros, game settings and every
//! addon's saved settings. Zipped, timestamped, pruned, restorable.

use std::fs::{self, File};
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Serialize;
use zip::write::SimpleFileOptions;

use crate::install::{move_dir, remove_dir_all_retry};
use crate::model::{Installation, Settings};

/// Why a backup was taken. Manual backups are never pruned automatically.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BackupReason {
    Manual,
    Launch,
    Update,
    Restore,
}

impl BackupReason {
    fn tag(self) -> &'static str {
        match self {
            BackupReason::Manual => "manual",
            BackupReason::Launch => "launch",
            BackupReason::Update => "update",
            BackupReason::Restore => "restore",
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "manual" => Some(BackupReason::Manual),
            "launch" => Some(BackupReason::Launch),
            "update" => Some(BackupReason::Update),
            "restore" => Some(BackupReason::Restore),
            _ => None,
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub id: String,
    pub path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub size: u64,
    pub reason: BackupReason,
}

const STAMP: &str = "%Y%m%dT%H%M%SZ";

/// `<backup folder>/<installation id>`; the backup folder defaults to the app data folder.
pub fn backup_dir(settings: &Settings, data_dir: &Path, installation: &Installation) -> PathBuf {
    let base = settings
        .backup_dir
        .as_deref()
        .filter(|d| !d.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("backups"));
    base.join(&installation.id)
}

fn parse_name(name: &str) -> Option<(DateTime<Utc>, BackupReason)> {
    let rest = name.strip_prefix("WTF-")?.strip_suffix(".zip")?;
    let (stamp, tail) = rest.split_once('-')?;
    let reason_tag = tail.split('-').next()?;
    let time = NaiveDateTime::parse_from_str(stamp, STAMP).ok()?.and_utc();
    Some((time, BackupReason::from_tag(reason_tag)?))
}

/// Zips the WTF folder. Returns `None` when the game hasn't created one yet.
pub fn create(dir: &Path, installation: &Installation, reason: BackupReason) -> Result<Option<BackupInfo>> {
    let wtf = installation.wtf_dir();
    if !wtf.is_dir() {
        return Ok(None);
    }
    fs::create_dir_all(dir).context("Couldn't create the backup folder")?;
    let now = Utc::now();
    let mut name = format!("WTF-{}-{}.zip", now.format(STAMP), reason.tag());
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("WTF-{}-{}-{n}.zip", now.format(STAMP), reason.tag());
        n += 1;
    }
    let final_path = dir.join(&name);
    let partial = dir.join(format!("{name}.partial"));

    let result = (|| -> Result<()> {
        let mut writer = zip::ZipWriter::new(BufWriter::new(File::create(&partial)?));
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .large_file(true);
        for entry in walkdir::WalkDir::new(&wtf).into_iter() {
            let entry = entry.context("Couldn't read the WTF folder")?;
            if !entry.file_type().is_file() {
                continue;
            }
            let relative = entry.path().strip_prefix(&wtf)?;
            let zip_name = Path::new("WTF").join(relative).to_string_lossy().replace('\\', "/");
            writer.start_file(zip_name, options)?;
            let mut source = File::open(entry.path())
                .with_context(|| format!("Couldn't read {}", entry.path().display()))?;
            io::copy(&mut source, &mut writer)?;
        }
        writer.finish()?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&partial);
        return Err(error).context("Couldn't back up your settings");
    }
    fs::rename(&partial, &final_path).context("Couldn't finish the backup")?;
    let size = fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0);
    Ok(Some(BackupInfo { id: name, path: final_path, created_at: now, size, reason }))
}

pub fn list(dir: &Path) -> Result<Vec<BackupInfo>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).context("Couldn't read the backup folder"),
    };
    let mut backups: Vec<BackupInfo> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let (created_at, reason) = parse_name(&name)?;
            Some(BackupInfo {
                id: name,
                path: e.path(),
                created_at,
                size: e.metadata().map(|m| m.len()).unwrap_or(0),
                reason,
            })
        })
        .collect();
    backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(backups)
}

/// Keeps the newest `keep` automatic backups; manual ones are left alone.
pub fn prune(dir: &Path, keep: usize) -> Result<usize> {
    let mut removed = 0;
    for backup in list(dir)?.into_iter().filter(|b| b.reason != BackupReason::Manual).skip(keep.max(1)) {
        if fs::remove_file(&backup.path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

fn checked_id(id: &str) -> Result<&str> {
    if id.contains(['/', '\\']) || id.contains("..") || parse_name(id).is_none() {
        bail!("That isn't a Hearthforge backup.");
    }
    Ok(id)
}

pub fn delete(dir: &Path, id: &str) -> Result<()> {
    fs::remove_file(dir.join(checked_id(id)?)).context("Couldn't delete the backup")
}

/// Replaces the WTF folder with the backup's contents. A safety backup of the
/// current folder is taken first, and the current folder is only swapped out
/// once extraction has fully succeeded.
pub fn restore(dir: &Path, installation: &Installation, id: &str) -> Result<Option<BackupInfo>> {
    let zip_path = dir.join(checked_id(id)?);
    if !zip_path.is_file() {
        bail!("That backup no longer exists.");
    }
    let safety = create(dir, installation, BackupReason::Restore)?;

    let staging = installation.path.join(format!(".hearthforge-restore-{}", uuid::Uuid::new_v4().simple()));
    let result = (|| -> Result<()> {
        let mut archive = zip::ZipArchive::new(File::open(&zip_path)?).context("The backup file is damaged")?;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            if entry.is_dir() {
                continue;
            }
            let Some(path) = entry.enclosed_name() else { continue };
            let Ok(relative) = path.strip_prefix("WTF") else { continue };
            let target = staging.join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            io::copy(&mut entry, &mut File::create(&target)?)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = remove_dir_all_retry(&staging);
        return Err(error).context("Couldn't restore the backup");
    }

    let wtf = installation.wtf_dir();
    let previous = installation.path.join(format!(".hearthforge-wtf-{}", uuid::Uuid::new_v4().simple()));
    if wtf.exists() {
        move_dir(&wtf, &previous).context("Couldn't move the current WTF folder aside. Is the game running?")?;
    }
    if let Err(error) = move_dir(&staging, &wtf) {
        let _ = move_dir(&previous, &wtf);
        let _ = remove_dir_all_retry(&staging);
        return Err(anyhow!(error)).context("Couldn't put the restored settings in place");
    }
    let _ = remove_dir_all_retry(&previous);
    Ok(safety)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::wow::flavor::Flavor;
    use crate::model::LaunchMethod;

    fn installation(path: &Path) -> Installation {
        Installation {
            id: "test".into(),
            game: "wow".into(),
            flavor: Flavor::Forever,
            label: "WoW Forever".into(),
            path: path.to_path_buf(),
            launch_method: LaunchMethod::Battlenet,
            default_channel: None,
            added_at: None,
        }
    }

    #[test]
    fn backs_up_lists_prunes_and_restores() {
        let tmp = tempfile::tempdir().unwrap();
        let client = tmp.path().join("_forever_");
        let saved = client.join("WTF").join("Account").join("ME").join("SavedVariables");
        fs::create_dir_all(&saved).unwrap();
        fs::write(saved.join("Questie.lua"), "QuestieConfig = { v = 1 }").unwrap();
        let inst = installation(&client);
        let dir = tmp.path().join("backups");

        let first = create(&dir, &inst, BackupReason::Manual).unwrap().unwrap();
        assert!(first.size > 0);
        for _ in 0..3 {
            create(&dir, &inst, BackupReason::Launch).unwrap();
        }
        assert_eq!(list(&dir).unwrap().len(), 4);
        prune(&dir, 1).unwrap();
        let remaining = list(&dir).unwrap();
        assert_eq!(remaining.len(), 2, "keeps one automatic backup plus the manual one");
        assert!(remaining.iter().any(|b| b.reason == BackupReason::Manual));

        fs::write(saved.join("Questie.lua"), "QuestieConfig = { v = 2 }").unwrap();
        fs::write(saved.join("New.lua"), "x").unwrap();
        let safety = restore(&dir, &inst, &first.id).unwrap();
        assert!(safety.is_some());
        assert_eq!(fs::read_to_string(saved.join("Questie.lua")).unwrap(), "QuestieConfig = { v = 1 }");
        assert!(!saved.join("New.lua").exists());

        assert!(delete(&dir, "../../etc/passwd").is_err());
        delete(&dir, &first.id).unwrap();
    }

    #[test]
    fn skips_clients_without_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let inst = installation(&tmp.path().join("_forever_"));
        assert!(create(&tmp.path().join("b"), &inst, BackupReason::Launch).unwrap().is_none());
    }
}
