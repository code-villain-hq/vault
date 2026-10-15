//! The run log (§22): the log is `crate::runs`; these only read and change
//! it in memory, so they stay plain commands. Its writer thread does the
//! disk.

use tauri::{AppHandle, State};

use super::AppState;
use crate::runs::{changed, RunLog};

/// Every run and finished task kept, newest first (RUN-3, RUN-9). Filters
/// and sums are the view's.
#[tauri::command]
pub fn list_runs(state: State<AppState>) -> RunLog {
    state.runs.list()
}

/// Forget every run and finished task (RUN-6).
#[tauri::command]
pub fn clear_runs(app: AppHandle, state: State<AppState>) {
    if state.runs.clear() {
        changed(&app);
    }
}
