use std::time::Duration;

use reqwest::Url;

use crate::{CdpEndpointRequest, CdpEvaluateRequest, WebIntrospectionRequest};

pub async fn cdp_list_targets(request: CdpEndpointRequest) -> serde_json::Value {
    tracing::info!(
        debugger_url = %request.debugger_url,
        timeout_ms = ?request.timeout_ms,
        "web.cdp.list_targets requested"
    );
    let base = match validate_local_debugger_url(&request.debugger_url) {
        Ok(url) => url,
        Err(error) => return error,
    };
    let client = match client(request.timeout_ms) {
        Ok(client) => client,
        Err(error) => return error,
    };
    let version = fetch_json(&client, join_debugger_url(&base, "/json/version")).await;
    let targets = fetch_json(&client, join_debugger_url(&base, "/json/list")).await;
    match targets {
        Ok(targets) => serde_json::json!({
            "ok": true,
            "debugger_url": base.as_str(),
            "version": version.ok(),
            "targets": targets,
        }),
        Err(error) => error,
    }
}

pub async fn cdp_evaluate(request: CdpEvaluateRequest) -> serde_json::Value {
    tracing::info!(
        debugger_url = %request.debugger_url,
        target_id = ?request.target_id,
        expression_len = request.expression.len(),
        "web.cdp.evaluate requested"
    );
    let targets = match discover_targets(&request.debugger_url, request.timeout_ms).await {
        Ok(targets) => targets,
        Err(error) => return error,
    };
    let target = select_target(&targets, request.target_id.as_deref());
    let Some(target) = target else {
        return serde_json::json!({
            "ok": false,
            "error": {
                "code": "cdp_target_not_found",
                "message": "requested CDP target was not found"
            },
            "targets": targets,
        });
    };
    serde_json::json!({
        "ok": false,
        "target": target,
        "expression": request.expression,
        "error": {
            "code": "cdp_websocket_not_implemented",
            "message": "CDP HTTP target discovery is available, but WebSocket command execution is not enabled in this build"
        }
    })
}

pub async fn dom_snapshot(request: WebIntrospectionRequest) -> serde_json::Value {
    unsupported_introspection(request, "web.dom.snapshot", "DOMSnapshot.captureSnapshot").await
}

pub async fn network_events(request: WebIntrospectionRequest) -> serde_json::Value {
    unsupported_introspection(request, "web.network.events", "Network.enable").await
}

pub async fn a11y_snapshot(request: WebIntrospectionRequest) -> serde_json::Value {
    unsupported_introspection(request, "web.a11y.snapshot", "Accessibility.getFullAXTree").await
}

pub async fn style_inspect(request: WebIntrospectionRequest) -> serde_json::Value {
    unsupported_introspection(request, "web.style.inspect", "CSS/DOM inspection").await
}

async fn unsupported_introspection(
    request: WebIntrospectionRequest,
    tool_name: &'static str,
    cdp_method: &'static str,
) -> serde_json::Value {
    tracing::info!(
        debugger_url = %request.debugger_url,
        target_id = ?request.target_id,
        selector = ?request.selector,
        tool_name = tool_name,
        "web introspection requested"
    );
    let targets = match discover_targets(&request.debugger_url, request.timeout_ms).await {
        Ok(targets) => targets,
        Err(error) => return error,
    };
    let target = select_target(&targets, request.target_id.as_deref());
    serde_json::json!({
        "ok": false,
        "provider_enabled": false,
        "target": target,
        "selector": request.selector,
        "cdp_method": cdp_method,
        "error": {
            "code": "cdp_websocket_not_implemented",
            "message": "CDP target discovery is available, but WebSocket-backed web introspection is not enabled in this build"
        },
        "targets": targets,
    })
}

async fn discover_targets(
    debugger_url: &str,
    timeout_ms: Option<u64>,
) -> Result<serde_json::Value, serde_json::Value> {
    let base = validate_local_debugger_url(debugger_url)?;
    let client = client(timeout_ms)?;
    fetch_json(&client, join_debugger_url(&base, "/json/list")).await
}

async fn fetch_json(
    client: &reqwest::Client,
    url: Url,
) -> Result<serde_json::Value, serde_json::Value> {
    let response = client.get(url.clone()).send().await.map_err(|error| {
        serde_json::json!({
            "ok": false,
            "error": {
                "code": "cdp_http_request_failed",
                "message": format!("failed to request {url}: {error}"),
            }
        })
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "cdp_http_status",
                "message": format!("{url} returned HTTP {status}"),
            }
        }));
    }
    response.json::<serde_json::Value>().await.map_err(|error| {
        serde_json::json!({
            "ok": false,
            "error": {
                "code": "cdp_json_decode_failed",
                "message": format!("failed to decode {url}: {error}"),
            }
        })
    })
}

fn client(timeout_ms: Option<u64>) -> Result<reqwest::Client, serde_json::Value> {
    reqwest::Client::builder()
        .timeout(Duration::from_millis(
            timeout_ms.unwrap_or(5_000).clamp(500, 60_000),
        ))
        .build()
        .map_err(|error| {
            serde_json::json!({
                "ok": false,
                "error": {
                    "code": "cdp_http_client_failed",
                    "message": format!("failed to initialize HTTP client: {error}"),
                }
            })
        })
}

fn validate_local_debugger_url(raw: &str) -> Result<Url, serde_json::Value> {
    let url = Url::parse(raw).map_err(|error| {
        serde_json::json!({
            "ok": false,
            "error": {
                "code": "invalid_debugger_url",
                "message": format!("invalid debugger_url: {error}"),
            }
        })
    })?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "unsupported_debugger_scheme",
                "message": "debugger_url must use http or https"
            }
        }));
    }
    let host = url.host_str().unwrap_or_default();
    let loopback = matches!(host, "localhost" | "127.0.0.1" | "::1") || host.starts_with("127.");
    if !loopback {
        return Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "debugger_url_not_loopback",
                "message": "CDP debugger endpoints must be loopback in this build"
            }
        }));
    }
    Ok(url)
}

fn join_debugger_url(base: &Url, path: &str) -> Url {
    let mut url = base.clone();
    url.set_path(path);
    url.set_query(None);
    url
}

fn select_target(
    targets: &serde_json::Value,
    target_id: Option<&str>,
) -> Option<serde_json::Value> {
    let array = targets.as_array()?;
    if let Some(target_id) = target_id {
        array
            .iter()
            .find(|target| target.get("id").and_then(|value| value.as_str()) == Some(target_id))
            .cloned()
    } else {
        array.first().cloned()
    }
}
