use serde_json::Value;

use crate::params::*;
use crate::rpc_types::block_on;
use crate::sse::publish_event;

pub(crate) fn external_import_fetch_import_url(params: &Value) -> Result<String, String> {
    let import_url = param_string(params, &["importUrl", "import_url"])?;
    block_on(cockpit_core::modules::external_import::fetch_external_import_url(&import_url))
}

pub(crate) fn external_import_submit_url(params: &Value) -> Result<Value, String> {
    let raw_url = param_string(params, &["rawUrl", "raw_url", "url"])?;
    let source =
        param_optional_string(params, &["source"])?.unwrap_or_else(|| "web-gateway".to_string());
    let mut payload =
        cockpit_core::modules::external_import::parse_external_import_url_with_reason(&raw_url)?;
    payload.source = Some(source);
    payload.raw_url = Some(raw_url);
    cockpit_core::modules::external_import::set_pending_external_import(payload.clone());
    let value =
        serde_json::to_value(&payload).map_err(|err| format!("serialize payload failed: {err}"))?;
    publish_event(
        cockpit_core::modules::external_import::EXTERNAL_PROVIDER_IMPORT_EVENT,
        value.clone(),
    );
    Ok(value)
}
