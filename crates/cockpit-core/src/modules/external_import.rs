use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use url::Url;

use crate::modules::logger;

pub const EXTERNAL_PROVIDER_IMPORT_EVENT: &str = "external:provider-import";
pub const MAX_EXTERNAL_IMPORT_BUNDLE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalProviderImportPayload {
    pub provider_id: String,
    pub page: String,
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_app_version: Option<String>,
    pub auto_import: bool,
    #[serde(default)]
    pub activate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_url: Option<String>,
}

static PENDING_EXTERNAL_PROVIDER_IMPORT: LazyLock<Mutex<Option<ExternalProviderImportPayload>>> =
    LazyLock::new(|| Mutex::new(None));

fn normalize_lookup_key(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn parse_boolean_like(value: Option<&String>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let normalized = value.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
}

fn resolve_provider_and_page(value: &str) -> Option<(&'static str, &'static str)> {
    let normalized = normalize_lookup_key(value);
    match normalized.as_str() {
        "antigravity" | "overview" | "accounts" => Some(("antigravity", "overview")),
        "codex" => Some(("codex", "codex")),
        "github_copilot" | "githubcopilot" | "ghcp" => Some(("github-copilot", "github-copilot")),
        "windsurf" => Some(("windsurf", "windsurf")),
        "kiro" => Some(("kiro", "kiro")),
        "cursor" => Some(("cursor", "cursor")),
        "gemini" => Some(("gemini", "gemini")),
        "codebuddy" => Some(("codebuddy", "codebuddy")),
        "codebuddy_cn" | "codebuddycn" => Some(("codebuddy_cn", "codebuddy-cn")),
        "qoder" => Some(("qoder", "qoder")),
        "trae" => Some(("trae", "trae")),
        "workbuddy" => Some(("workbuddy", "workbuddy")),
        "zed" => Some(("zed", "zed")),
        _ => None,
    }
}

fn is_supported_scheme(scheme: &str) -> bool {
    matches!(scheme, "cockpit-tools" | "cockpittools")
}

fn is_import_action(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if matches!(
        host.as_str(),
        "import" | "provider-import" | "account-import"
    ) {
        return true;
    }

    let path = url.path().trim_matches('/').to_ascii_lowercase();
    matches!(
        path.as_str(),
        "import" | "provider-import" | "account-import"
    )
}

fn parse_query_map(url: &Url) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (key, value) in url.query_pairs() {
        let normalized_key = normalize_lookup_key(key.as_ref());
        if normalized_key.is_empty() {
            continue;
        }
        map.entry(normalized_key)
            .or_insert_with(|| value.into_owned());
    }
    map
}

fn summarize_candidate(candidate: &str) -> String {
    let Ok(parsed) = Url::parse(candidate) else {
        let preview = candidate.chars().take(140).collect::<String>();
        return format!("raw='{preview}'");
    };

    let mut keys = Vec::new();
    let mut token_len: Option<usize> = None;
    for (key, value) in parsed.query_pairs() {
        let normalized_key = normalize_lookup_key(key.as_ref());
        if normalized_key.is_empty() {
            continue;
        }
        if matches!(
            normalized_key.as_str(),
            "token"
                | "import_token"
                | "importtoken"
                | "payload"
                | "import_payload"
                | "importpayload"
                | "import_url"
                | "importurl"
        ) && token_len.is_none()
        {
            token_len = Some(value.trim().len());
        }
        keys.push(normalized_key);
    }

    format!(
        "{}://{}{}?keys={:?},token_len={}",
        parsed.scheme(),
        parsed.host_str().unwrap_or_default(),
        parsed.path(),
        keys,
        token_len
            .map(|len| len.to_string())
            .unwrap_or_else(|| "-".to_string())
    )
}

