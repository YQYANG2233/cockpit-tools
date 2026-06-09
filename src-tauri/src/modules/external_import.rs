use tauri::{AppHandle, Emitter, Runtime};

use crate::modules::{floating_card_window, logger};

pub use cockpit_core::modules::external_import::{
    ExternalProviderImportPayload, EXTERNAL_PROVIDER_IMPORT_EVENT,
};

pub fn take_pending_external_import() -> Option<ExternalProviderImportPayload> {
    cockpit_core::modules::external_import::take_pending_external_import()
}

fn emit_external_import_payload<R: Runtime>(
    app: &AppHandle<R>,
    payload: &ExternalProviderImportPayload,
) {
    if let Err(err) = app.emit(EXTERNAL_PROVIDER_IMPORT_EVENT, payload.clone()) {
        logger::log_warn(&format!(
            "[ExternalImport] send external import event failed: {}",
            err
        ));
        return;
    }
    logger::log_info(&format!(
        "[ExternalImport] sent external import event: provider={}, page={}, auto_import={}, token_len={}",
        payload.provider_id,
        payload.page,
        payload.auto_import,
        payload.token.len()
    ));
}

pub fn handle_external_import_args<R: Runtime>(
    app: &AppHandle<R>,
    args: &[String],
    source: &str,
) -> bool {
    let Some(payload) =
        cockpit_core::modules::external_import::handle_external_import_args(args, source)
    else {
        return false;
    };

    if let Err(err) = floating_card_window::show_main_window_and_navigate(app, &payload.page) {
        logger::log_warn(&format!(
            "[ExternalImport] show main window and navigate failed: {}",
            err
        ));
    }
    emit_external_import_payload(app, &payload);

    logger::log_info(&format!(
        "[ExternalImport] received external import request: provider={}, page={}, auto_import={}, source={}, token_len={}",
        payload.provider_id,
        payload.page,
        payload.auto_import,
        source,
        payload.token.len()
    ));
    true
}
