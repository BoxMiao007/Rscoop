//! Centralized proxy configuration.
//!
//! Reads the proxy mode and URL from the Tauri persistent store and provides
//! helpers that the rest of the app uses to create proxy-aware HTTP clients,
//! git2 fetch options, and environment variables for child processes.

use git2::{FetchOptions, ProxyOptions};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

const STORE_PATH: &str = "store.json";

/// The three proxy modes exposed to the user.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    /// Follow the Windows system proxy settings.
    #[default]
    System,
    /// Use a user-supplied proxy URL.
    Custom,
    /// Connect directly, ignoring any proxy.
    None,
}

impl ProxyMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "system" => Some(Self::System),
            "custom" => Some(Self::Custom),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

/// Resolved proxy configuration: a mode plus an optional URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfig {
    pub mode: ProxyMode,
    /// Only meaningful when `mode == Custom`.
    pub url: String,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            mode: ProxyMode::System,
            url: String::new(),
        }
    }
}

// ── Store helpers ───────────────────────────────────────────────────────────

/// Read the current proxy configuration from the persistent store.
pub fn read_proxy_config<R: Runtime>(app: &AppHandle<R>) -> ProxyConfig {
    let store = match app.store(PathBuf::from(STORE_PATH)) {
        Ok(s) => s,
        Err(_) => return ProxyConfig::default(),
    };

    let mode = store
        .get("proxy.mode")
        .and_then(|v| serde_json::from_value::<ProxyMode>(v.clone()).ok())
        .unwrap_or_default();

    let url = store
        .get("proxy.url")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();

    ProxyConfig { mode, url }
}

/// Persist the proxy configuration into the persistent store.
pub fn write_proxy_config<R: Runtime>(
    app: &AppHandle<R>,
    config: &ProxyConfig,
) -> Result<(), String> {
    let store = app
        .store(PathBuf::from(STORE_PATH))
        .map_err(|e| format!("Failed to open settings store: {}", e))?;

    let mode = serde_json::to_value(config.mode).map_err(|e| e.to_string())?;
    store.set("proxy.mode", mode);
    store.set("proxy.url", serde_json::json!(config.url));
    store
        .save()
        .map_err(|e| format!("Failed to save proxy settings: {}", e))?;
    Ok(())
}

// ── System proxy detection (Windows registry) ──────────────────────────────

/// Normalize the Windows `ProxyServer` registry value into a URL.
///
/// The value can be a bare `host:port`, a full URL, or a protocol map such as
/// `http=h:1;https=h:2;ftp=h:3`. When a protocol map is present the `https`
/// entry is preferred, then `http`, then `ftp`.
fn normalize_proxy_server(server: &str) -> Option<String> {
    let server = server.trim();
    if server.is_empty() {
        return None;
    }

    // Protocol map form: pick the best entry (https > http > ftp).
    let host_port = if server.contains('=') {
        let mut https = None;
        let mut http = None;
        let mut ftp = None;
        for entry in server.split(';') {
            let entry = entry.trim();
            let Some((scheme, value)) = entry.split_once('=') else {
                continue;
            };
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            match scheme.trim().to_ascii_lowercase().as_str() {
                "https" => https = https.or(Some(value)),
                "http" => http = http.or(Some(value)),
                "ftp" => ftp = ftp.or(Some(value)),
                _ => {}
            }
        }
        https.or(http).or(ftp)?
    } else {
        server
    };

    if host_port.starts_with("http://")
        || host_port.starts_with("https://")
        || host_port.starts_with("socks4://")
        || host_port.starts_with("socks4a://")
        || host_port.starts_with("socks5://")
        || host_port.starts_with("socks5h://")
    {
        Some(host_port.to_string())
    } else {
        Some(format!("http://{}", host_port))
    }
}

/// Try to read the system proxy from the Windows Internet Settings registry key.
///
/// Returns `Some(url)` when a proxy is enabled and configured.
#[cfg(windows)]
pub fn detect_system_proxy() -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let settings = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;

    let enabled: u32 = settings.get_value("ProxyEnable").ok()?;
    if enabled == 0 {
        return None;
    }

    let server: String = settings.get_value("ProxyServer").ok()?;
    normalize_proxy_server(&server)
}

/// Read the Windows `ProxyOverride` bypass list as a `NO_PROXY` value.
#[cfg(windows)]
pub fn detect_proxy_override() -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let settings = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;
    let override_list: String = settings.get_value("ProxyOverride").ok()?;
    let normalized = override_list
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty() && *entry != "<local>")
        .collect::<Vec<_>>()
        .join(",");
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

#[cfg(not(windows))]
pub fn detect_system_proxy() -> Option<String> {
    // On non-Windows platforms, defer to environment variables.
    std::env::var("HTTPS_PROXY")
        .or_else(|_| std::env::var("HTTP_PROXY"))
        .or_else(|_| std::env::var("https_proxy"))
        .or_else(|_| std::env::var("http_proxy"))
        .ok()
        .filter(|s| !s.is_empty())
}

