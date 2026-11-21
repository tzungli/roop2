use roo_dsp::{FilterDescriptor, FilterPreviewResult, UserFilterSpec, REGISTRY};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct FilterSlotsResponse {
    pub ok: bool,
    pub slots: Vec<UserFilterSpec>,
}

#[tauri::command]
pub async fn dsp_list_filter_plugins() -> Result<Vec<FilterDescriptor>, String> {
    Ok(REGISTRY.list())
}

#[tauri::command]
pub async fn filters_list_slots() -> Result<FilterSlotsResponse, String> {
    Ok(FilterSlotsResponse {
        ok: true,
        slots: REGISTRY.list_user_slots(),
    })
}

#[tauri::command]
pub async fn filters_save_slot(spec: UserFilterSpec) -> Result<FilterSlotsResponse, String> {
    REGISTRY.save_user_slot(spec);
    Ok(FilterSlotsResponse {
        ok: true,
        slots: REGISTRY.list_user_slots(),
    })
}

#[tauri::command]
pub async fn filters_delete_slot(slot_id: String) -> Result<FilterSlotsResponse, String> {
    REGISTRY.delete_user_slot(&slot_id);
    Ok(FilterSlotsResponse {
        ok: true,
        slots: REGISTRY.list_user_slots(),
    })
}

#[tauri::command]
pub async fn filters_preview(
    spec: UserFilterSpec,
    src_rate: f64,
    target_rate: f64,
) -> Result<Option<FilterPreviewResult>, String> {
    Ok(REGISTRY.preview_filter(&spec, src_rate, target_rate))
}

#[tauri::command]
pub async fn capture_step(_n: u32, _info: Option<String>) -> Result<(), String> {
    Ok(())
}
