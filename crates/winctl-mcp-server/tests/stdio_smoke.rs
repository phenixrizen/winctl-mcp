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
        .arg("--self-test")
        .output()
        .expect("self-test should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("self-test stdout should be UTF-8");
    assert!(stdout.contains("winctl-mcp-server self-test"));
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

fn temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()))
}
