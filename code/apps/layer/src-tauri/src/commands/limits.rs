//! Each agent plan's limits (§27): the store is `crate::limits`.

use tauri::{AppHandle, State};

use crate::limits::{Limits, PlanLimits};

/// Every provider's latest reading (LIMIT-4).
#[tauri::command]
pub fn plan_limits(limits: State<Limits>) -> Vec<PlanLimits> {
    limits.list()
}

/// Ask OpenAI again now, and answer with every provider's latest. Claude's
/// cannot be asked for: it comes when a Claude agent next replies (LIMIT-2).
#[tauri::command]
pub async fn refresh_plan_limits(app: AppHandle) -> Vec<PlanLimits> {
    crate::limits::refresh_openai(&app).await;
    use tauri::Manager;
    app.state::<Limits>().list()
}
