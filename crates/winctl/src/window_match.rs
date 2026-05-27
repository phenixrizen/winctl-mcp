use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ScoredWindow, WindowInfo, WindowSelector};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WindowBindErrorCode {
    NoMatch,
    WeakSelector,
    AmbiguousMatch,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Error)]
#[error("{message}")]
pub struct WindowBindError {
    pub code: WindowBindErrorCode,
    pub message: String,
    pub candidates: Vec<ScoredWindow>,
}

pub fn score_window(selector: &WindowSelector, w: &WindowInfo) -> Option<i32> {
    let mut score = 0;

    if let Some(id) = &selector.id {
        if &w.id == id {
            score += 90;
        } else {
            return None;
        }
    }
    if let Some(pid) = selector.pid {
        if w.pid == pid {
            score += 80;
        } else {
            return None;
        }
    }
    if let Some(hwnd) = &selector.hwnd {
        if w.hwnd_hex.eq_ignore_ascii_case(hwnd) {
            score += 100;
        } else {
            return None;
        }
    }
    if let Some(proc_name) = &selector.process_name {
        if w.process_name
            .as_deref()
            .unwrap_or_default()
            .eq_ignore_ascii_case(proc_name)
        {
            score += 60;
        } else {
            return None;
        }
    }
    if let Some(class_name) = &selector.class_name_contains {
        if w.class_name
            .to_lowercase()
            .contains(&class_name.to_lowercase())
        {
            score += 30;
        } else {
            return None;
        }
    }
    if let Some(title) = &selector.title_contains {
        if w.title.to_lowercase().contains(&title.to_lowercase()) {
            score += 15;
        } else {
            return None;
        }
    }
    if let Some(rx) = &selector.title_regex {
        if Regex::new(rx)
            .ok()
            .map(|r| r.is_match(&w.title))
            .unwrap_or(false)
        {
            score += 15;
        } else {
            return None;
        }
    }
    if let Some(exe_contains) = &selector.exe_path_contains {
        if w.exe_path
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(&exe_contains.to_lowercase())
        {
            score += 70;
        } else {
            return None;
        }
    }
    if let Some(exe_suffix) = &selector.exe_path_ends_with {
        if w.exe_path
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .ends_with(&exe_suffix.to_lowercase())
        {
            score += 70;
        } else {
            return None;
        }
    }

    if selector.must_be_visible.unwrap_or(false) && !w.visible {
        return None;
    }
    if !selector.allow_minimized.unwrap_or(false) && w.minimized {
        score -= 80;
    }
    if !selector.allow_cloaked.unwrap_or(false) && w.cloaked {
        score -= 100;
    }

    let terminal_penalty = [
        "windowsterminal.exe",
        "cmd.exe",
        "powershell.exe",
        "pwsh.exe",
    ];
    if let Some(proc_name) = &w.process_name {
        if terminal_penalty
            .iter()
            .any(|p| proc_name.eq_ignore_ascii_case(p))
        {
            score -= 50;
        }
    }

    Some(score)
}

pub fn find_windows(selector: &WindowSelector, windows: &[WindowInfo]) -> Vec<ScoredWindow> {
    let mut out: Vec<ScoredWindow> = windows
        .iter()
        .filter_map(|w| {
            score_window(selector, w).map(|score| ScoredWindow {
                score,
                window: w.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| b.score.cmp(&a.score));
    out
}

pub fn select_window_for_bind(
    selector: &WindowSelector,
    windows: &[WindowInfo],
) -> Result<ScoredWindow, WindowBindError> {
    let candidates = find_windows(selector, windows);

    if candidates.is_empty() {
        return Err(WindowBindError {
            code: WindowBindErrorCode::NoMatch,
            message: "no window matched selector".into(),
            candidates,
        });
    }

    if !selector.is_strong_selector() {
        let code = if candidates.len() > 1 {
            WindowBindErrorCode::AmbiguousMatch
        } else {
            WindowBindErrorCode::WeakSelector
        };
        return Err(WindowBindError {
            code,
            message: "selector is not strong enough to bind; use hwnd, pid, process_name, or executable path selector".into(),
            candidates,
        });
    }

    let top_score = candidates[0].score;
    let tied_top_count = candidates.iter().filter(|c| c.score == top_score).count();
    if tied_top_count > 1 {
        return Err(WindowBindError {
            code: WindowBindErrorCode::AmbiguousMatch,
            message: "selector matched multiple windows with the same top score".into(),
            candidates,
        });
    }

    Ok(candidates[0].clone())
}

#[cfg(test)]
mod bind_selection_tests {
    use super::*;

    fn mk(id: &str, process_name: &str, title: &str) -> WindowInfo {
        WindowInfo {
            id: id.to_string(),
            hwnd: 1,
            hwnd_hex: "0x1".into(),
            pid: 1,
            tid: 1,
            process_name: Some(process_name.into()),
            exe_path: Some(format!("C:/{}.exe", process_name)),
            title: title.into(),
            class_name: "Class".into(),
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            visible: true,
            enabled: true,
            foreground: false,
            minimized: false,
            cloaked: false,
            top_level: true,
        }
    }

    #[test]
    fn bind_selection_rejects_title_only_ambiguity() {
        let windows = vec![
            mk("win_0001", "Betty.exe", "Betty"),
            mk("win_0002", "WindowsTerminal.exe", "build logs: betty"),
        ];
        let selector = WindowSelector {
            title_contains: Some("betty".into()),
            ..Default::default()
        };

        let err = select_window_for_bind(&selector, &windows).unwrap_err();

        assert_eq!(err.code, WindowBindErrorCode::AmbiguousMatch);
        assert_eq!(err.candidates.len(), 2);
    }

    #[test]
    fn bind_selection_reports_no_match() {
        let windows = vec![mk("win_0001", "Betty.exe", "Betty")];
        let selector = WindowSelector {
            process_name: Some("notepad.exe".into()),
            ..Default::default()
        };

        let err = select_window_for_bind(&selector, &windows).unwrap_err();

        assert_eq!(err.code, WindowBindErrorCode::NoMatch);
        assert!(err.candidates.is_empty());
    }

    #[test]
    fn bind_selection_allows_unique_strong_match() {
        let windows = vec![
            mk("win_0001", "Betty.exe", "Betty"),
            mk("win_0002", "WindowsTerminal.exe", "build logs: betty"),
        ];
        let selector = WindowSelector {
            process_name: Some("Betty.exe".into()),
            title_contains: Some("Betty".into()),
            ..Default::default()
        };

        let selected = select_window_for_bind(&selector, &windows).unwrap();

        assert_eq!(selected.window.id, "win_0001");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(id: &str, process_name: &str, title: &str) -> WindowInfo {
        WindowInfo {
            id: id.to_string(),
            hwnd: 1,
            hwnd_hex: "0x1".into(),
            pid: 1,
            tid: 1,
            process_name: Some(process_name.into()),
            exe_path: Some(format!("C:/{}.exe", process_name)),
            title: title.into(),
            class_name: "Class".into(),
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            visible: true,
            enabled: true,
            foreground: false,
            minimized: false,
            cloaked: false,
            top_level: true,
        }
    }

    #[test]
    fn title_only_is_ambiguous_for_betty() {
        let windows = vec![
            mk("win_0001", "Betty.exe", "Betty"),
            mk("win_0002", "WindowsTerminal.exe", "build logs: betty"),
        ];
        let selector = WindowSelector {
            title_contains: Some("betty".into()),
            ..Default::default()
        };
        let results = find_windows(&selector, &windows);
        assert_eq!(results.len(), 2);
    }
}
