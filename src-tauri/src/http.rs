//! HTTP helpers: the shared client, JSON fetching, streaming downloads and the
//! GitHub API wrapper with ETag caching and friendly rate-limit errors.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, ETAG, IF_NONE_MATCH};
use reqwest::{Response, StatusCode};
use serde::de::DeserializeOwned;
use tokio::io::AsyncWriteExt;

use crate::error::HttpStatusError;
use crate::secrets::SecretKind;
use crate::state::AppState;

pub fn build_client(app_version: &str) -> Result<reqwest::Client> {
    // reqwest is built without a bundled TLS provider; make sure ring is installed first.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .user_agent(format!("Hearthforge/{app_version} (+https://github.com/Noolies/wow-mods)"))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .build()
        .context("Couldn't set up networking")
}

/// Turns a non-success response into an `HttpStatusError` with a readable message.
pub async fn ensure_success(response: Response, what: &str) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    log::warn!("{what} request failed: {} {}", status.as_u16(), response.url());
    let message = match status {
        StatusCode::NOT_FOUND => format!("{what} wasn't found."),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            format!("{what} refused the request ({}). Check the API key or token in Settings.", status.as_u16())
        }
        StatusCode::TOO_MANY_REQUESTS => format!("{what} is rate limiting requests. Try again in a few minutes."),
        s if s.is_server_error() => format!("{what} is having problems right now ({}). Try again later.", s.as_u16()),
        s => format!("{what} returned an unexpected response ({}).", s.as_u16()),
    };
    Err(HttpStatusError { status: status.as_u16(), message }.into())
}

pub async fn get_json<T: DeserializeOwned>(client: &reqwest::Client, url: &str, what: &str) -> Result<T> {
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Couldn't reach {what}. Check your internet connection."))?;
    let response = ensure_success(response, what).await?;
    response.json::<T>().await.with_context(|| format!("{what} sent data Hearthforge couldn't read"))
}

/// Streams `url` into `dest`, reporting (received, total) at most ~10 times a second.
pub async fn download(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    headers: HeaderMap,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<u64> {
    let response = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .context("Couldn't start the download. Check your internet connection.")?;
    let mut response = ensure_success(response, "The download server").await?;
    let total = response.content_length();
    let mut file = tokio::fs::File::create(dest).await.context("Couldn't create a temporary file")?;
    let mut received: u64 = 0;
    let mut last_report = std::time::Instant::now();
    while let Some(chunk) = response.chunk().await.context("The download was interrupted")? {
        file.write_all(&chunk).await.context("Couldn't write the download to disk")?;
        received += chunk.len() as u64;
        if last_report.elapsed() >= Duration::from_millis(100) {
            on_progress(received, total);
            last_report = std::time::Instant::now();
        }
    }
    file.flush().await?;
    on_progress(received, total);
    if received == 0 {
        anyhow::bail!("The download was empty.");
    }
    Ok(received)
}

/// GET against api.github.com with auth (if configured), ETag caching and clear
/// rate-limit messages. Returns the response body.
pub async fn github_api(state: &AppState, url: &str) -> Result<String> {
    let mut request = state
        .http
        .get(url)
        .header(ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(token) = state.secret(SecretKind::GithubToken) {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let cached = state.github_cache.lock().unwrap().get(url).cloned();
    if let Some((etag, _)) = &cached {
        if let Ok(value) = HeaderValue::from_str(etag) {
            request = request.header(IF_NONE_MATCH, value);
        }
    }
    let response = request
        .send()
        .await
        .context("Couldn't reach GitHub. Check your internet connection.")?;

    if response.status() == StatusCode::NOT_MODIFIED {
        if let Some((_, body)) = cached {
            return Ok(body);
        }
    }
    let remaining = response
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let reset = response
        .headers()
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok());
    let status = response.status();
    if (status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS) && remaining == Some(0) {
        let when = reset
            .and_then(|r| chrono::DateTime::from_timestamp(r, 0))
            .map(|t| t.with_timezone(&chrono::Local).format("%-I:%M %p").to_string())
            .unwrap_or_else(|| "within the hour".to_string());
        return Err(HttpStatusError {
            status: status.as_u16(),
            message: format!(
                "GitHub's hourly limit for anonymous requests is used up (it resets at {when}). Add a GitHub token in Settings to raise the limit."
            ),
        }
        .into());
    }
    let response = ensure_success(response, "GitHub").await?;
    let etag = response.headers().get(ETAG).and_then(|v| v.to_str().ok()).map(str::to_string);
    let body = response.text().await.context("GitHub's reply was cut short")?;
    if let Some(etag) = etag {
        state.github_cache.lock().unwrap().insert(url.to_string(), (etag, body.clone()));
    }
    Ok(body)
}
