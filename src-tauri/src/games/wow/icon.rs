//! The game's own icon, read from its executable at runtime. This is the same emblem
//! Battle.net shows for each game (WoW Forever's teal "W", retail's gold one, Classic's
//! green one), and reading it from the user's install means Hearthforge never ships
//! Blizzard's artwork itself.

use std::path::Path;

use anyhow::{Context, Result};
use base64::Engine;

use super::launch::find_game_exe;

/// Pixel size of the icon handed to the interface: sharp at 64 CSS pixels on 2x screens.
const ICON_SIZE: u32 = 128;

/// The client's icon as a PNG `data:` URL, or `None` when it can't be read
/// (no executable yet, a non-Windows build, or the file is locked mid-patch).
pub fn client_icon_data_url(client_dir: &Path) -> Option<String> {
    let exe = find_game_exe(client_dir)?;
    let is_exe = exe.extension().map(|e| e.eq_ignore_ascii_case("exe")).unwrap_or(false);
    if !is_exe {
        return None;
    }
    match exe_icon(&exe).and_then(|(bytes, mime)| thumbnail(&bytes, mime, ICON_SIZE)) {
        Ok(png) => Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))),
        Err(error) => {
            log::info!("No icon from {}: {error:#}", exe.display());
            None
        }
    }
}

/// Scales an icon image (PNG or ICO) down to a `size`-pixel PNG.
pub fn thumbnail(bytes: &[u8], mime: &str, size: u32) -> Result<Vec<u8>> {
    let format = if mime == "image/png" { image::ImageFormat::Png } else { image::ImageFormat::Ico };
    let decoded = image::load_from_memory_with_format(bytes, format).context("Couldn't decode the icon")?;
    let scaled = if decoded.width() > size || decoded.height() > size {
        decoded.resize(size, size, image::imageops::FilterType::Lanczos3)
    } else {
        decoded
    };
    let mut png = std::io::Cursor::new(Vec::new());
    scaled.write_to(&mut png, image::ImageFormat::Png).context("Couldn't encode the icon")?;
    Ok(png.into_inner())
}

/// The largest image of an executable's main icon: the PNG frame when there is one
/// (Windows stores 256px icons as PNG), otherwise the whole icon as an `.ico` file.
pub fn exe_icon(exe: &Path) -> Result<(Vec<u8>, &'static str)> {
    let map = pelite::FileMap::open(exe).context("Couldn't open the game executable")?;
    let file = pelite::PeFile::from_bytes(&map).context("That isn't a Windows executable")?;
    let resources = file.resources().context("The executable has no resources")?;
    // Windows uses the first icon group as the program's icon.
    let (_, group) = resources
        .icons()
        .filter_map(|icon| icon.ok())
        .next()
        .context("The executable has no icon")?;
    let best = group
        .entries()
        .iter()
        .max_by_key(|e| (if e.bWidth == 0 { 256 } else { u32::from(e.bWidth) }, e.wBitCount))
        .context("The icon has no images")?;
    let image = group.image(best.nId).context("The icon image is missing")?;
    if image.starts_with(b"\x89PNG") {
        return Ok((image.to_vec(), "image/png"));
    }
    let mut ico = Vec::new();
    group.write(&mut ico).context("Couldn't read the icon")?;
    Ok((ico, "image/x-icon"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_folders_without_a_game() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(client_icon_data_url(tmp.path()).is_none());
        std::fs::write(tmp.path().join("Wow.exe"), b"not really an exe").unwrap();
        assert!(client_icon_data_url(tmp.path()).is_none());
    }

    /// Reads a real Windows executable's icon (Windows only, always present).
    #[cfg(windows)]
    #[test]
    fn reads_an_icon_from_a_real_executable() {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
        let notepad = Path::new(&windir).join("System32").join("notepad.exe");
        if notepad.exists() {
            let (bytes, mime) = exe_icon(&notepad).expect("notepad has an icon");
            assert!(!bytes.is_empty());
            assert!(mime == "image/png" || mime == "image/x-icon");
            let png = thumbnail(&bytes, mime, 32).expect("scales down");
            let decoded = image::load_from_memory(&png).unwrap();
            assert!(decoded.width() <= 32 && decoded.height() <= 32);
        }
    }
}
