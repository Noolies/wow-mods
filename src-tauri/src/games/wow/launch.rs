//! Starting the game, either through the Battle.net app or directly.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::model::{Installation, LaunchMethod};

/// Executable names Blizzard has used for WoW clients, preferred in this order.
const KNOWN_EXES: &[&str] = &[
    "WowForever.exe",
    "WowF.exe",
    "Wow.exe",
    "WowClassic.exe",
    "WowT.exe",
    "WowB.exe",
    "WowClassicT.exe",
    "WowClassicB.exe",
];

pub fn find_battlenet() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use windows_registry::LOCAL_MACHINE;
        for key in [
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Battle.net",
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Battle.net",
        ] {
            if let Ok(k) = LOCAL_MACHINE.open(key) {
                if let Ok(location) = k.get_string("InstallLocation") {
                    let exe = PathBuf::from(location.trim()).join("Battle.net.exe");
                    if exe.is_file() {
                        return Some(exe);
                    }
                }
            }
        }
        for candidate in [
            r"C:\Program Files (x86)\Battle.net\Battle.net.exe",
            r"C:\Program Files\Battle.net\Battle.net.exe",
        ] {
            let exe = PathBuf::from(candidate);
            if exe.is_file() {
                return Some(exe);
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        let app = PathBuf::from("/Applications/Battle.net.app");
        app.exists().then_some(app)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        None
    }
}

/// The game executable (Windows) or app bundle (macOS) in a client folder.
pub fn find_game_exe(dir: &Path) -> Option<PathBuf> {
    let names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    if cfg!(target_os = "macos") {
        return names.iter().find(|n| n.ends_with(".app")).map(|n| dir.join(n));
    }
    for known in KNOWN_EXES {
        if let Some(name) = names.iter().find(|n| n.eq_ignore_ascii_case(known)) {
            return Some(dir.join(name));
        }
    }
    names
        .iter()
        .find(|n| {
            let l = n.to_ascii_lowercase();
            l.starts_with("wow") && l.ends_with(".exe") && !l.contains("error") && !l.contains("crash")
        })
        .map(|n| dir.join(n))
}

/// Starts the game and returns a short description of how it was started.
pub fn launch(installation: &Installation) -> Result<&'static str> {
    let code = installation.flavor.info().battlenet_code;
    if installation.launch_method == LaunchMethod::Battlenet {
        if let (Some(code), Some(battlenet)) = (code, find_battlenet()) {
            launch_with_battlenet(&battlenet, code)?;
            return Ok("battlenet");
        }
    }
    let exe = find_game_exe(&installation.path).ok_or_else(|| {
        anyhow!(
            "Couldn't find the game in {}. Check the folder in Settings, or start the game from Battle.net.",
            installation.path.display()
        )
    })?;
    launch_direct(&exe, &installation.path)?;
    Ok("direct")
}

fn launch_with_battlenet(battlenet: &Path, code: &str) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new(battlenet)
            .raw_arg(format!("--exec=\"launch {code}\""))
            .spawn()
            .context("Couldn't start Battle.net")?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-a")
            .arg(battlenet)
            .arg("--args")
            .arg(format!("--exec=launch {code}"))
            .spawn()
            .context("Couldn't start Battle.net")?;
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (battlenet, code);
        anyhow::bail!("Launching through Battle.net isn't supported on this platform.");
    }
    Ok(())
}

fn launch_direct(exe: &Path, working_dir: &Path) -> Result<()> {
    if cfg!(target_os = "macos") {
        Command::new("open").arg("-a").arg(exe).spawn().context("Couldn't start the game")?;
    } else {
        Command::new(exe).current_dir(working_dir).spawn().context("Couldn't start the game")?;
    }
    Ok(())
}

/// True when a process running from the client folder is alive.
pub fn is_running(system: &mut System, installation: &Installation) -> bool {
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    let root = installation.path.to_string_lossy().to_ascii_lowercase();
    system.processes().values().any(|process| {
        let Some(exe) = process.exe() else { return false };
        let exe_l = exe.to_string_lossy().to_ascii_lowercase();
        let name = exe.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        // macOS bundles are named "World of Warcraft ..."; Windows executables start with "Wow".
        exe_l.starts_with(&root) && (cfg!(target_os = "macos") || name.starts_with("wow"))
    })
}