pub fn parse_external_import_url_with_reason(
    raw_url: &str,
) -> Result<ExternalProviderImportPayload, String> {
    let parsed = Url::parse(raw_url).map_err(|err| format!("URL parse failed: {err}"))?;
    if !is_supported_scheme(parsed.scheme()) {
        return Err(format!("unsupported scheme: {}", parsed.scheme()));
    }
    if !is_import_action(&parsed) {
        return Err(format!(
            "unsupported action: host='{}', path='{}'",
            parsed.host_str().unwrap_or_default(),
            parsed.path()
        ));
    }

    let query = parse_query_map(&parsed);
    let provider_raw = query
        .get("provider")
        .or_else(|| query.get("provider_id"))
        .or_else(|| query.get("providerid"))
        .or_else(|| query.get("platform"))
        .or_else(|| query.get("platform_id"))
        .or_else(|| query.get("platformid"))
        .or_else(|| query.get("target"))
        .or_else(|| query.get("page"))
        .ok_or_else(|| "missing provider/platform/target/page parameter".to_string())?;
    let (provider_id, page) = resolve_provider_and_page(provider_raw)
        .ok_or_else(|| format!("unsupported provider: {provider_raw}"))?;

    let token = query
        .get("token")
        .or_else(|| query.get("import_token"))
        .or_else(|| query.get("importtoken"))
        .or_else(|| query.get("payload"))
        .or_else(|| query.get("import_payload"))
        .or_else(|| query.get("importpayload"))
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    let import_url = query
        .get("import_url")
        .or_else(|| query.get("importurl"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let api_base_url = query
        .get("api_base_url")
        .or_else(|| query.get("apibaseurl"))
        .or_else(|| query.get("base_url"))
        .or_else(|| query.get("baseurl"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if token.is_empty() && import_url.is_none() {
        return Err("missing token/payload/import_url parameter".to_string());
    }
    if !token.is_empty() && import_url.is_some() {
        return Err("token/payload and import_url cannot both be present".to_string());
    }
    let min_app_version = query
        .get("min_app_version")
        .or_else(|| query.get("minappversion"))
        .map(|value| {
            value
                .trim()
                .trim_start_matches(|ch| ch == 'v' || ch == 'V')
                .to_string()
        })
        .filter(|value| !value.is_empty());
    let auto_import = parse_boolean_like(
        query
            .get("auto_import")
            .or_else(|| query.get("autoimport"))
            .or_else(|| query.get("auto_submit"))
            .or_else(|| query.get("autosubmit")),
    );
    let activate = parse_boolean_like(
        query
            .get("activate")
            .or_else(|| query.get("auto_activate"))
            .or_else(|| query.get("autoactivate")),
    );

    Ok(ExternalProviderImportPayload {
        provider_id: provider_id.to_string(),
        page: page.to_string(),
        token: token.trim().to_string(),
        import_url,
        api_base_url,
        min_app_version,
        auto_import,
        activate,
        source: None,
        raw_url: None,
    })
}

#[cfg(test)]
fn parse_external_import_url(raw_url: &str) -> Option<ExternalProviderImportPayload> {
    parse_external_import_url_with_reason(raw_url).ok()
}

pub fn set_pending_external_import(payload: ExternalProviderImportPayload) {
    if let Ok(mut guard) = PENDING_EXTERNAL_PROVIDER_IMPORT.lock() {
        logger::log_info(&format!(
            "[ExternalImport] set pending import: provider={}, page={}, auto_import={}, token_len={}",
            payload.provider_id,
            payload.page,
            payload.auto_import,
            payload.token.len()
        ));
        *guard = Some(payload);
    }
}

pub fn take_pending_external_import() -> Option<ExternalProviderImportPayload> {
    let Ok(mut guard) = PENDING_EXTERNAL_PROVIDER_IMPORT.lock() else {
        return None;
    };
    let payload = guard.take();
    if let Some(item) = payload.as_ref() {
        logger::log_info(&format!(
            "[ExternalImport] take pending import: provider={}, page={}, auto_import={}, token_len={}",
            item.provider_id,
            item.page,
            item.auto_import,
            item.token.len()
        ));
    } else {
        logger::log_info("[ExternalImport] take pending import: empty");
    }
    payload
}

pub fn should_skip_argv0(source: &str) -> bool {
    matches!(source, "single-instance" | "startup")
}

pub fn parse_external_import_args(
    args: &[String],
    source: &str,
) -> Option<ExternalProviderImportPayload> {
    logger::log_info(&format!(
        "[ExternalImport] parse args: source={}, arg_count={}",
        source,
        args.len()
    ));
    let skip_argv0 = should_skip_argv0(source);
    let mut saw_deep_link = false;
    for (i, arg) in args.iter().enumerate() {
        if skip_argv0 && i == 0 {
            continue;
        }
        let candidate = arg.trim();
        if candidate.is_empty() {
            continue;
        }
        let candidate_is_deep_link =
            candidate.starts_with("cockpit-tools://") || candidate.starts_with("cockpittools://");
        if candidate_is_deep_link {
            saw_deep_link = true;
        }
        let candidate_summary = summarize_candidate(candidate);
        logger::log_info(&format!(
            "[ExternalImport] inspect arg: source={}, is_deep_link={}, candidate={}",
            source, candidate_is_deep_link, candidate_summary
        ));

        let mut payload = match parse_external_import_url_with_reason(candidate) {
            Ok(payload) => payload,
            Err(reason) => {
                if candidate_is_deep_link {
                    logger::log_warn(&format!(
                        "[ExternalImport] arg parse rejected: source={}, candidate={}, reason={}",
                        source, candidate_summary, reason
                    ));
                }
                continue;
            }
        };
        payload.source = Some(source.to_string());
        payload.raw_url = Some(candidate.to_string());
        return Some(payload);
    }
    if saw_deep_link {
        logger::log_warn(&format!(
            "[ExternalImport] no matching deep link found: source={source}"
        ));
    } else {
        logger::log_info(&format!(
            "[ExternalImport] no deep link argument found: source={source}"
        ));
    }
    None
}

pub fn handle_external_import_args(
    args: &[String],
    source: &str,
) -> Option<ExternalProviderImportPayload> {
    let payload = parse_external_import_args(args, source)?;
    set_pending_external_import(payload.clone());
    Some(payload)
}

pub async fn fetch_external_import_url(import_url: &str) -> Result<String, String> {
    let import_url = import_url.trim();
    if import_url.is_empty() {
        return Err("import URL is empty".to_string());
    }

    let parsed = Url::parse(import_url).map_err(|err| format!("import URL is invalid: {err}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("import URL only supports http/https".to_string());
    }

    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|err| format!("create HTTP client failed: {err}"))?
        .get(parsed)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|err| format!("fetch import bundle failed: {err}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "fetch import bundle failed: HTTP {}",
            status.as_u16()
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|err| format!("read import bundle failed: {err}"))?;
    if bytes.len() > MAX_EXTERNAL_IMPORT_BUNDLE_BYTES {
        return Err("import bundle is too large".to_string());
    }

    String::from_utf8(bytes.to_vec())
        .map_err(|_| "import bundle is not valid UTF-8 text".to_string())
}

#[cfg(test)]
mod tests {
    use super::{fetch_external_import_url, parse_external_import_url, should_skip_argv0};

    #[test]
    fn skip_argv0_for_single_instance() {
        assert!(should_skip_argv0("single-instance"));
    }

    #[test]
    fn skip_argv0_for_startup() {
        assert!(should_skip_argv0("startup"));
    }

    #[test]
    fn do_not_skip_argv0_for_deep_link_sources() {
        assert!(!should_skip_argv0("deep-link-open-url"));
        assert!(!should_skip_argv0("deep-link-current"));
        assert!(!should_skip_argv0("run-event-opened"));
    }

    #[test]
    fn parse_basic_import_link() {
        let raw = "cockpit-tools://import?provider=codex&token=abc123";
        let payload = parse_external_import_url(raw).expect("payload");
        assert_eq!(payload.provider_id, "codex");
        assert_eq!(payload.page, "codex");
        assert_eq!(payload.token, "abc123");
        assert_eq!(payload.import_url, None);
        assert_eq!(payload.api_base_url, None);
        assert_eq!(payload.min_app_version, None);
        assert!(!payload.auto_import);
    }

    #[test]
    fn parse_alias_and_boolean() {
        let raw =
            "cockpit-tools://provider-import?platform=codebuddy-cn&payload=%7B%7D&auto_import=true";
        let payload = parse_external_import_url(raw).expect("payload");
        assert_eq!(payload.provider_id, "codebuddy_cn");
        assert_eq!(payload.page, "codebuddy-cn");
        assert_eq!(payload.token, "{}");
        assert_eq!(payload.import_url, None);
        assert_eq!(payload.api_base_url, None);
        assert_eq!(payload.min_app_version, None);
        assert!(payload.auto_import);
    }

    #[test]
    fn parse_antigravity_overview_alias() {
        let raw = "cockpittools://account-import?page=overview&token=1%2F%2F0gTokenDemo";
        let payload = parse_external_import_url(raw).expect("payload");
        assert_eq!(payload.provider_id, "antigravity");
        assert_eq!(payload.page, "overview");
        assert_eq!(payload.token, "1//0gTokenDemo");
        assert_eq!(payload.import_url, None);
        assert_eq!(payload.api_base_url, None);
        assert_eq!(payload.min_app_version, None);
        assert!(!payload.auto_import);
    }

    #[test]
    fn parse_import_url_link() {
        let raw = "cockpit-tools://import?provider=codex&import_url=https%3A%2F%2Fexample.com%2Fuser%2Fapi%2FtoolsImport%2Ffetch%3Fid%3Dabc%26token%3Ddef&auto_import=true";
        let payload = parse_external_import_url(raw).expect("payload");
        assert_eq!(payload.provider_id, "codex");
        assert_eq!(payload.page, "codex");
        assert_eq!(payload.token, "");
        assert_eq!(
            payload.import_url,
            Some("https://example.com/user/api/toolsImport/fetch?id=abc&token=def".to_string())
        );
        assert_eq!(payload.api_base_url, None);
        assert_eq!(payload.min_app_version, None);
        assert!(payload.auto_import);
    }

    #[test]
    fn parse_import_link_with_api_base_url() {
        let raw = "cockpit-tools://provider-import?platform=codex&import_url=https%3A%2F%2Fchongflow.cn%2Fapi%2Fcockpit-tools%2Fimport%2Fabc&api_base_url=https%3A%2F%2Fchongflow.cn%2Fv1&auto_import=true";
        let payload = parse_external_import_url(raw).expect("payload");
        assert_eq!(payload.provider_id, "codex");
        assert_eq!(
            payload.api_base_url,
            Some("https://chongflow.cn/v1".to_string())
        );
    }

    #[test]
    fn parse_activate_flag() {
        let raw = "cockpit-tools://provider-import?platform=codex&import_url=https%3A%2F%2Fexample.com%2Fbundle&auto_import=true&activate=true";
        let payload = parse_external_import_url(raw).expect("payload");
        assert_eq!(payload.provider_id, "codex");
        assert!(payload.auto_import);
        assert!(payload.activate);
    }

    #[test]
    fn parse_min_app_version() {
        let raw = "cockpit-tools://import?provider=codex&token=abc123&min_app_version=v0.22.21";
        let payload = parse_external_import_url(raw).expect("payload");
        assert_eq!(payload.provider_id, "codex");
        assert_eq!(payload.min_app_version, Some("0.22.21".to_string()));
    }

    #[tokio::test]
    async fn fetch_import_url_rejects_empty_url() {
        let err = fetch_external_import_url("   ").await.expect_err("error");
        assert_eq!(err, "import URL is empty");
    }

    #[tokio::test]
    async fn fetch_import_url_rejects_invalid_url() {
        let err = fetch_external_import_url("not a url")
            .await
            .expect_err("error");
        assert!(err.starts_with("import URL is invalid:"));
    }

    #[tokio::test]
    async fn fetch_import_url_rejects_non_http_scheme() {
        let err = fetch_external_import_url("file:///tmp/import.json")
            .await
            .expect_err("error");
        assert_eq!(err, "import URL only supports http/https");
    }
}
