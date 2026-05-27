mod tools;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use winctl::{find_windows, list_windows, BoundWindow, WindowSelector};

#[derive(Default, Clone)]
pub struct AppState {
    pub bound: Arc<Mutex<HashMap<String, BoundWindow>>>,
}

impl AppState {
    pub fn bind_window(&self, selector: WindowSelector) -> anyhow::Result<BoundWindow> {
        let windows = list_windows();
        let matches = find_windows(&selector, &windows);
        if matches.is_empty() {
            anyhow::bail!("no window matched selector")
        }
        if matches.len() > 1 && !selector.is_strong_selector() {
            anyhow::bail!("ambiguous selector; use pid/hwnd/process_name/exe selector")
        }

        let selected = matches[0].window.clone();
        let bound = BoundWindow {
            bound_id: selected.id.clone(),
            window: selected.clone(),
            title_at_bind: selected.title.clone(),
            bound_at_unix_ms: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
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
