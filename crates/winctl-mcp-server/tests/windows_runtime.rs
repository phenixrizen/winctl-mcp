#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VK_CONTROL, VK_ESCAPE, VK_MENU,
};

static WINDOWS_RUNTIME_TEST_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn windows_mcp_exercises_native_uia_metrics_and_emergency_stop() {
    let _guard = runtime_test_lock().await;
    if std::env::var_os("WINCTL_SKIP_WINDOWS_RUNTIME_INTEGRATION").is_some() {
        eprintln!("skipping Windows runtime integration because WINCTL_SKIP_WINDOWS_RUNTIME_INTEGRATION is set");
        return;
    }

    let server_exe = server_exe_path();
    let target_exe = target_exe_path();
    assert!(
        server_exe.exists(),
        "winctl-mcp-server exe does not exist at {}",
        server_exe.display()
    );
    assert!(
        target_exe.exists(),
        "winctl-test-target exe does not exist at {}; run cargo build --workspace --target x86_64-pc-windows-msvc first or set WINCTL_TEST_TARGET_EXE",
        target_exe.display()
    );

    let mut harness = McpHarness::start(&server_exe).await;
    harness.initialize().await;

    let launch = harness
        .call_tool(
            "process.launch",
            serde_json::json!({
                "exe": target_exe.to_string_lossy(),
                "args": [
                    "--title", "winctl runtime target",
                    "--class", "WinctlRuntimeTarget",
                    "--width", "860",
                    "--height", "560",
                    "--duration-ms", "60000",
                    "--automation-controls"
                ],
                "wait_for_window": true,
                "timeout_ms": 10000
            }),
        )
        .await;
    assert_ok("process.launch", &launch);
    let pid = launch["pid"].as_u64().expect("launch should return pid") as u32;
    let launch_id = launch["launch_id"]
        .as_str()
        .expect("launch should return launch_id")
        .to_owned();
    let hwnd = launch["candidate_top_level_windows"]
        .as_array()
        .and_then(|windows| windows.first())
        .and_then(|window| window["hwnd_hex"].as_str())
        .expect("launch should return a candidate HWND")
        .to_owned();

    let bind = harness
        .call_tool(
            "windows.bind",
            serde_json::json!({
                "pid": pid,
                "hwnd": hwnd,
                "must_be_visible": true
            }),
        )
        .await;
    assert_ok("windows.bind", &bind);
    let bound_id = bind["bound"]["bound_id"]
        .as_str()
        .expect("bind should return bound_id")
        .to_owned();

    let arm = harness
        .call_tool(
            "control.arm",
            serde_json::json!({
                "session_id": "windows-runtime-test",
                "bound_id": bound_id,
                "allow_for_ms": 60000,
                "reason": "Windows runtime integration test"
            }),
        )
        .await;
    assert_ok("control.arm", &arm);

    let invoke = harness
        .call_tool(
            "uia.invoke",
            action_args(&bound_id, selector("Invoke Action", "Button")),
        )
        .await;
    assert_direct_pattern("uia.invoke", &invoke, "InvokePattern.Invoke");
    assert_eq!(invoke["control_preflight"]["notification_sent"], true);
    assert_eq!(
        invoke["control_preflight"]["native_toast"]["attempted"], true,
        "first sensitive action should attempt a native toast: {invoke:#}"
    );
    assert_eq!(
        invoke["control_preflight"]["active_overlay"]["attempted"], true,
        "control action should attempt the active overlay: {invoke:#}"
    );
    assert_eq!(
        invoke["control_preflight"]["active_overlay"]["visible"], true,
        "active overlay provider should acknowledge a visible overlay window: {invoke:#}"
    );
    let control_after_invoke = harness
        .call_tool("control.state", serde_json::json!({}))
        .await;
    assert_ok("control.state after invoke", &control_after_invoke);
    assert_control_event(&control_after_invoke, "native_toast");
    assert_control_event(&control_after_invoke, "overlay_started");
    assert_control_event(&control_after_invoke, "overlay_cleared");
    let status = harness
        .call_tool(
            "uia.find",
            serde_json::json!({
                "bound_id": bound_id,
                "selector": {"name": "invoke=done", "role": "Text"},
                "max_depth": 12,
                "max_elements": 4000
            }),
        )
        .await;
    assert_ok("uia.find status", &status);
    assert!(
        status["match_count"].as_u64().unwrap_or_default() >= 1,
        "InvokePattern should update the target app status text: {status:#}"
    );

    let set_value = harness
        .call_tool(
            "uia.set_value",
            serde_json::json!({
                "bound_id": bound_id,
                "selector": {"role": "Edit"},
                "value": "set by Windows UIA integration",
                "max_depth": 12,
                "max_elements": 4000
            }),
        )
        .await;
    assert_direct_pattern("uia.set_value", &set_value, "ValuePattern.SetValue");
    let get_value = harness
        .call_tool(
            "uia.get_value",
            action_args(&bound_id, serde_json::json!({"role": "Edit"})),
        )
        .await;
    assert_direct_pattern("uia.get_value", &get_value, "ValuePattern.CurrentValue");
    assert_eq!(
        get_value["outcome"]["value"]["value"],
        "set by Windows UIA integration"
    );

    let focus = harness
        .call_tool(
            "uia.set_focus",
            action_args(&bound_id, serde_json::json!({"role": "Edit"})),
        )
        .await;
    assert_direct_pattern("uia.set_focus", &focus, "IUIAutomationElement.SetFocus");
    assert_eq!(focus["outcome"]["after"]["focused"], true);

    let secret_value = format!("winctl-secret-{pid}");
    let secret_set = harness
        .call_tool(
            "secret.set",
            serde_json::json!({
                "name": "windows-runtime/login",
                "value": secret_value.clone(),
                "description": "Windows runtime macro.type_secret fixture",
                "tags": ["runtime"]
            }),
        )
        .await;
    assert_ok("secret.set", &secret_set);
    let secret_set_json = secret_set.to_string();
    assert!(
        !secret_set_json.contains(&secret_value),
        "secret.set response must not include plaintext: {secret_set:#}"
    );
    let clear_edit = harness
        .call_tool(
            "uia.set_value",
            serde_json::json!({
                "bound_id": bound_id,
                "selector": {"role": "Edit"},
                "value": "",
                "max_depth": 12,
                "max_elements": 4000
            }),
        )
        .await;
    assert_direct_pattern("uia.set_value clear", &clear_edit, "ValuePattern.SetValue");
    let focus_secret = harness
        .call_tool(
            "uia.set_focus",
            action_args(&bound_id, serde_json::json!({"role": "Edit"})),
        )
        .await;
    assert_direct_pattern(
        "uia.set_focus before secret",
        &focus_secret,
        "IUIAutomationElement.SetFocus",
    );
    let type_secret = harness
        .call_tool(
            "macro.type_secret",
            serde_json::json!({
                "bound_id": bound_id,
                "secret_ref": "windows-runtime/login"
            }),
        )
        .await;
    assert_ok("macro.type_secret", &type_secret);
    assert_eq!(type_secret["typed_secret"], true);
    assert!(
        type_secret.get("typed").is_none(),
        "macro.type_secret must not return typed counts: {type_secret:#}"
    );
    let type_secret_json = type_secret.to_string();
    assert!(
        !type_secret_json.contains(&secret_value),
        "macro.type_secret response must not include plaintext: {type_secret:#}"
    );
    assert!(
        !type_secret_json.contains("windows-runtime/login"),
        "macro.type_secret response must not include the secret_ref: {type_secret:#}"
    );
    let typed_secret_value = harness
        .call_tool(
            "uia.get_value",
            action_args(&bound_id, serde_json::json!({"role": "Edit"})),
        )
        .await;
    assert_direct_pattern(
        "uia.get_value after secret",
        &typed_secret_value,
        "ValuePattern.CurrentValue",
    );
    assert_eq!(
        typed_secret_value["outcome"]["value"]["value"],
        secret_value
    );

    let toggle = harness
        .call_tool(
            "uia.toggle",
            action_args(&bound_id, selector("Toggle Choice", "CheckBox")),
        )
        .await;
    assert_direct_pattern("uia.toggle", &toggle, "TogglePattern.Toggle");
    assert_eq!(toggle["outcome"]["value"]["before_state"], "off");
    if toggle["outcome"]["value"]["after_state"] != "on" {
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let toggle_back = harness
        .call_tool(
            "uia.toggle",
            action_args(&bound_id, selector("Toggle Choice", "CheckBox")),
        )
        .await;
    assert_direct_pattern("uia.toggle second", &toggle_back, "TogglePattern.Toggle");
    assert_eq!(toggle_back["outcome"]["value"]["before_state"], "on");
    let toggle_off_noop = harness
        .call_tool(
            "uia.toggle",
            desired_toggle_args(&bound_id, "Toggle Choice", "CheckBox", "off"),
        )
        .await;
    assert_direct_pattern(
        "uia.toggle desired off",
        &toggle_off_noop,
        "TogglePattern.Toggle",
    );
    assert_eq!(toggle_off_noop["outcome"]["value"]["before_state"], "off");
    assert_eq!(toggle_off_noop["outcome"]["value"]["after_state"], "off");
    assert_eq!(toggle_off_noop["outcome"]["value"]["toggle_count"], 0);
    let toggle_on = harness
        .call_tool(
            "uia.toggle",
            desired_toggle_args(&bound_id, "Toggle Choice", "CheckBox", "on"),
        )
        .await;
    assert_direct_pattern("uia.toggle desired on", &toggle_on, "TogglePattern.Toggle");
    assert_eq!(toggle_on["outcome"]["value"]["desired_state"], "on");
    assert_eq!(toggle_on["outcome"]["value"]["after_state"], "on");
    assert_eq!(toggle_on["outcome"]["value"]["toggle_count"], 1);

    let select = harness
        .call_tool(
            "uia.select",
            action_args(&bound_id, selector("Scroll Item 02", "ListItem")),
        )
        .await;
    assert_direct_pattern("uia.select", &select, "SelectionItemPattern.Select");
    assert_eq!(select["outcome"]["value"]["selected"], true);
    let select_add = harness
        .call_tool(
            "uia.select",
            select_mode_args(&bound_id, "Scroll Item 03", "ListItem", "add"),
        )
        .await;
    assert_direct_pattern(
        "uia.select add",
        &select_add,
        "SelectionItemPattern.AddToSelection",
    );
    assert_eq!(select_add["outcome"]["value"]["mode"], "add");
    assert_eq!(select_add["outcome"]["value"]["selected"], true);
    let select_remove = harness
        .call_tool(
            "uia.select",
            select_mode_args(&bound_id, "Scroll Item 03", "ListItem", "remove"),
        )
        .await;
    assert_direct_pattern(
        "uia.select remove",
        &select_remove,
        "SelectionItemPattern.RemoveFromSelection",
    );
    assert_eq!(select_remove["outcome"]["value"]["mode"], "remove");
    assert_eq!(select_remove["outcome"]["value"]["selected"], false);

    let expand = harness
        .call_tool("uia.expand_collapse", expand_args(&bound_id, "expand"))
        .await;
    assert_direct_pattern_prefix(
        "uia.expand_collapse expand",
        &expand,
        "ExpandCollapsePattern",
    );
    assert_eq!(expand["outcome"]["value"]["actual_action"], "expand");
    assert_eq!(expand["outcome"]["value"]["after_state"], "expanded");
    send_key(VK_ESCAPE);

    let range = harness
        .call_tool(
            "uia.range_value",
            serde_json::json!({
                "bound_id": bound_id,
                "selector": {"role": "Slider"},
                "value": 70.0,
                "max_depth": 12,
                "max_elements": 4000
            }),
        )
        .await;
    assert_direct_pattern("uia.range_value", &range, "RangeValuePattern.SetValue");
    assert!(
        range["outcome"]["value"]["after_value"]
            .as_f64()
            .unwrap_or_default()
            >= 69.0,
        "RangeValuePattern should set the slider value: {range:#}"
    );

    let scroll = harness
        .call_tool(
            "uia.scroll_into_view",
            serde_json::json!({
                "bound_id": bound_id,
                "selector": {
                    "name": "Scroll Item 30",
                    "role": "ListItem",
                    "include_offscreen": true
                },
                "max_depth": 12,
                "max_elements": 4000
            }),
        )
        .await;
    assert_direct_pattern(
        "uia.scroll_into_view",
        &scroll,
        "ScrollItemPattern.ScrollIntoView",
    );
    assert_eq!(scroll["outcome"]["after"]["offscreen"], false);

    let ocr = harness
        .call_tool(
            "capture.ocr_region",
            serde_json::json!({
                "bound_id": bound_id,
                "x": 300,
                "y": 170,
                "width": 520,
                "height": 160
            }),
        )
        .await;
    assert_ok("capture.ocr_region", &ocr);
    assert_eq!(ocr["provider"], "windows_media_ocr");
    let ocr_text = ocr["text"]
        .as_str()
        .expect("capture.ocr_region should return text")
        .to_ascii_uppercase();
    assert!(
        ocr_text.contains("WINCTL") && ocr_text.contains("4829"),
        "Windows.Media.Ocr should read known fixture text: {ocr:#}"
    );

    let metrics = harness
        .call_tool("process.metrics", serde_json::json!({"pid": pid}))
        .await;
    assert_ok("process.metrics", &metrics);
    assert_eq!(metrics["provider_enabled"], true);
    assert!(
        metrics["metrics"]["working_set_bytes"]
            .as_u64()
            .unwrap_or_default()
            > 0,
        "process.metrics should report a nonzero working set: {metrics:#}"
    );
    assert!(
        metrics["metrics"]["handle_count"]
            .as_u64()
            .unwrap_or_default()
            > 0,
        "process.metrics should report a nonzero handle count: {metrics:#}"
    );

    let crash_pid = launch_crashing_target(&target_exe).await;
    let crash_report = harness.wait_for_crash_report_event(crash_pid).await;
    assert_eq!(crash_report["event_log"]["source"], "wevtapi");
    let crash_entries = crash_report["event_log"]["entries"]
        .as_array()
        .expect("crash_report should return structured event log entries");
    assert!(
        !crash_entries.is_empty(),
        "crash_report should find an Application error for deliberately crashed test process: {crash_report:#}"
    );
    assert!(
        crash_entries.iter().any(|entry| entry["raw_xml"]
            .as_str()
            .map(|xml| xml
                .to_ascii_lowercase()
                .contains("winctl-test-target"))
            .unwrap_or(false)),
        "structured crash_report entry should include the crashed process identity: {crash_report:#}"
    );

    send_ctrl_alt_esc();
    let emergency = harness.wait_for_emergency_stop().await;
    assert_eq!(emergency["control"]["emergency_stop_active"], true);
    assert_eq!(emergency["control"]["status"], "revoked");

    let rearm = harness
        .call_tool(
            "control.arm",
            serde_json::json!({
                "session_id": "windows-runtime-test-cleanup",
                "bound_id": bound_id,
                "allow_for_ms": 10000,
                "reason": "cleanup after emergency-stop verification"
            }),
        )
        .await;
    assert_ok("control.arm cleanup", &rearm);
    let kill = harness
        .call_tool(
            "process.kill",
            serde_json::json!({
                "launch_id": launch_id,
                "force": true
            }),
        )
        .await;
    assert_ok("process.kill cleanup", &kill);
    assert_eq!(kill["exited"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn windows_control_audit_log_persists_across_restart() {
    let _guard = runtime_test_lock().await;
    if std::env::var_os("WINCTL_SKIP_WINDOWS_RUNTIME_INTEGRATION").is_some() {
        eprintln!("skipping Windows runtime integration because WINCTL_SKIP_WINDOWS_RUNTIME_INTEGRATION is set");
        return;
    }

    let server_exe = server_exe_path();
    assert!(
        server_exe.exists(),
        "winctl-mcp-server exe does not exist at {}",
        server_exe.display()
    );
    let capture_dir = temp_path("winctl-audit-captures");
    std::fs::create_dir_all(&capture_dir).expect("audit capture dir should be created");
    let marker = format!(
        "audit persistence marker {}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    );

    {
        let mut harness = McpHarness::start_with_capture_dir(&server_exe, Some(&capture_dir)).await;
        harness.initialize().await;
        let arm = harness
            .call_tool(
                "control.arm",
                serde_json::json!({
                    "session_id": "windows-audit-persistence-test",
                    "allow_for_ms": 10000,
                    "reason": marker
                }),
            )
            .await;
        assert_ok("control.arm audit", &arm);

        let state = harness
            .call_tool("control.state", serde_json::json!({}))
            .await;
        assert_ok("control.state audit", &state);
        assert_audit_entry(&state, "armed", &marker);
        let audit_path = state["control"]["audit_log"]["path"]
            .as_str()
            .expect("control.state should return audit log path");
        assert!(
            PathBuf::from(audit_path).exists(),
            "control audit log should exist at {audit_path}"
        );
    }

    {
        let mut restarted =
            McpHarness::start_with_capture_dir(&server_exe, Some(&capture_dir)).await;
        restarted.initialize().await;
        let state = restarted
            .call_tool("control.state", serde_json::json!({}))
            .await;
        assert_ok("control.state restarted audit", &state);
        assert_audit_entry(&state, "armed", &marker);

        let notifications = restarted
            .call_tool("notifications.list", serde_json::json!({"max_items": 3}))
            .await;
        assert_ok("notifications.list", &notifications);
        assert_eq!(notifications["provider_enabled"], true);
        assert_eq!(
            notifications["provider"],
            "windows_user_notification_listener"
        );
        assert!(
            notifications["access"]["status"].is_string(),
            "notifications.list should report provider access state: {notifications:#}"
        );
        assert!(
            notifications["notifications"].is_array(),
            "notifications.list should return a notification array even when access is denied: {notifications:#}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn windows_dialog_tools_invoke_message_box_button() {
    let _guard = runtime_test_lock().await;
    if std::env::var_os("WINCTL_SKIP_WINDOWS_RUNTIME_INTEGRATION").is_some() {
        eprintln!("skipping Windows runtime integration because WINCTL_SKIP_WINDOWS_RUNTIME_INTEGRATION is set");
        return;
    }

    let server_exe = server_exe_path();
    let target_exe = target_exe_path();
    assert!(
        server_exe.exists(),
        "server exe missing at {}",
        server_exe.display()
    );
    assert!(
        target_exe.exists(),
        "target exe missing at {}",
        target_exe.display()
    );

    let mut harness = McpHarness::start(&server_exe).await;
    harness.initialize().await;
    let launch = harness
        .call_tool(
            "process.launch",
            serde_json::json!({
                "exe": target_exe.to_string_lossy(),
                "args": [
                    "--title", "winctl dialog owner",
                    "--class", "WinctlDialogOwner",
                    "--width", "520",
                    "--height", "320",
                    "--duration-ms", "60000",
                    "--message-box-after-ms", "500"
                ],
                "wait_for_window": true,
                "timeout_ms": 10000
            }),
        )
        .await;
    assert_ok("process.launch dialog target", &launch);
    let launch_id = launch["launch_id"]
        .as_str()
        .expect("launch should return launch_id")
        .to_owned();

    let dialog = harness.wait_for_dialog_button("OK").await;
    let hwnd = dialog["window"]["hwnd_hex"]
        .as_str()
        .expect("dialog should include hwnd_hex")
        .to_owned();
    let pid = dialog["window"]["pid"]
        .as_u64()
        .expect("dialog should include pid") as u32;

    let arm = harness
        .call_tool(
            "control.arm",
            serde_json::json!({
                "session_id": "windows-dialog-test",
                "allow_for_ms": 30000,
                "reason": "Windows dialog integration test"
            }),
        )
        .await;
    assert_ok("control.arm dialog", &arm);

    let invoke = harness
        .call_tool(
            "dialogs.invoke_button",
            serde_json::json!({
                "hwnd": hwnd,
                "pid": pid,
                "button_name": "OK",
                "max_depth": 8,
                "max_elements": 1000
            }),
        )
        .await;
    assert_ok("dialogs.invoke_button", &invoke);
    assert_eq!(invoke["outcome"]["direct_uia_pattern_used"], true);
    assert_eq!(invoke["outcome"]["pattern_used"], "InvokePattern.Invoke");

    let kill = harness
        .call_tool(
            "process.kill",
            serde_json::json!({
                "launch_id": launch_id,
                "force": true
            }),
        )
        .await;
    assert_ok("process.kill dialog cleanup", &kill);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn windows_video_capture_records_display_artifact() {
    let _guard = runtime_test_lock().await;
    if std::env::var_os("WINCTL_SKIP_WINDOWS_RUNTIME_INTEGRATION").is_some() {
        eprintln!("skipping Windows runtime integration because WINCTL_SKIP_WINDOWS_RUNTIME_INTEGRATION is set");
        return;
    }

    let server_exe = server_exe_path();
    assert!(
        server_exe.exists(),
        "server exe missing at {}",
        server_exe.display()
    );
    let mut harness = McpHarness::start(&server_exe).await;
    harness.initialize().await;
    let start = harness
        .call_tool(
            "capture.video_start",
            serde_json::json!({
                "display_index": 0,
                "frame_interval_ms": 200,
                "max_duration_ms": 5000,
                "max_frame_width": 640,
                "max_frame_height": 360,
                "output_name": "windows-runtime-video"
            }),
        )
        .await;
    assert_ok("capture.video_start", &start);
    let recording_id = start["recording"]["recording_id"]
        .as_str()
        .expect("video_start should return recording_id")
        .to_owned();
    tokio::time::sleep(Duration::from_millis(3000)).await;
    let stop = harness
        .call_tool(
            "capture.video_stop",
            serde_json::json!({
                "recording_id": recording_id
            }),
        )
        .await;
    assert_ok("capture.video_stop", &stop);
    let output_path = stop["recording"]["output_path"]
        .as_str()
        .expect("video_stop should return output_path");
    assert!(
        PathBuf::from(output_path).exists(),
        "video artifact should exist at {output_path}: {stop:#}"
    );
    assert!(
        stop["recording"]["frame_count"]
            .as_u64()
            .unwrap_or_default()
            >= 2,
        "video recording should capture multiple frames: {stop:#}"
    );
    assert_eq!(stop["recording"]["format"], "gif");
    assert!(
        stop["recording"]["encoded_width"]
            .as_u64()
            .unwrap_or(u64::MAX)
            <= 640,
        "video GIF width should respect max_frame_width: {stop:#}"
    );
    assert!(
        stop["recording"]["encoded_height"]
            .as_u64()
            .unwrap_or(u64::MAX)
            <= 360,
        "video GIF height should respect max_frame_height: {stop:#}"
    );
}

struct McpHarness {
    child: Child,
    client: reqwest::Client,
    base: String,
    session_id: Option<String>,
    next_id: u64,
}

impl McpHarness {
    async fn start(server_exe: &Path) -> Self {
        Self::start_with_capture_dir(server_exe, None).await
    }

    async fn start_with_capture_dir(server_exe: &Path, capture_dir: Option<&Path>) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("port should bind");
        let addr = listener.local_addr().expect("local addr should resolve");
        drop(listener);
        let log_path = temp_path("winctl-windows-runtime.log");
        let mut command = Command::new(server_exe);
        command
            .arg("serve")
            .arg("--transport")
            .arg("http")
            .arg("--listen")
            .arg(addr.to_string())
            .arg("--log-file")
            .arg(log_path)
            .env("RUST_LOG", "info")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(capture_dir) = capture_dir {
            command.arg("--capture-dir").arg(capture_dir);
        }
        let child = command.spawn().expect("server should start");
        let harness = Self {
            child,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .expect("HTTP client should build"),
            base: format!("http://{addr}"),
            session_id: None,
            next_id: 1,
        };
        harness.wait_for_health().await;
        harness
    }

    async fn initialize(&mut self) {
        let id = self.next_id();
        let init = self
            .post_mcp(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": {"name": "windows-runtime-test", "version": "0.1"}
                }
            }))
            .await;
        assert_eq!(init["jsonrpc"], "2.0");
        let session_id = init["__session_id"]
            .as_str()
            .expect("initialize should return Mcp-Session-Id")
            .to_owned();
        self.session_id = Some(session_id.clone());
        let initialized = self
            .client
            .post(format!("{}/mcp", self.base))
            .header("Accept", "text/event-stream, application/json")
            .header("Mcp-Session-Id", session_id)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {}
            }))
            .send()
            .await
            .expect("initialized notification should send");
        assert_eq!(initialized.status(), reqwest::StatusCode::ACCEPTED);
    }

    async fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        let id = self.next_id();
        let response = self
            .post_mcp(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": {"name": name, "arguments": arguments}
            }))
            .await;
        tool_payload(name, response)
    }

    async fn wait_for_emergency_stop(&mut self) -> Value {
        let started = std::time::Instant::now();
        loop {
            let state = self.call_tool("control.state", serde_json::json!({})).await;
            if state["control"]["emergency_stop_active"] == true {
                return state;
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "timed out waiting for Ctrl+Alt+Esc emergency stop; last state: {state:#}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn wait_for_crash_report_event(&mut self, pid: u32) -> Value {
        let started = std::time::Instant::now();
        loop {
            let report = self
                .call_tool("diagnostics.crash_report", serde_json::json!({"pid": pid}))
                .await;
            assert_ok("diagnostics.crash_report", &report);
            if report["event_log"]["entries"]
                .as_array()
                .map(|entries| !entries.is_empty())
                .unwrap_or(false)
            {
                return report;
            }
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "timed out waiting for crash_report Event Log entry; last report: {report:#}"
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn wait_for_dialog_button(&mut self, button_name: &str) -> Value {
        let started = std::time::Instant::now();
        loop {
            let list = self
                .call_tool(
                    "dialogs.list",
                    serde_json::json!({
                        "max_depth": 8,
                        "max_elements": 1000
                    }),
                )
                .await;
            assert_ok("dialogs.list", &list);
            if let Some(dialog) = list["dialogs"]
                .as_array()
                .and_then(|dialogs| {
                    dialogs.iter().find(|dialog| {
                        dialog["buttons"]
                            .as_array()
                            .map(|buttons| {
                                buttons.iter().any(|button| button["name"] == button_name)
                            })
                            .unwrap_or(false)
                    })
                })
                .cloned()
            {
                return dialog;
            }
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "timed out waiting for dialog button {button_name:?}; last dialogs.list: {list:#}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn wait_for_health(&self) {
        let started = std::time::Instant::now();
        loop {
            match self
                .client
                .get(format!("{}/healthz", self.base))
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => return,
                _ if started.elapsed() < Duration::from_secs(10) => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                _ => panic!("HTTP server did not become healthy"),
            }
        }
    }

    async fn post_mcp(&self, body: Value) -> Value {
        let mut request = self
            .client
            .post(format!("{}/mcp", self.base))
            .header("Accept", "text/event-stream, application/json")
            .json(&body);
        if let Some(session_id) = &self.session_id {
            request = request.header("Mcp-Session-Id", session_id);
        }
        let response = request.send().await.expect("MCP request should send");
        assert!(
            response.status().is_success(),
            "MCP request failed with status {}",
            response.status()
        );
        let session = response
            .headers()
            .get("Mcp-Session-Id")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let text = response.text().await.expect("MCP response should be text");
        let mut value = if content_type.starts_with("text/event-stream") {
            parse_first_sse_json(&text)
        } else {
            serde_json::from_str::<Value>(&text)
                .unwrap_or_else(|error| panic!("HTTP MCP response was not JSON: {text:?}: {error}"))
        };
        if let Some(session) = session {
            value["__session_id"] = Value::String(session);
        }
        value
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

async fn runtime_test_lock() -> tokio::sync::MutexGuard<'static, ()> {
    WINDOWS_RUNTIME_TEST_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

impl Drop for McpHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn action_args(bound_id: &str, selector: Value) -> Value {
    serde_json::json!({
        "bound_id": bound_id,
        "selector": selector,
        "max_depth": 12,
        "max_elements": 4000
    })
}

fn expand_args(bound_id: &str, action: &str) -> Value {
    serde_json::json!({
        "bound_id": bound_id,
        "selector": {"role": "ComboBox"},
        "expand_collapse_action": action,
        "max_depth": 12,
        "max_elements": 4000
    })
}

fn selector(name: &str, role: &str) -> Value {
    serde_json::json!({"name": name, "role": role})
}

fn assert_ok(tool: &str, value: &Value) {
    assert_eq!(value["ok"], true, "{tool} failed: {value:#}");
}

fn assert_direct_pattern(tool: &str, value: &Value, pattern: &str) {
    assert_ok(tool, value);
    assert_eq!(value["outcome"]["direct_uia_pattern_used"], true);
    assert_eq!(value["outcome"]["pattern_used"], pattern);
}

fn assert_direct_pattern_prefix(tool: &str, value: &Value, pattern_prefix: &str) {
    assert_ok(tool, value);
    assert_eq!(value["outcome"]["direct_uia_pattern_used"], true);
    let pattern = value["outcome"]["pattern_used"]
        .as_str()
        .expect("pattern_used should be a string");
    assert!(
        pattern.starts_with(pattern_prefix),
        "{tool} used unexpected pattern {pattern:?}: {value:#}"
    );
}

fn assert_control_event(value: &Value, kind: &str) {
    let events = value["control"]["events"]
        .as_array()
        .expect("control.state should return an event array");
    assert!(
        events.iter().any(|event| event["kind"] == kind),
        "control events should contain {kind}: {value:#}"
    );
}

fn assert_audit_entry(value: &Value, kind: &str, message_contains: &str) {
    let entries = value["control"]["audit_log"]["recent_persisted_entries"]
        .as_array()
        .expect("control.state should return persisted audit entries");
    assert!(
        entries.iter().any(|record| {
            record["event"]["kind"] == kind
                && record["event"]["message"]
                    .as_str()
                    .map(|message| message.contains(message_contains))
                    .unwrap_or(false)
        }),
        "control audit entries should contain {kind:?} with {message_contains:?}: {value:#}"
    );
}

fn tool_payload(tool: &str, response: Value) -> Value {
    assert_eq!(
        response["jsonrpc"], "2.0",
        "{tool} returned invalid MCP response: {response:#}"
    );
    let result = &response["result"];
    assert_ne!(
        result["isError"], true,
        "{tool} returned an MCP tool error: {response:#}"
    );
    if result["structuredContent"].is_object() {
        return result["structuredContent"].clone();
    }
    if let Some(content) = result["content"].as_array() {
        for item in content {
            if item["type"] == "text" {
                let text = item["text"]
                    .as_str()
                    .expect("text content should be a string");
                return serde_json::from_str(text).unwrap_or_else(|error| {
                    panic!("{tool} text content was not JSON: {text:?}: {error}")
                });
            }
        }
    }
    panic!("{tool} response did not contain JSON tool payload: {response:#}");
}

fn desired_toggle_args(bound_id: &str, name: &str, role: &str, desired_state: &str) -> Value {
    let mut args = action_args(bound_id, selector(name, role));
    args["desired_state"] = serde_json::json!(desired_state);
    args
}

fn select_mode_args(bound_id: &str, name: &str, role: &str, mode: &str) -> Value {
    let mut args = action_args(bound_id, selector(name, role));
    args["mode"] = serde_json::json!(mode);
    args
}

async fn launch_crashing_target(target_exe: &PathBuf) -> u32 {
    let mut child = Command::new(target_exe)
        .arg("--title")
        .arg("winctl crash target")
        .arg("--class")
        .arg("WinctlCrashTarget")
        .arg("--width")
        .arg("360")
        .arg("--height")
        .arg("220")
        .arg("--crash-after-ms")
        .arg("500")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("crashing test target should start");
    let pid = child.id();
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .expect("crashing test target wait should not fail")
        {
            assert!(
                !status.success(),
                "crashing test target should exit with a failure status"
            );
            return pid;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "crashing test target did not exit"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn parse_first_sse_json(text: &str) -> Value {
    let data = text
        .lines()
        .find_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .unwrap_or_else(|| panic!("SSE response did not contain data: {text:?}"));
    serde_json::from_str(data)
        .unwrap_or_else(|error| panic!("SSE data was not JSON: {data:?}: {error}"))
}

fn target_exe_path() -> PathBuf {
    if let Some(path) = std::env::var_os("WINCTL_TEST_TARGET_EXE") {
        return windows_path(&path.to_string_lossy());
    }
    let current = std::env::current_exe().expect("current exe should resolve");
    let debug_dir = current
        .parent()
        .and_then(|deps| deps.parent())
        .expect("test exe should live under target/.../debug/deps");
    debug_dir.join("winctl-test-target.exe")
}

fn server_exe_path() -> PathBuf {
    if let Some(path) = std::env::var_os("WINCTL_MCP_SERVER_EXE") {
        return windows_path(&path.to_string_lossy());
    }
    windows_path(env!("CARGO_BIN_EXE_winctl-mcp-server"))
}

fn windows_path(path: &str) -> PathBuf {
    if path.starts_with('/') {
        if let Ok(distro) = std::env::var("WSL_DISTRO_NAME") {
            let mut converted = format!(r"\\wsl.localhost\{distro}");
            converted.push_str(&path.replace('/', r"\"));
            return PathBuf::from(converted);
        }
    }
    PathBuf::from(path)
}

fn temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()))
}

fn send_ctrl_alt_esc() {
    let inputs = [
        key_input(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
        key_input(VK_MENU, KEYBD_EVENT_FLAGS(0)),
        key_input(VK_ESCAPE, KEYBD_EVENT_FLAGS(0)),
        key_input(VK_ESCAPE, KEYEVENTF_KEYUP),
        key_input(VK_MENU, KEYEVENTF_KEYUP),
        key_input(VK_CONTROL, KEYEVENTF_KEYUP),
    ];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    assert_eq!(
        sent,
        inputs.len() as u32,
        "SendInput should deliver Ctrl+Alt+Esc"
    );
}

fn send_key(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) {
    let inputs = [
        key_input(vk, KEYBD_EVENT_FLAGS(0)),
        key_input(vk, KEYEVENTF_KEYUP),
    ];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    assert_eq!(sent, inputs.len() as u32, "SendInput should deliver key");
}

fn key_input(
    vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY,
    flags: KEYBD_EVENT_FLAGS,
) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
