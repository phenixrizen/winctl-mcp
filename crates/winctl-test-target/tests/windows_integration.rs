#![cfg(windows)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use winctl::{
    capture_region_from_window, select_window_for_bind, window_from_point, BoundWindow,
    WindowBindErrorCode, WindowIdentity, WindowInfo, WindowSelector,
};

#[test]
fn fixture_acceptance_criteria() {
    if std::env::var_os("WINCTL_RUN_WINDOWS_INTEGRATION").is_none() {
        eprintln!("skipping Windows integration harness; set WINCTL_RUN_WINDOWS_INTEGRATION=1");
        return;
    }

    let token = unique_token();
    let class_name = format!("WinctlHarness{token}");
    let title = format!("Betty Target {token}");
    let child_title = format!("Betty Child {token}");

    let target = Fixture::launch(&title, &class_name, 80, 80, true, Some(&child_title));
    let decoy = Fixture::launch(&title, &class_name, 780, 80, false, None);

    let target_window = wait_for_window(&title, &class_name, target.pid());
    let decoy_window = wait_for_window(&title, &class_name, decoy.pid());
    let windows = wait_for_windows(&title, &class_name, 2);

    let title_only = WindowSelector {
        title_contains: Some(token.clone()),
        must_be_visible: Some(true),
        ..Default::default()
    };
    let title_error = select_window_for_bind(&title_only, &windows).unwrap_err();
    assert_eq!(title_error.code, WindowBindErrorCode::AmbiguousMatch);

    let by_hwnd = WindowSelector {
        hwnd: Some(target_window.hwnd_hex.clone()),
        ..Default::default()
    };
    let hwnd_match = select_window_for_bind(&by_hwnd, &windows).unwrap();
    assert_eq!(hwnd_match.window.hwnd, target_window.hwnd);

    let by_pid = WindowSelector {
        pid: Some(target_window.pid),
        title_contains: Some(token.clone()),
        class_name_contains: Some(class_name.clone()),
        must_be_visible: Some(true),
        ..Default::default()
    };
    let pid_match = select_window_for_bind(&by_pid, &windows).unwrap();
    assert_eq!(pid_match.window.pid, target_window.pid);

    let bound = bound_window(&target_window, by_hwnd.clone(), hwnd_match.score);
    let wrong_target = window_from_point(decoy_window.x + 20, decoy_window.y + 80, Some(&bound));
    assert_eq!(wrong_target.belongs_to_bound_window, Some(false));

    let child_hit = window_from_point(target_window.x + 60, target_window.y + 120, Some(&bound));
    assert_eq!(child_hit.belongs_to_bound_window, Some(true));
    assert!(child_hit.child.is_some());

    let region = capture_region_from_window(&target_window);
    assert_eq!(region.x, target_window.x);
    assert_eq!(region.y, target_window.y);
    assert_eq!(region.width, target_window.width);
    assert_eq!(region.height, target_window.height);
    assert!(region.width > 0);
    assert!(region.height > 0);

    drop(decoy);
    wait_for_window_exit(decoy_window.hwnd);

    let process_name = target_window
        .process_name
        .clone()
        .expect("fixture process name should be resolved");
    let process_windows = wait_for_windows(&title, &class_name, 1);
    let by_process = WindowSelector {
        process_name: Some(process_name),
        title_contains: Some(token),
        class_name_contains: Some(class_name),
        must_be_visible: Some(true),
        ..Default::default()
    };
    let process_match = select_window_for_bind(&by_process, &process_windows).unwrap();
    assert_eq!(process_match.window.pid, target_window.pid);

    drop(target);
}

struct Fixture {
    child: Child,
    ready_file: PathBuf,
}

impl Fixture {
    fn launch(
        title: &str,
        class_name: &str,
        x: i32,
        y: i32,
        create_child: bool,
        child_title: Option<&str>,
    ) -> Self {
        let ready_file = std::env::temp_dir().join(format!("{}.ready", unique_token()));
        let mut command = Command::new(env!("CARGO_BIN_EXE_winctl-test-target"));
        command.args([
            "--title",
            title,
            "--class",
            class_name,
            "--x",
            &x.to_string(),
            "--y",
            &y.to_string(),
            "--width",
            "560",
            "--height",
            "360",
            "--duration-ms",
            "30000",
            "--ready-file",
            ready_file.to_str().expect("ready path should be UTF-8"),
        ]);
        if create_child {
            command.arg("--create-child");
        }
        if let Some(child_title) = child_title {
            command.args(["--child-title", child_title]);
        }

        let child = command.spawn().expect("failed to launch test fixture");
        wait_for_ready_file(&ready_file);
        Self { child, ready_file }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.ready_file);
    }
}

fn bound_window(window: &WindowInfo, selector: WindowSelector, match_score: i32) -> BoundWindow {
    BoundWindow {
        bound_id: window.id.clone(),
        identity: WindowIdentity::from_window(window),
        window: window.clone(),
        selector,
        match_score,
        title_at_bind: window.title.clone(),
        bound_at_unix_ms: 0,
    }
}

fn wait_for_ready_file(path: &Path) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        if path.exists() {
            return;
        }
        sleep(Duration::from_millis(50));
    }
    panic!("fixture did not report ready at {}", path.display());
}

fn wait_for_window(title: &str, class_name: &str, pid: u32) -> WindowInfo {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        if let Some(window) = winctl::list_windows().into_iter().find(|window| {
            window.title == title && window.class_name == class_name && window.pid == pid
        }) {
            return window;
        }
        sleep(Duration::from_millis(100));
    }
    panic!("window not found for title={title:?} class={class_name:?} pid={pid}");
}

fn wait_for_windows(title: &str, class_name: &str, expected_count: usize) -> Vec<WindowInfo> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        let windows: Vec<_> = winctl::list_windows()
            .into_iter()
            .filter(|window| window.title == title && window.class_name == class_name)
            .collect();
        if windows.len() == expected_count {
            return windows;
        }
        sleep(Duration::from_millis(100));
    }
    panic!("expected {expected_count} windows for title={title:?} class={class_name:?}");
}

fn wait_for_window_exit(hwnd: isize) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        if winctl::window_info_from_hwnd(hwnd).is_none() {
            return;
        }
        sleep(Duration::from_millis(100));
    }
    panic!("window {hwnd:#x} did not exit");
}

fn unique_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_nanos();
    format!("winctl-it-{}-{nanos}", std::process::id())
}
