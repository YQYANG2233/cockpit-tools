use crate::models::codex_local_access::{CodexLocalAccessTestFailure, CodexLocalAccessTestResult};
use serde::Serialize;
use std::time::{Duration, Instant};

const CODEX_MODEL_PROVIDER_TEST_TIMEOUT_SECS: u64 = 20;

fn codex_model_provider_models_url(base_url: &str) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("PROVIDER_BASE_URL_INVALID".to_string());
    }
    let mut url =
        reqwest::Url::parse(trimmed).map_err(|_| "PROVIDER_BASE_URL_INVALID".to_string())?;
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err("PROVIDER_BASE_URL_INVALID".to_string()),
    }
    let next_path = if url.path().is_empty() || url.path() == "/" {
        "/models".to_string()
    } else {
        format!("{}/models", url.path().trim_end_matches('/'))
    };
    url.set_path(&next_path);
    url.set_query(None);
    Ok(url.to_string())
}

fn codex_model_provider_usage_url(base_url: &str) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("PROVIDER_BASE_URL_INVALID".to_string());
    }
    let mut url =
        reqwest::Url::parse(trimmed).map_err(|_| "PROVIDER_BASE_URL_INVALID".to_string())?;
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err("PROVIDER_BASE_URL_INVALID".to_string()),
    }
    let next_path = if url.path().is_empty() || url.path() == "/" {
        "/usage".to_string()
    } else {
        format!("{}/usage", url.path().trim_end_matches('/'))
    };
    url.set_path(&next_path);
    url.set_query(None);
    Ok(url.to_string())
}

fn codex_model_provider_new_api_billing_url(
    base_url: &str,
    endpoint: &str,
) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("PROVIDER_BASE_URL_INVALID".to_string());
    }
    let mut url =
        reqwest::Url::parse(trimmed).map_err(|_| "PROVIDER_BASE_URL_INVALID".to_string())?;
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err("PROVIDER_BASE_URL_INVALID".to_string()),
    }
    let base_path = url.path().trim_end_matches('/');
    let next_path = if base_path.is_empty() {
        format!("/{}", endpoint.trim_start_matches('/'))
    } else {
        format!("{}/{}", base_path, endpoint.trim_start_matches('/'))
    };
    url.set_path(&next_path);
    url.set_query(None);
    Ok(url.to_string())
}

fn codex_model_provider_new_api_api_url(base_url: &str, endpoint: &str) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("PROVIDER_BASE_URL_INVALID".to_string());
    }
    let mut url =
        reqwest::Url::parse(trimmed).map_err(|_| "PROVIDER_BASE_URL_INVALID".to_string())?;
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err("PROVIDER_BASE_URL_INVALID".to_string()),
    }
    let mut base_path = url.path().trim_end_matches('/').to_string();
    if base_path == "/v1" {
        base_path.clear();
    }
    let next_path = if base_path.is_empty() {
        format!("/{}", endpoint.trim_start_matches('/'))
    } else {
        format!("{}/{}", base_path, endpoint.trim_start_matches('/'))
    };
    url.set_path(&next_path);
    url.set_query(None);
    Ok(url.to_string())
}

fn codex_model_provider_failure(
    title: &str,
    stage: &str,
    cause: String,
    suggestion: &str,
    status: Option<u16>,
    detail: Option<String>,
) -> CodexLocalAccessTestResult {
    CodexLocalAccessTestResult {
        model_id: None,
        latency_ms: None,
        output: None,
        failure: Some(CodexLocalAccessTestFailure {
            title: title.to_string(),
            stage: stage.to_string(),
            cause,
            suggestion: suggestion.to_string(),
            status,
            model_id: None,
            detail,
            gateway_output: None,
        }),
    }
}

