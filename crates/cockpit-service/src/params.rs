use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

fn extract_object(params: &Value) -> Result<&serde_json::Map<String, Value>, String> {
    params
        .as_object()
        .ok_or_else(|| "params must be an object".to_string())
}

pub(crate) fn param_string(params: &Value, keys: &[&str]) -> Result<String, String> {
    let obj = extract_object(params)?;
    for key in keys {
        if let Some(value) = obj.get(*key).and_then(Value::as_str) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }
    Err(format!("missing string param: {}", keys.join("|")))
}

pub(crate) fn param_string_or_empty(params: &Value, keys: &[&str]) -> Result<String, String> {
    let obj = extract_object(params)?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(String::new());
            }
            if let Some(value) = value.as_str() {
                return Ok(value.to_string());
            }
        }
    }
    Err(format!("missing string param: {}", keys.join("|")))
}

pub(crate) fn param_optional_string(
    params: &Value,
    keys: &[&str],
) -> Result<Option<String>, String> {
    let obj = extract_object(params)?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(None);
            }
            if let Some(value) = value.as_str() {
                let trimmed = value.trim();
                return Ok((!trimmed.is_empty()).then_some(trimmed.to_string()));
            }
            return Err(format!("{key} must be a string or null"));
        }
    }
    Ok(None)
}

pub(crate) fn param_nullable_string_presence(
    params: &Value,
    keys: &[&str],
) -> Result<Option<Option<String>>, String> {
    let obj = extract_object(params)?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(Some(None));
            }
            if let Some(value) = value.as_str() {
                let trimmed = value.trim();
                return Ok(Some((!trimmed.is_empty()).then_some(trimmed.to_string())));
            }
            return Err(format!("{key} must be a string or null"));
        }
    }
    Ok(None)
}

pub(crate) fn param_bool(params: &Value, keys: &[&str]) -> Result<bool, String> {
    let obj = extract_object(params)?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if let Some(value) = value.as_bool() {
                return Ok(value);
            }
            if let Some(value) = value.as_str() {
                return match value.trim().to_ascii_lowercase().as_str() {
                    "true" | "1" | "yes" => Ok(true),
                    "false" | "0" | "no" => Ok(false),
                    _ => Err(format!("{key} must be a boolean")),
                };
            }
            return Err(format!("{key} must be a boolean"));
        }
    }
    Err(format!("missing boolean param: {}", keys.join("|")))
}

pub(crate) fn param_optional_bool(params: &Value, keys: &[&str]) -> Result<Option<bool>, String> {
    let obj = extract_object(params)?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(None);
            }
            return param_bool(params, &[*key]).map(Some);
        }
    }
    Ok(None)
}

pub(crate) fn param_u64(params: &Value, keys: &[&str]) -> Result<u64, String> {
    let obj = extract_object(params)?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if let Some(value) = value.as_u64() {
                return Ok(value);
            }
            if let Some(value) = value.as_str() {
                return value
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| format!("{key} must be an unsigned integer"));
            }
            return Err(format!("{key} must be an unsigned integer"));
        }
    }
    Err(format!("missing integer param: {}", keys.join("|")))
}

pub(crate) fn param_optional_u64(params: &Value, keys: &[&str]) -> Result<Option<u64>, String> {
    let obj = extract_object(params)?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(None);
            }
            return param_u64(params, &[*key]).map(Some);
        }
    }
    Ok(None)
}

pub(crate) fn param_u32(params: &Value, keys: &[&str]) -> Result<u32, String> {
    let value = param_u64(params, keys)?;
    u32::try_from(value).map_err(|_| format!("{} must fit in u32", keys.join("|")))
}

pub(crate) fn param_u16(params: &Value, keys: &[&str]) -> Result<u16, String> {
    let value = param_u64(params, keys)?;
    u16::try_from(value).map_err(|_| format!("{} must be a valid port", keys.join("|")))
}

pub(crate) fn param_optional_u16(params: &Value, keys: &[&str]) -> Result<Option<u16>, String> {
    match param_optional_u64(params, keys)? {
        Some(value) => u16::try_from(value)
            .map(Some)
            .map_err(|_| format!("{} must be a valid port", keys.join("|"))),
        None => Ok(None),
    }
}

pub(crate) fn param_optional_i64(params: &Value, keys: &[&str]) -> Result<Option<i64>, String> {
    let obj = extract_object(params)?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(None);
            }
            if let Some(value) = value.as_i64() {
                return Ok(Some(value));
            }
            if let Some(value) = value.as_str() {
                return value
                    .trim()
                    .parse::<i64>()
                    .map(Some)
                    .map_err(|_| format!("{key} must be an integer"));
            }
            return Err(format!("{key} must be an integer"));
        }
    }
    Ok(None)
}

pub(crate) fn param_i32(params: &Value, keys: &[&str]) -> Result<i32, String> {
    let value = param_optional_i64(params, keys)?
        .ok_or_else(|| format!("missing integer param: {}", keys.join("|")))?;
    i32::try_from(value).map_err(|_| format!("{} must fit in i32", keys.join("|")))
}

pub(crate) fn param_string_vec(params: &Value, keys: &[&str]) -> Result<Vec<String>, String> {
    let obj = extract_object(params)?;
    for key in keys {
        if let Some(values) = obj.get(*key).and_then(Value::as_array) {
            return values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToString::to_string)
                        .ok_or_else(|| format!("{key} must contain only non-empty strings"))
                })
                .collect();
        }
    }
    Err(format!("missing string array param: {}", keys.join("|")))
}

