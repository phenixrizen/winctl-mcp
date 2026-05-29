use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

#[test]
fn mcp_stdio_stdout_is_json_rpc_only_and_logs_to_file() {
    let log_path = temp_path("winctl-stdio-smoke.log");
    let mut child = Command::new(env!("CARGO_BIN_EXE_winctl-mcp-server"))
        .arg("serve")
        .arg("--transport")
        .arg("stdio")
        .arg("--log-file")
        .arg(&log_path)
        .env("RUST_LOG", "info")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("server should start");

    let mut stdin = child.stdin.take().expect("stdin should be piped");
    let stdout = child.stdout.take().expect("stdout should be piped");
    let stdout_rx = collect_stdout_lines(stdout);

    send_json(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "stdio-smoke", "version": "0.1"}
            }
        }),
    );
    assert_json_rpc_response(&stdout_rx, 1);

    send_json(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
    );
    send_json(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let tools = assert_json_rpc_response(&stdout_rx, 2);
    let tool_names: Vec<_> = tools["result"]["tools"]
        .as_array()
        .expect("tools/list should return tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(tool_names.contains(&"server.ping"));
    assert!(tool_names.contains(&"windows.list"));

    send_json(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "server.ping", "arguments": {}}
        }),
    );
    assert_json_rpc_response(&stdout_rx, 3);

    send_json(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {"name": "windows.list", "arguments": {}}
        }),
    );
    assert_json_rpc_response(&stdout_rx, 4);

    assert!(
        child
            .try_wait()
            .expect("server status should be readable")
            .is_none(),
        "server should remain alive after basic MCP calls"
    );

    drop(stdin);
    wait_for_exit(&mut child, Duration::from_secs(5));

    while let Ok(line) = stdout_rx.try_recv() {
        serde_json::from_str::<Value>(&line)
            .unwrap_or_else(|error| panic!("stdout line was not JSON-RPC: {line:?}: {error}"));
    }

    let log = fs::read_to_string(&log_path).expect("log file should be readable");
    assert!(log.contains("winctl-mcp-server starting on stdio"));
    assert!(log.contains("server.ping requested"));
    assert!(log.contains("windows.list requested"));

    let _ = fs::remove_file(log_path);
}