fn summarize_model_provider_models(body: &serde_json::Value) -> (Option<String>, Option<String>) {
    let ids: Vec<String> = body
        .get("data")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(|id| id.as_str()))
                .take(8)
                .map(|id| id.to_string())
                .collect()
        })
        .unwrap_or_default();
    let first = ids.first().cloned();
    let output = if ids.is_empty() {
        None
    } else {
        Some(ids.join(", "))
    };
    (first, output)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelProviderUsageDetail {
    pub key: String,
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelProviderUsageSummary {
    pub mode: Option<String>,
    pub is_valid: Option<bool>,
    pub status: Option<String>,
    pub plan_name: Option<String>,
    pub remaining: Option<f64>,
    pub balance: Option<f64>,
    pub unit: Option<String>,
    pub quota_unlimited: Option<bool>,
    pub quota_limit: Option<f64>,
    pub quota_used: Option<f64>,
    pub quota_remaining: Option<f64>,
    pub today_requests: Option<i64>,
    pub today_total_tokens: Option<i64>,
    pub today_cost: Option<f64>,
    pub total_requests: Option<i64>,
    pub total_total_tokens: Option<i64>,
    pub total_cost: Option<f64>,
    pub model_stats_count: usize,
    pub latency_ms: u64,
    pub details: Vec<CodexModelProviderUsageDetail>,
}

fn json_f64_at(value: &serde_json::Value, path: &[&str]) -> Option<f64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_f64()
}

fn json_i64_at(value: &serde_json::Value, path: &[&str]) -> Option<i64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_i64()
}

fn json_string_at(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str().map(|item| item.to_string())
}

fn json_bool_at(value: &serde_json::Value, path: &[&str]) -> Option<bool> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_bool()
}

