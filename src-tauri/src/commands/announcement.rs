use cockpit_core::modules::announcement;
use cockpit_core::modules::announcement::{AnnouncementState, SponsorModuleState, TopRightAdState};

fn application_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
pub async fn announcement_get_state() -> Result<AnnouncementState, String> {
    announcement::get_announcement_state_for_version(application_version()).await
}

#[tauri::command]
pub async fn announcement_mark_as_read(id: String) -> Result<(), String> {
    announcement::mark_announcement_as_read(&id).await
}

#[tauri::command]
pub async fn announcement_mark_all_as_read() -> Result<(), String> {
    announcement::mark_all_announcements_as_read_for_version(application_version()).await
}

#[tauri::command]
pub async fn announcement_force_refresh() -> Result<AnnouncementState, String> {
    announcement::force_refresh_announcements_for_version(application_version()).await
}

#[tauri::command]
pub async fn announcement_get_top_right_ad() -> Result<TopRightAdState, String> {
    announcement::get_top_right_ad_state_for_version(application_version()).await
}

#[tauri::command]
pub async fn announcement_get_sponsor_module() -> Result<SponsorModuleState, String> {
    announcement::get_sponsor_module_state_for_version(application_version()).await
}

#[tauri::command]
pub async fn announcement_force_refresh_sponsor_module() -> Result<SponsorModuleState, String> {
    announcement::force_refresh_sponsor_module_for_version(application_version()).await
}
