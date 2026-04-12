use meeting_notes_daemon::services::config as svc;
use meeting_notes_daemon::services::ServiceError;
use serde_json::Value;
use tracing::info;

#[tauri::command]
pub async fn mn_get_config() -> Result<Value, ServiceError> {
    info!("mn_get_config invoked");
    Ok(svc::get_config())
}

/// Returns basic app identity and the resolved system locale — called once
/// by the frontend at startup to pick the i18n bundle. Bundled here because
/// it's static app info, same as the schema above.
#[tauri::command]
pub async fn mn_get_app_info() -> Result<Value, ServiceError> {
    let locale = sys_locale_fallback();
    info!("mn_get_app_info invoked, locale={}", locale);
    Ok(serde_json::json!({
        "name": "VoiceRecords",
        "name_zh": "主簿",
        "version": env!("CARGO_PKG_VERSION"),
        "locale": locale,
    }))
}

/// Minimal locale detector — reads `LANG`/`LC_ALL` with graceful fallback
/// to `en`. We avoid pulling in `sys-locale` as a dependency for now.
fn sys_locale_fallback() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                let short = v.split(&['.', '_'][..]).next().unwrap_or("en").to_string();
                if !short.is_empty() {
                    return short;
                }
            }
        }
    }
    "en".to_string()
}