fn format_usage_number(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{:.0}", value)
    } else {
        format!("{:.4}", value)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn push_usage_detail(
    details: &mut Vec<CodexModelProviderUsageDetail>,
    key: &str,
    label: &str,
    value: Option<String>,
) {
    let Some(value) = value else {
        return;
    };
    if value.trim().is_empty() {
        return;
    }
    details.push(CodexModelProviderUsageDetail {
        key: key.to_string(),
        label: label.to_string(),
        value,
    });
}

fn summarize_model_provider_usage(
    body: &serde_json::Value,
    latency_ms: u64,
) -> CodexModelProviderUsageSummary {
    let model_stats_count = body
        .get("model_stats")
        .and_then(|value| value.as_array())
        .map(|items| items.len())
        .unwrap_or(0);
    let mut details = Vec::new();
    push_usage_detail(
        &mut details,
        "mode",
        "Mode",
        json_string_at(body, &["mode"]),
    );
    push_usage_detail(
        &mut details,
        "status",
        "Status",
        json_string_at(body, &["status"]),
    );
    push_usage_detail(
        &mut details,
        "planName",
        "Plan",
        json_string_at(body, &["planName"]),
    );
    push_usage_detail(
        &mut details,
        "remaining",
        "Remaining",
        json_f64_at(body, &["remaining"]).map(format_usage_number),
    );
    push_usage_detail(
        &mut details,
        "balance",
        "Balance",
        json_f64_at(body, &["balance"]).map(format_usage_number),
    );
    push_usage_detail(
        &mut details,
        "todayRequests",
        "Today Requests",
        json_i64_at(body, &["usage", "today", "requests"]).map(|value| value.to_string()),
    );
    push_usage_detail(
        &mut details,
        "todayTokens",
        "Today Tokens",
        json_i64_at(body, &["usage", "today", "total_tokens"]).map(|value| value.to_string()),
    );
    push_usage_detail(
        &mut details,
        "todayCost",
        "Today Cost",
        json_f64_at(body, &["usage", "today", "cost"]).map(format_usage_number),
    );
    push_usage_detail(
        &mut details,
        "totalRequests",
        "Total Requests",
        json_i64_at(body, &["usage", "total", "requests"]).map(|value| value.to_string()),
    );
    push_usage_detail(
        &mut details,
        "totalTokens",
        "Total Tokens",
        json_i64_at(body, &["usage", "total", "total_tokens"]).map(|value| value.to_string()),
    );
    push_usage_detail(
        &mut details,
        "totalCost",
        "Total Cost",
        json_f64_at(body, &["usage", "total", "cost"]).map(format_usage_number),
    );

    CodexModelProviderUsageSummary {
        mode: json_string_at(body, &["mode"]),
        is_valid: json_bool_at(body, &["is_active"]).or_else(|| json_bool_at(body, &["isValid"])),
        status: json_string_at(body, &["status"]),
        plan_name: json_string_at(body, &["planName"]),
        remaining: json_f64_at(body, &["remaining"]),
        balance: json_f64_at(body, &["balance"]),
        unit: json_string_at(body, &["unit"]).or_else(|| json_string_at(body, &["quota", "unit"])),
        quota_unlimited: json_bool_at(body, &["quota", "unlimited"]),
        quota_limit: json_f64_at(body, &["quota", "limit"]),
        quota_used: json_f64_at(body, &["quota", "used"]),
        quota_remaining: json_f64_at(body, &["quota", "remaining"]),
        today_requests: json_i64_at(body, &["usage", "today", "requests"]),
        today_total_tokens: json_i64_at(body, &["usage", "today", "total_tokens"]),
        today_cost: json_f64_at(body, &["usage", "today", "cost"]),
        total_requests: json_i64_at(body, &["usage", "total", "requests"]),
        total_total_tokens: json_i64_at(body, &["usage", "total", "total_tokens"]),
        total_cost: json_f64_at(body, &["usage", "total", "cost"]),
        model_stats_count,
        latency_ms,
        details,
    }
}

fn summarize_new_api_model_provider_usage(
    subscription: &serde_json::Value,
    usage: &serde_json::Value,
    token_usage: Option<&serde_json::Value>,
    latency_ms: u64,
) -> CodexModelProviderUsageSummary {
    let raw_quota_limit = json_f64_at(subscription, &["hard_limit_usd"])
        .or_else(|| json_f64_at(subscription, &["soft_limit_usd"]))
        .or_else(|| json_f64_at(subscription, &["system_hard_limit_usd"]));
    let quota_used = json_f64_at(usage, &["total_usage"]).map(|value| value / 100.0);
    let token_data = token_usage.and_then(|value| value.get("data"));
    let quota_unlimited = token_data
        .and_then(|value| json_bool_at(value, &["unlimited_quota"]))
        .unwrap_or_else(|| {
            let hard = json_f64_at(subscription, &["hard_limit_usd"]);
            let soft = json_f64_at(subscription, &["soft_limit_usd"]);
            let system = json_f64_at(subscription, &["system_hard_limit_usd"]);
            matches!(
                (hard, soft, system),
                (Some(h), Some(s), Some(sys))
                    if (h - 100_000_000.0).abs() < f64::EPSILON
                        && (s - 100_000_000.0).abs() < f64::EPSILON
                        && (sys - 100_000_000.0).abs() < f64::EPSILON
            )
        });
    let quota_limit = if quota_unlimited {
        None
    } else {
        raw_quota_limit
    };
    let quota_remaining = match (quota_limit, quota_used) {
        (Some(limit), Some(used)) => Some((limit - used).max(0.0)),
        _ => None,
    };
    let mut details = Vec::new();
    push_usage_detail(
        &mut details,
        "hardLimitUsd",
        "Hard Limit USD",
        json_f64_at(subscription, &["hard_limit_usd"]).map(format_usage_number),
    );
    push_usage_detail(
        &mut details,
        "softLimitUsd",
        "Soft Limit USD",
        json_f64_at(subscription, &["soft_limit_usd"]).map(format_usage_number),
    );
    push_usage_detail(
        &mut details,
        "systemHardLimitUsd",
        "System Hard Limit USD",
        json_f64_at(subscription, &["system_hard_limit_usd"]).map(format_usage_number),
    );
    push_usage_detail(
        &mut details,
        "accessUntil",
        "Access Until",
        json_i64_at(subscription, &["access_until"]).map(|value| value.to_string()),
    );
    push_usage_detail(
        &mut details,
        "quotaUnlimited",
        "Unlimited Quota",
        Some(quota_unlimited.to_string()),
    );
    if let Some(token_data) = token_data {
        push_usage_detail(
            &mut details,
            "totalGranted",
            "Total Granted",
            json_f64_at(token_data, &["total_granted"]).map(format_usage_number),
        );
        push_usage_detail(
            &mut details,
            "totalAvailable",
            "Total Available",
            json_f64_at(token_data, &["total_available"]).map(format_usage_number),
        );
        push_usage_detail(
            &mut details,
            "expiresAt",
            "Expires At",
            json_i64_at(token_data, &["expires_at"]).map(|value| value.to_string()),
        );
        push_usage_detail(
            &mut details,
            "modelLimitsEnabled",
            "Model Limits",
            json_bool_at(token_data, &["model_limits_enabled"]).map(|value| value.to_string()),
        );
    }
    push_usage_detail(
        &mut details,
        "totalUsage",
        "Total Usage",
        json_f64_at(usage, &["total_usage"]).map(format_usage_number),
    );

    CodexModelProviderUsageSummary {
        mode: Some("new_api".to_string()),
        is_valid: None,
        status: None,
        plan_name: None,
        remaining: quota_remaining,
        balance: None,
        unit: Some("USD".to_string()),
        quota_unlimited: Some(quota_unlimited),
        quota_limit,
        quota_used,
        quota_remaining,
        today_requests: None,
        today_total_tokens: None,
        today_cost: None,
        total_requests: None,
        total_total_tokens: None,
        total_cost: quota_used,
        model_stats_count: 0,
        latency_ms,
        details,
    }
}

pub async fn test_connection(
    base_url: String,
    api_key: String,
    wire_api: Option<String>,
) -> Result<CodexLocalAccessTestResult, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Ok(codex_model_provider_failure(
            "missing_api_key",
            "credential",
            "MISSING_API_KEY".to_string(),
            "add_api_key",
            None,
            None,
        ));
    }

    let url = match codex_model_provider_models_url(&base_url) {
        Ok(url) => url,
        Err(error) => {
            return Ok(codex_model_provider_failure(
                "invalid_base_url",
                "url",
                error,
                "check_base_url",
                None,
                None,
            ));
        }
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(CODEX_MODEL_PROVIDER_TEST_TIMEOUT_SECS))
        .build()
        .map_err(|err| format!("CREATE_HTTP_CLIENT_FAILED: {err}"))?;
    let started = Instant::now();
    let response = match client
        .get(&url)
        .bearer_auth(key)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return Ok(codex_model_provider_failure(
                "network_failed",
                "network",
                error.to_string(),
                "check_network",
                None,
                Some(format!("GET {}", url)),
            ));
        }
    };
    let latency_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    let status = response.status();
    let text = response.text().await.unwrap_or_default();

    if !status.is_success() {
        let suggestion = if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            "check_api_key"
        } else if status == reqwest::StatusCode::NOT_FOUND {
            "check_base_url"
        } else {
            "check_provider_status"
        };
        return Ok(codex_model_provider_failure(
            "provider_http_failed",
            "models",
            "HTTP_STATUS".to_string(),
            suggestion,
            Some(status.as_u16()),
            Some(text.chars().take(1000).collect()),
        ));
    }

    let parsed = match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(value) => value,
        Err(error) => {
            return Ok(codex_model_provider_failure(
                "response_parse_failed",
                "parse",
                error.to_string(),
                "check_openai_compatible_models",
                Some(status.as_u16()),
                Some(text.chars().take(1000).collect()),
            ));
        }
    };
    let (model_id, output) = summarize_model_provider_models(&parsed);
    let protocol = wire_api.unwrap_or_else(|| "auto".to_string());
    Ok(CodexLocalAccessTestResult {
        model_id,
        latency_ms: Some(latency_ms),
        output: output.or_else(|| Some(format!("{} connection ok", protocol))),
        failure: None,
    })
}