#[cfg(not(windows))]
pub fn detect_proxy_override() -> Option<String> {
    std::env::var("NO_PROXY")
        .or_else(|_| std::env::var("no_proxy"))
        .ok()
        .filter(|s| !s.is_empty())
}

// ── Custom URL validation ──────────────────────────────────────────────────

/// Validate and normalize a user-supplied proxy URL.
///
/// A bare `host:port` is accepted and normalized to `http://host:port`.
/// Supported schemes are `http`, `https`, `socks4`, `socks4a`, `socks5`, and
/// `socks5h`; anything else is rejected with a user-facing message.
pub fn normalize_custom_proxy_url(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Proxy URL is empty.".to_string());
    }

    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{}", trimmed)
    };

    let url = reqwest::Url::parse(&candidate)
        .map_err(|e| format!("Invalid proxy URL '{}': {}", trimmed, e))?;

    match url.scheme() {
        "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h" => {}
        other => {
            return Err(format!(
                "Unsupported proxy scheme '{}'. Use an HTTP proxy such as http://127.0.0.1:7890.",
                other
            ))
        }
    }

    if url.host_str().is_none() {
        return Err("Proxy URL is missing a host.".to_string());
    }

    Ok(url.to_string().trim_end_matches('/').to_string())
}

/// Convert an HTTP(S) proxy URL into the value Scoop's `proxy` config expects.
///
/// Scoop prepends `http://` itself, so the value is `[user:pass@]host[:port]`.
/// Returns `None` for proxy schemes Scoop cannot use (for example SOCKS).
pub fn scoop_proxy_address(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return None,
    }

    let host = parsed.host_str()?;
    let mut address = String::new();
    if !parsed.username().is_empty() {
        let username = parsed.username().replace('\\', "\\\\");
        address.push_str(&username);
        if let Some(password) = parsed.password() {
            let password = password.replace('\\', "\\\\").replace('@', "\\@").replace(':', "\\:");
            address.push(':');
            address.push_str(&password);
        }
        address.push('@');
    }
    address.push_str(host);
    if let Some(port) = parsed.port() {
        address.push(':');
        address.push_str(&port.to_string());
    }
    Some(address)
}

// ── Resolve effective proxy URL ────────────────────────────────────────────

/// Return the proxy URL that should be used for the given configuration.
pub fn resolve_proxy_url(config: &ProxyConfig) -> Option<String> {
    match config.mode {
        ProxyMode::System => detect_system_proxy(),
        ProxyMode::Custom => {
            let url = config.url.trim();
            if url.is_empty() {
                None
            } else {
                Some(url.to_string())
            }
        }
        ProxyMode::None => None,
    }
}

// ── reqwest Client builder ─────────────────────────────────────────────────

/// Build a `reqwest::Client` from an explicit proxy configuration.
pub fn build_client_from(config: &ProxyConfig) -> Result<Client, String> {
    let mut builder = Client::builder();

    match config.mode {
        ProxyMode::System => {
            // reqwest's `system-proxy` default feature reads the OS settings.
        }
        ProxyMode::Custom => {
            let url = config.url.trim();
            if !url.is_empty() {
                let proxy = reqwest::Proxy::all(url)
                    .map_err(|e| format!("Invalid proxy URL '{}': {}", url, e))?;
                builder = builder.proxy(proxy);
            }
        }
        ProxyMode::None => {
            builder = builder.no_proxy();
        }
    }

    builder
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

/// Build a `reqwest::Client` that respects the currently saved proxy settings.
pub fn build_http_client<R: Runtime>(app: &AppHandle<R>) -> Result<Client, String> {
    build_client_from(&read_proxy_config(app))
}

// ── git2 FetchOptions helper ───────────────────────────────────────────────

/// Apply the given proxy configuration to a `git2::FetchOptions`.
pub fn apply_proxy_to_fetch_options<'cb>(config: &ProxyConfig, fetch_opts: &mut FetchOptions<'cb>) {
    match config.mode {
        ProxyMode::System => {
            // `auto()` lets libgit2 detect the system proxy itself.
            let mut proxy = ProxyOptions::new();
            proxy.auto();
            fetch_opts.proxy_options(proxy);
        }
        ProxyMode::Custom => {
            let url = config.url.trim();
            if !url.is_empty() {
                let mut proxy = ProxyOptions::new();
                proxy.url(url);
                fetch_opts.proxy_options(proxy);
            }
        }
        ProxyMode::None => {
            // Default `ProxyOptions` performs no proxy detection.
        }
    }
}

// ── Environment variables for child processes (Scoop commands) ─────────────

