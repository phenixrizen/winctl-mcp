mod tools;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use winctl::{
    list_windows, select_window_for_bind, BoundWindow, WindowBindError, WindowIdentity,
    WindowSelector,
};

#[derive(Default, Clone)]
pub struct AppState {
    pub bound: Arc<Mutex<HashMap<String, BoundWindow>>>,
}

impl AppState {
    pub fn bind_window(&self, selector: WindowSelector) -> Result<BoundWindow, WindowBindError> {
        let windows = list_windows();
        let selected_match = select_window_for_bind(&selector, &windows)?;
        let selected = selected_match.window;
        let bound = BoundWindow {
            bound_id: selected.id.clone(),
            identity: WindowIdentity::from_window(&selected),
            window: selected.clone(),
            selector,
            match_score: selected_match.score,
            title_at_bind: selected.title.clone(),
            bound_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or_default(),
        };
        self.bound
            .lock()
            .expect("bound mutex poisoned")
            .insert(bound.bound_id.clone(), bound.clone());
        Ok(bound)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let state = AppState::default();
    tracing::info!("winctl-mcp-server initialized; using local function tool wiring");

    let _ = tools::windows::windows_list();
    let _ = tools::windows::windows_find(WindowSelector::default());
    let _ = tools::windows::windows_monitors();
    let _ = tools::windows::windows_describe(&state, "".to_string());
    Ok(())
}
