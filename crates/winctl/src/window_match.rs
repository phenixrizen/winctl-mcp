use regex::Regex;

use crate::{ScoredWindow, WindowInfo, WindowSelector};

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