pub async fn query_usage(
    base_url: String,
    api_key: String,
    integration_type: Option<String>,
) -> Result<CodexModelProviderUsageSummary, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err("MISSING_API_KEY".to_string());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(CODEX_MODEL_PROVIDER_TEST_TIMEOUT_SECS))
        .build()
        .map_err(|err| format!("CREATE_HTTP_CLIENT_FAILED: {err}"))?;

    let requested_type = integration_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match requested_type {
        Some("new_api") => query_new_api_model_provider_usage(&client, &base_url, key).await,
        Some("sub2api") => query_sub2api_model_provider_usage(&client, &base_url, key).await,
        Some(value) => Err(format!("PROVIDER_USAGE_TYPE_UNSUPPORTED: {}", value)),
        None => {
            let new_api_error =
                match query_new_api_model_provider_usage(&client, &base_url, key).await {
                    Ok(summary) => return Ok(summary),
                    Err(error) => error,
                };
            match query_sub2api_model_provider_usage(&client, &base_url, key).await {
                Ok(summary) => Ok(summary),
                Err(sub2api_error) => Err(format!(
                    "PROVIDER_USAGE_DETECT_FAILED: new_api: {}; sub2api: {}",
                    new_api_error, sub2api_error
                )),
            }
        }
    }
}

