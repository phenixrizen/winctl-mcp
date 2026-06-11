use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct ServerGuard(Child);
impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
}

fn spawn_server(port: u16, cfg_path: &std::path::Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_winctl-mcp-server"))
        .arg("serve")
        .arg("--transport")
        .arg("http")
        .arg("--listen")
        .arg(format!("127.0.0.1:{port}"))
        .arg("--config")
        .arg(cfg_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("server should start")
}

async fn wait_for_health(client: &reqwest::Client, base: &str) {
    let started = std::time::Instant::now();
    loop {
        match client.get(format!("{base}/healthz")).send().await {
            Ok(response) if response.status().is_success() => return,
            _ if started.elapsed() < Duration::from_secs(10) => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            _ => panic!("server did not become healthy within 10s"),
        }
    }
}

/// Load / save / persist / gating over HTTP on a tokenless-loopback server.
#[tokio::test]
async fn config_endpoint_loads_saves_and_gates() {
    let port = free_port();
    let cfg_path = std::env::temp_dir().join(format!(
        "winctl-config-test-{}-{port}.toml",
        std::process::id()
    ));
    std::fs::write(
        &cfg_path,
        format!(
            "[transport]\nmode = \"http\"\nlisten = \"127.0.0.1:{port}\"\n\n[policy]\nenable_filesystem_mutation = false\n"
        ),
    )
    .expect("write temp config");

    let child = spawn_server(port, &cfg_path);
    let _guard = ServerGuard(child);
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();
    wait_for_health(&client, &base).await;

    // 1. GET /dashboard/config → editable == true, policy.enable_filesystem_mutation == false,
    //    and the response body must NOT contain a literal token value.
    let get: serde_json::Value = client
        .get(format!("{base}/dashboard/config"))
        .send()
        .await
        .expect("GET /dashboard/config should respond")
        .json()
        .await
        .expect("GET /dashboard/config should return JSON");
    assert_eq!(get["ok"], true, "GET should return ok=true: {get}");
    assert_eq!(
        get["editable"], true,
        "GET should return editable=true: {get}"
    );
    assert_eq!(
        get["sections"]["policy"]["enable_filesystem_mutation"], false,
        "policy.enable_filesystem_mutation should be false: {get}"
    );
    // The response must never expose a raw token value (only booleans are allowed).
    let get_str = get.to_string();
    // No [auth] section in this config → no token at all; confirm nothing leaks anyway.
    // Pattern: reject any occurrence of `"token":"<non-empty>"`.
    assert!(
        !get_str.contains("\"token\":\""),
        "GET must not expose a raw token value: {get_str}"
    );

    // 2. POST /dashboard/config with NO Authorization header → 403.
    let unauth = client
        .post(format!("{base}/dashboard/config"))
        .json(&serde_json::json!({"policy": {"enable_filesystem_mutation": true}}))
        .send()
        .await
        .expect("unauthenticated POST should respond");
    assert_eq!(
        unauth.status().as_u16(),
        403,
        "POST without bearer should be 403"
    );

    // 3. Bootstrap a token via the open (tokenless-loopback) connect endpoint.
    let token_resp: serde_json::Value = client
        .post(format!("{base}/dashboard/connect/token"))
        .json(&serde_json::json!({"label": "config-http-test"}))
        .send()
        .await
        .expect("POST /dashboard/connect/token should respond")
        .json()
        .await
        .expect("token create should return JSON");
    assert_eq!(
        token_resp["ok"], true,
        "token creation should succeed: {token_resp}"
    );
    let bearer = token_resp["token"]
        .as_str()
        .expect("token field should be a string")
        .to_owned();

    // 4. POST /dashboard/config with valid bearer → 200.
    let save = client
        .post(format!("{base}/dashboard/config"))
        .bearer_auth(&bearer)
        .json(&serde_json::json!({"policy": {"enable_filesystem_mutation": true}}))
        .send()
        .await
        .expect("authorized POST should respond");
    assert_eq!(
        save.status().as_u16(),
        200,
        "authorized POST should be 200"
    );

    // 5. Read the config file from disk and verify the change persisted and
    //    the locked [transport] section is preserved.
    let on_disk = std::fs::read_to_string(&cfg_path).expect("config file should be readable");
    assert!(
        on_disk.contains("enable_filesystem_mutation = true"),
        "change must be persisted: {on_disk}"
    );
    assert!(
        on_disk.contains("[transport]"),
        "[transport] section must be preserved: {on_disk}"
    );

    // 6. POST /dashboard/restart with NO token → 403 (gating check only; no real restart).
    let restart_unauth = client
        .post(format!("{base}/dashboard/restart"))
        .send()
        .await
        .expect("unauthenticated POST /dashboard/restart should respond");
    assert_eq!(
        restart_unauth.status().as_u16(),
        403,
        "POST /dashboard/restart without bearer should be 403"
    );

    let _ = std::fs::remove_file(&cfg_path);
}

/// Regression test: saving must fail safely (422) when the on-disk config is
/// unparseable — the file must NOT be overwritten with defaults or the edit.
#[tokio::test]
async fn config_save_fails_safely_when_disk_file_is_unparseable() {
    let port = free_port();
    let cfg_path = std::env::temp_dir().join(format!(
        "winctl-config-corrupt-{}-{port}.toml",
        std::process::id()
    ));
    std::fs::write(
        &cfg_path,
        format!(
            "[transport]\nmode = \"http\"\nlisten = \"127.0.0.1:{port}\"\n\n[policy]\nenable_filesystem_mutation = false\n"
        ),
    )
    .expect("write temp config");

    let child = spawn_server(port, &cfg_path);
    let _guard = ServerGuard(child);
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();
    wait_for_health(&client, &base).await;

    // Bootstrap a token.
    let token_resp: serde_json::Value = client
        .post(format!("{base}/dashboard/connect/token"))
        .json(&serde_json::json!({"label": "corrupt-test"}))
        .send()
        .await
        .expect("token create should respond")
        .json()
        .await
        .expect("token create should return JSON");
    let bearer = token_resp["token"]
        .as_str()
        .expect("token should be a string")
        .to_owned();

    // Overwrite the config file on disk with invalid TOML.
    let invalid_toml = "this is not valid toml = = =";
    std::fs::write(&cfg_path, invalid_toml).expect("overwrite with invalid TOML");

    // POST /dashboard/config with bearer → expect 422 with reason config_parse_failed.
    let save = client
        .post(format!("{base}/dashboard/config"))
        .bearer_auth(&bearer)
        .json(&serde_json::json!({"policy": {"enable_filesystem_mutation": true}}))
        .send()
        .await
        .expect("POST should respond");
    let status = save.status().as_u16();
    let body: serde_json::Value = save.json().await.expect("response should be JSON");
    assert_eq!(
        status, 422,
        "save over unparseable config should be 422, got {status}: {body}"
    );
    assert_eq!(
        body["reason"], "config_parse_failed",
        "reason should be config_parse_failed: {body}"
    );

    // The file must still contain the invalid content — not overwritten.
    let on_disk = std::fs::read_to_string(&cfg_path).expect("config file should be readable");
    assert_eq!(
        on_disk, invalid_toml,
        "invalid file must NOT be overwritten: got {on_disk:?}"
    );

    let _ = std::fs::remove_file(&cfg_path);
}