/// Build the proxy environment variables for an explicit configuration.
pub fn proxy_env_vars_from(config: &ProxyConfig) -> Vec<(String, String)> {
    let Some(url) = resolve_proxy_url(config) else {
        return Vec::new();
    };

    let mut vars = vec![
        ("HTTP_PROXY".to_string(), url.clone()),
        ("HTTPS_PROXY".to_string(), url.clone()),
        ("http_proxy".to_string(), url.clone()),
        ("https_proxy".to_string(), url),
    ];

    if config.mode == ProxyMode::System {
        if let Some(no_proxy) = detect_proxy_override() {
            vars.push(("NO_PROXY".to_string(), no_proxy.clone()));
            vars.push(("no_proxy".to_string(), no_proxy));
        }
    }

    vars
}

/// Return environment variable pairs to inject into child processes so that
/// tools like `scoop`, `git`, and `curl` respect the proxy settings.
pub fn proxy_env_vars<R: Runtime>(app: &AppHandle<R>) -> Vec<(String, String)> {
    proxy_env_vars_from(&read_proxy_config(app))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_bare_host_port() {
        assert_eq!(
            normalize_proxy_server("127.0.0.1:7890").as_deref(),
            Some("http://127.0.0.1:7890")
        );
    }

    #[test]
    fn keeps_scheme_when_present() {
        assert_eq!(
            normalize_proxy_server("socks5://127.0.0.1:1080").as_deref(),
            Some("socks5://127.0.0.1:1080")
        );
        assert_eq!(
            normalize_proxy_server("http://proxy.local:3128").as_deref(),
            Some("http://proxy.local:3128")
        );
    }

    #[test]
    fn prefers_https_entry_in_protocol_map() {
        assert_eq!(
            normalize_proxy_server("http=proxy:80;https=proxy:443;ftp=proxy:21").as_deref(),
            Some("http://proxy:443")
        );
    }

    #[test]
    fn falls_back_to_http_entry_in_protocol_map() {
        assert_eq!(
            normalize_proxy_server("ftp=proxy:21;http=proxy:80").as_deref(),
            Some("http://proxy:80")
        );
    }

    #[test]
    fn empty_server_yields_none() {
        assert!(normalize_proxy_server("   ").is_none());
    }

    #[test]
    fn custom_mode_env_vars_ignore_bypass_list() {
        let config = ProxyConfig {
            mode: ProxyMode::Custom,
            url: "http://127.0.0.1:7890".to_string(),
        };
        let vars = proxy_env_vars_from(&config);
        assert_eq!(vars.len(), 4);
        assert!(vars.iter().all(|(_, value)| value == "http://127.0.0.1:7890"));
    }

    #[test]
    fn none_mode_yields_no_env_vars() {
        let config = ProxyConfig {
            mode: ProxyMode::None,
            url: "http://127.0.0.1:7890".to_string(),
        };
        assert!(proxy_env_vars_from(&config).is_empty());
    }

    #[test]
    fn custom_mode_with_empty_url_yields_no_env_vars() {
        let config = ProxyConfig {
            mode: ProxyMode::Custom,
            url: "   ".to_string(),
        };
        assert!(proxy_env_vars_from(&config).is_empty());
    }

    #[test]
    fn normalizes_bare_custom_url() {
        assert_eq!(
            normalize_custom_proxy_url("127.0.0.1:7890").as_deref(),
            Ok("http://127.0.0.1:7890")
        );
    }

    #[test]
    fn keeps_custom_url_with_scheme() {
        assert_eq!(
            normalize_custom_proxy_url("http://proxy.local:8080/").as_deref(),
            Ok("http://proxy.local:8080")
        );
        assert_eq!(
            normalize_custom_proxy_url("socks5://127.0.0.1:1080").as_deref(),
            Ok("socks5://127.0.0.1:1080")
        );
    }

    #[test]
    fn rejects_unsupported_custom_scheme() {
        assert!(normalize_custom_proxy_url("ftp://proxy.local:21").is_err());
    }

    #[test]
    fn rejects_empty_custom_url() {
        assert!(normalize_custom_proxy_url("  ").is_err());
    }

    #[test]
    fn scoop_address_strips_scheme() {
        assert_eq!(
            scoop_proxy_address("http://127.0.0.1:7890").as_deref(),
            Some("127.0.0.1:7890")
        );
        assert_eq!(
            scoop_proxy_address("https://proxy.local:8443").as_deref(),
            Some("proxy.local:8443")
        );
    }

    #[test]
    fn scoop_address_includes_credentials() {
        assert_eq!(
            scoop_proxy_address("http://user:pw@127.0.0.1:7890").as_deref(),
            Some("user:pw@127.0.0.1:7890")
        );
    }

    #[test]
    fn scoop_address_rejects_socks() {
        assert!(scoop_proxy_address("socks5://127.0.0.1:1080").is_none());
    }
}