async fn query_new_api_model_provider_usage(
    client: &reqwest::Client,
    base_url: &str,
    key: &str,
) -> Result<CodexModelProviderUsageSummary, String> {
    let subscription_url =
        codex_model_provider_new_api_billing_url(base_url, "dashboard/billing/subscription")?;
    let usage_url = codex_model_provider_new_api_billing_url(base_url, "dashboard/billing/usage")?;
    let token_usage_url = codex_model_provider_new_api_api_url(base_url, "api/usage/token/")?;
    let started = Instant::now();
    let subscription_response = client
        .get(&subscription_url)
        .bearer_auth(key)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|err| format!("PROVIDER_USAGE_NETWORK_FAILED: {err}"))?;
    let subscription_status = subscription_response.status();
    let subscription_text = subscription_response.text().await.unwrap_or_default();
    if !subscription_status.is_success() {
        return Err(format!(
            "PROVIDER_USAGE_HTTP_{}: {}",
            subscription_status.as_u16(),
            subscription_text.chars().take(300).collect::<String>()
        ));
    }
    let usage_response = client
        .get(&usage_url)
        .bearer_auth(key)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|err| format!("PROVIDER_USAGE_NETWORK_FAILED: {err}"))?;
    let latency_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    let usage_status = usage_response.status();
    let usage_text = usage_response.text().await.unwrap_or_default();
    if !usage_status.is_success() {
        return Err(format!(
            "PROVIDER_USAGE_HTTP_{}: {}",
            usage_status.as_u16(),
            usage_text.chars().take(300).collect::<String>()
        ));
    }
    let subscription = serde_json::from_str::<serde_json::Value>(&subscription_text)
        .map_err(|err| format!("PROVIDER_USAGE_PARSE_FAILED: {err}"))?;
    let usage = serde_json::from_str::<serde_json::Value>(&usage_text)
        .map_err(|err| format!("PROVIDER_USAGE_PARSE_FAILED: {err}"))?;
    let token_usage = match client
        .get(&token_usage_url)
        .bearer_auth(key)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            let text = response.text().await.unwrap_or_default();
            serde_json::from_str::<serde_json::Value>(&text).ok()
        }
        _ => None,
    };
    Ok(summarize_new_api_model_provider_usage(
        &subscription,
        &usage,
        token_usage.as_ref(),
        latency_ms,
    ))
}

async fn query_sub2api_model_provider_usage(
    client: &reqwest::Client,
    base_url: &str,
    key: &str,
) -> Result<CodexModelProviderUsageSummary, String> {
    let url = codex_model_provider_usage_url(base_url)?;
    let started = Instant::now();
    let response = client
        .get(&url)
        .bearer_auth(key)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|err| format!("PROVIDER_USAGE_NETWORK_FAILED: {err}"))?;
    let latency_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "PROVIDER_USAGE_HTTP_{}: {}",
            status.as_u16(),
            text.chars().take(300).collect::<String>()
        ));
    }
    let parsed = serde_json::from_str::<serde_json::Value>(&text)
        .map_err(|err| format!("PROVIDER_USAGE_PARSE_FAILED: {err}"))?;
    Ok(summarize_model_provider_usage(&parsed, latency_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_models_endpoint() {
        assert_eq!(
            codex_model_provider_models_url("https://example.test/v1").unwrap(),
            "https://example.test/v1/models"
        );
    }

    #[test]
    fn new_api_api_url_drops_v1_prefix() {
        assert_eq!(
            codex_model_provider_new_api_api_url("https://example.test/v1", "api/usage/token/")
                .unwrap(),
            "https://example.test/api/usage/token/"
        );
    }
}
