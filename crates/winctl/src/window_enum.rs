use crate::WindowInfo;

#[cfg(windows)]
pub fn list_windows() -> Vec<WindowInfo> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn list_windows() -> Vec<WindowInfo> {
    vec![]
}

pub fn window_by_id(id: &str) -> Option<WindowInfo> {
    list_windows().into_iter().find(|w| w.id == id)
}