#[test]
fn self_test_mode_may_write_human_output_to_stdout() {
    let output = Command::new(env!("CARGO_BIN_EXE_winctl-mcp-server"))
        .arg("self-test")
        .arg("windows-list")
        .output()
        .expect("self-test should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("self-test stdout should be UTF-8");
    assert!(stdout.contains("winctl-mcp-server self-test"));
    assert!(stdout.contains("windows-list"));
}

#[tokio::test]
async fn streamable_http_health_and_tool_listing_work() {
    let log_path = temp_path("winctl-http-smoke.log");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("port should bind");
    let addr = listener.local_addr().expect("local addr should resolve");
    drop(listener);

    let mut child = Command::new(env!("CARGO_BIN_EXE_winctl-mcp-server"))
        .arg("serve")
        .arg("--transport")
        .arg("http")
        .arg("--listen")
        .arg(addr.to_string())
        .arg("--log-file")
        .arg(&log_path)
        .env("RUST_LOG", "info")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("server should start");

    let client = reqwest::Client::new();
    let base = format!("http://{addr}");
    wait_for_health(&client, &base).await;
    let dashboard = client
        .get(format!("{base}/dashboard"))
        .send()
        .await
        .expect("dashboard should respond");
    assert_eq!(dashboard.status(), reqwest::StatusCode::OK);
    let dashboard_body = dashboard
        .text()
        .await
        .expect("dashboard body should be readable");
    assert!(dashboard_body.contains("winctl-mcp dashboard"));

    let dashboard_state: Value = client
        .get(format!("{base}/dashboard/state"))
        .send()
        .await
        .expect("dashboard state should respond")
        .json()
        .await
        .expect("dashboard state should be JSON");
    assert_eq!(dashboard_state["ok"], true);
    assert_eq!(dashboard_state["service"], "winctl-mcp-server");
    let recorder = client
        .get(format!("{base}/recorder"))
        .send()
        .await
        .expect("recorder should respond");
    assert_eq!(recorder.status(), reqwest::StatusCode::OK);
    let recorder_body = recorder.text().await.expect("recorder body");
    assert!(recorder_body.contains("winctl-mcp recorder"));

    let init = post_mcp(
        &client,
        &base,
        None,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "http-smoke", "version": "0.1"}
            }
        }),
    )
    .await;
    assert_eq!(init["jsonrpc"], "2.0");
    assert_eq!(init["id"], 1);
    let session_id = init["__session_id"]
        .as_str()
        .expect("initialize should return Mcp-Session-Id")
        .to_owned();

    let initialized = client
        .post(format!("{base}/mcp"))
        .header("Accept", "text/event-stream, application/json")
        .header("Mcp-Session-Id", &session_id)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }))
        .send()
        .await
        .expect("initialized notification should send");
    assert_eq!(initialized.status(), reqwest::StatusCode::ACCEPTED);

    let tools = post_mcp(
        &client,
        &base,
        Some(&session_id),
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await;
    assert_eq!(tools["jsonrpc"], "2.0");
    assert_eq!(tools["id"], 2);
    let tool_names: Vec<_> = tools["result"]["tools"]
        .as_array()
        .expect("tools/list should return tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(tool_names.contains(&"server.ping"));
    assert!(tool_names.contains(&"windows.list"));
    assert!(tool_names.contains(&"process.launch"));
    assert!(tool_names.contains(&"windows.wait_for_window"));

    assert!(
        child
            .try_wait()
            .expect("server status should be readable")
            .is_none(),
        "server should remain alive after HTTP MCP calls"
    );

    child.kill().expect("server should be killable");
    let _ = child.wait();
    let log = fs::read_to_string(&log_path).expect("log file should be readable");
    assert!(log.contains("winctl-mcp-server starting on HTTP"));
    assert!(log.contains("HTTP health check requested"));
    let _ = fs::remove_file(log_path);
}

fn send_json(stdin: &mut impl Write, value: Value) {
    let line = serde_json::to_string(&value).expect("request should serialize");
    writeln!(stdin, "{line}").expect("request should write");
    stdin.flush().expect("request should flush");
}

fn assert_json_rpc_response(rx: &Receiver<String>, id: i64) -> Value {
    let line = rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or_else(|_| panic!("timed out waiting for response id {id}"));
    let value: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("stdout line was not JSON-RPC: {line:?}: {error}"));
    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], id);
    value
}

fn collect_stdout_lines(stdout: impl std::io::Read + Send + 'static) -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

fn wait_for_exit(child: &mut Child, timeout: Duration) {
    let started = std::time::Instant::now();
    loop {
        if child
            .try_wait()
            .expect("server status should be readable")
            .is_some()
        {
            return;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            panic!("server did not exit after stdin closed");
        }
        thread::sleep(Duration::from_millis(50));
    }
}

async fn wait_for_health(client: &reqwest::Client, base: &str) {
    let started = std::time::Instant::now();
    loop {
        match client.get(format!("{base}/healthz")).send().await {
            Ok(response) if response.status().is_success() => return,
            _ if started.elapsed() < Duration::from_secs(5) => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            _ => panic!("HTTP server did not become healthy"),
        }
    }
}

async fn post_mcp(
    client: &reqwest::Client,
    base: &str,
    session_id: Option<&str>,
    body: Value,
) -> Value {
    let mut request = client
        .post(format!("{base}/mcp"))
        .header("Accept", "text/event-stream, application/json")
        .json(&body);
    if let Some(session_id) = session_id {
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

fn parse_first_sse_json(text: &str) -> Value {
    let data = text
        .lines()
        .find_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .unwrap_or_else(|| panic!("SSE response did not contain data: {text:?}"));
    serde_json::from_str(data)
        .unwrap_or_else(|error| panic!("SSE data was not JSON: {data:?}: {error}"))
}

fn temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()))
}
