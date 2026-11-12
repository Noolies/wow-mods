//! Shareable addon setups ("packs"): a compact text code friends can paste to
//! install the same addons. JSON, deflated, base64url, prefixed with `HF1:`.

use std::io::{Read, Write};

use anyhow::{bail, Context, Result};
use base64::Engine;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};

use crate::games::wow::flavor::Flavor;
use crate::model::{Channel, ProviderId};

pub const PREFIX: &str = "HF1:";
const MAX_DECODED_BYTES: u64 = 256 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pack {
    pub v: u32,
    #[serde(default)]
    pub name: String,
    pub flavor: Flavor,
    pub addons: Vec<PackAddon>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PackAddon {
    #[serde(rename = "p")]
    pub provider: ProviderId,
    #[serde(rename = "i")]
    pub project_id: String,
    #[serde(rename = "n")]
    pub name: String,
    #[serde(rename = "c", default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<Channel>,
}

pub fn encode(pack: &Pack) -> Result<String> {
    let json = serde_json::to_vec(pack)?;
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&json)?;
    let compressed = encoder.finish()?;
    Ok(format!("{PREFIX}{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compressed)))
}

pub fn decode(code: &str) -> Result<Pack> {
    let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let Some(body) = compact.strip_prefix(PREFIX) else {
        bail!("That isn't a Hearthforge share code. Codes start with {PREFIX}");
    };
    let compressed = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(body.trim_end_matches('='))
        .context("The share code is incomplete. Copy the whole code and try again.")?;
    let mut json = Vec::new();
    DeflateDecoder::new(compressed.as_slice())
        .take(MAX_DECODED_BYTES)
        .read_to_end(&mut json)
        .context("The share code is damaged. Copy the whole code and try again.")?;
    let pack: Pack = serde_json::from_slice(&json).context("The share code is damaged or from a newer version")?;
    if pack.v != 1 {
        bail!("This code was made by a newer version of Hearthforge. Update the app and try again.");
    }
    if pack.addons.len() > 500 {
        bail!("That share code lists too many addons.");
    }
    Ok(pack)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_rejects_garbage() {
        let pack = Pack {
            v: 1,
            name: "Raid night".into(),
            flavor: Flavor::Forever,
            addons: vec![
                PackAddon { provider: ProviderId::Github, project_id: "Questie/Questie".into(), name: "Questie".into(), channel: None },
                PackAddon { provider: ProviderId::Tukui, project_id: "elvui".into(), name: "ElvUI".into(), channel: Some(Channel::Beta) },
            ],
        };
        let code = encode(&pack).unwrap();
        assert!(code.starts_with(PREFIX));
        let spaced = format!("  {}\n{} ", &code[..10], &code[10..]);
        assert_eq!(decode(&spaced).unwrap(), pack);
        assert!(decode("hello").is_err());
        assert!(decode("HF1:not-valid!!").is_err());
    }
}
