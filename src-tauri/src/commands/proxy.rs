//! Tauri commands for proxy settings.

use crate::proxy::{self, ProxyConfig, ProxyMode};
use tauri::{AppHandle, Runtime};

/// Build a validated proxy configuration from command arguments.
fn build_config(mode: &str, url: &str) -> Result<ProxyConfig, String> {
    let mode = ProxyMode::parse(mode).ok_or_else(|| {
        format!(
            "Unknown proxy mode '{}'. Expected system, custom, or none.",
            mode
        )
    })?;

    let url = match mode {
        ProxyMode::Custom => proxy::normalize_custom_proxy_url(url)?,
        _ => url.trim().to_string(),
    };

    Ok(ProxyConfig { mode, url })
}

/// Get the current proxy configuration.
#[tauri::command]
pub fn get_proxy_config<R: Runtime>(app: AppHandle<R>) -> Result<ProxyConfig, String> {
    Ok(proxy::read_proxy_config(&app))
}

/// Save the proxy configuration and mirror it into Scoop's own config.
#[tauri::command]
pub fn set_proxy_config<R: Runtime>(
    app: AppHandle<R>,
    mode: String,
    url: String,
) -> Result<(), String> {
    let config = build_config(&mode, &url)?;
    proxy::write_proxy_config(&app, &config)?;

    crate::commands::settings::sync_scoop_proxy(&config).map_err(|e| {
        format!(
            "Saved the proxy for rScoop, but could not update Scoop's config: {}",
            e
        )
    })
}

/// Detect the current system proxy (Windows registry on Windows).
#[tauri::command]
pub fn detect_system_proxy() -> Result<Option<String>, String> {
    Ok(proxy::detect_system_proxy())
}

/// Test connectivity through the given (or saved) proxy settings.
///
/// When `mode`/`url` are supplied, the test uses those values instead of the
/// saved configuration so the user can validate a proxy before saving it.
#[tauri::command]
pub async fn test_proxy_connection<R: Runtime>(
    app: AppHandle<R>,
    mode: Option<String>,
    url: Option<String>,
) -> Result<String, String> {
    let config = match mode {
        Some(mode) => build_config(&mode, &url.unwrap_or_default())?,
        None => {
            let config = proxy::read_proxy_config(&app);
            if config.mode == ProxyMode::Custom && config.url.is_empty() {
                return Err("Enter a proxy URL before testing.".to_string());
            }
            config
        }
    };

    let client = proxy::build_client_from(&config)?;
    let response = client
        .head("https://github.com")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if response.status().is_success() || response.status().is_redirection() {
        Ok(format!("OK ({})", response.status().as_u16()))
    } else {
        Err(format!(
            "Connected, but the server returned {}",
            response.status().as_u16()
        ))
    }
}