pub(crate) fn param_optional_string_vec(
    params: &Value,
    keys: &[&str],
) -> Result<Option<Vec<String>>, String> {
    let obj = extract_object(params)?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(None);
            }
            if let Some(values) = value.as_array() {
                let parsed = values
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(ToString::to_string)
                            .ok_or_else(|| format!("{key} must contain only non-empty strings"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(Some(parsed));
            }
            return Err(format!("{key} must be a string array or null"));
        }
    }
    Ok(None)
}

pub(crate) fn param_optional_tray_layout_groups(
    params: &Value,
) -> Result<Option<Vec<cockpit_core::modules::tray_layout::TrayLayoutGroup>>, String> {
    let obj = extract_object(params)?;
    for key in ["platformGroups", "platform_groups"] {
        if let Some(value) = obj.get(key) {
            if value.is_null() {
                return Ok(None);
            }
            return serde_json::from_value(value.clone())
                .map(Some)
                .map_err(|err| format!("{key} must be a tray layout group array: {err}"));
        }
    }
    Ok(None)
}

pub(crate) fn param_string_map(
    params: &Value,
    key: &str,
) -> Result<HashMap<String, String>, String> {
    let obj = extract_object(params)?;
    let Some(value) = obj.get(key) else {
        return Ok(HashMap::new());
    };
    serde_json::from_value(value.clone()).map_err(|err| format!("{key} must be a string map: {err}"))
}

pub(crate) fn param_optional_bool_map(
    params: &Value,
    keys: &[&str],
) -> Result<HashMap<String, bool>, String> {
    let obj = extract_object(params)?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(HashMap::new());
            }
            return serde_json::from_value(value.clone())
                .map_err(|err| format!("{key} must be a boolean map: {err}"));
        }
    }
    Ok(HashMap::new())
}

pub(crate) fn param_json<T: DeserializeOwned>(
    params: &Value,
    keys: &[&str],
) -> Result<T, String> {
    let obj = extract_object(params)?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            return serde_json::from_value(value.clone())
                .map_err(|err| format!("{} is invalid: {err}", keys.join("|")));
        }
    }
    Err(format!("missing param: {}", keys.join("|")))
}

pub(crate) fn param_optional_json<T: DeserializeOwned>(
    params: &Value,
    keys: &[&str],
) -> Result<Option<T>, String> {
    let obj = extract_object(params)?;
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if value.is_null() {
                return Ok(None);
            }
            return serde_json::from_value(value.clone())
                .map(Some)
                .map_err(|err| format!("{} is invalid: {err}", keys.join("|")));
        }
    }
    Ok(None)
}

pub(crate) fn param_optional_codex_api_provider_mode(
    params: &Value,
) -> Result<Option<cockpit_core::models::codex::CodexApiProviderMode>, String> {
    let obj = extract_object(params)?;
    for key in ["apiProviderMode", "api_provider_mode"] {
        if let Some(value) = obj.get(key) {
            if value.is_null() {
                return Ok(None);
            }
            return serde_json::from_value(value.clone())
                .map(Some)
                .map_err(|err| format!("{key} is invalid: {err}"));
        }
    }
    Ok(None)
}

pub(crate) fn param_codex_app_speed(
    params: &Value,
) -> Result<cockpit_core::models::codex::CodexAppSpeed, String> {
    let obj = extract_object(params)?;
    let value = obj
        .get("speed")
        .ok_or_else(|| "missing speed param".to_string())?;
    serde_json::from_value(value.clone()).map_err(|err| format!("speed is invalid: {err}"))
}

pub(crate) fn param_account_id(params: &Value) -> Result<String, String> {
    param_string(params, &["accountId", "account_id"])
}

pub(crate) fn param_account_ids(params: &Value) -> Result<Vec<String>, String> {
    param_string_vec(params, &["accountIds", "account_ids"])
}

pub(crate) fn param_json_content(params: &Value) -> Result<String, String> {
    param_string(params, &["jsonContent", "json_content"])
}

pub(crate) fn param_access_token(params: &Value) -> Result<String, String> {
    param_string(
        params,
        &[
            "accessToken",
            "access_token",
            "githubAccessToken",
            "github_access_token",
        ],
    )
}

pub(crate) fn param_login_id(params: &Value) -> Result<String, String> {
    param_string(params, &["loginId", "login_id"])
}

pub(crate) fn param_optional_login_id(params: &Value) -> Result<Option<String>, String> {
    param_optional_string(params, &["loginId", "login_id"])
}

pub(crate) fn param_callback_url(params: &Value) -> Result<String, String> {
    param_string(params, &["callbackUrl", "callback_url"])
}

pub(crate) fn param_data(params: &Value) -> Result<String, String> {
    param_string(params, &["data"])
}

pub(crate) fn param_tags(params: &Value) -> Result<Vec<String>, String> {
    param_string_vec(params, &["tags"])
}

// ---------------------------------------------------------------------------
// General helpers used alongside param functions
// ---------------------------------------------------------------------------

pub(crate) fn count_success<T>(results: Vec<(String, Result<T, String>)>) -> i32 {
    results.iter().filter(|(_, result)| result.is_ok()).count() as i32
}

pub(crate) fn maybe_one<T: Serialize>(
    result: Result<Option<T>, String>,
    missing: &str,
) -> Result<Value, String> {
    match result? {
        Some(value) => serde_json::to_value(vec![value])
            .map_err(|err| format!("serialize import result failed: {err}")),
        None => Err(missing.to_string()),
    }
}
