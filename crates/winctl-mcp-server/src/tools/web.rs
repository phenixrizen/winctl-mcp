use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use reqwest::Url;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::{CdpEndpointRequest, CdpEvaluateRequest, WebIntrospectionRequest};

type CdpWebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

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
    match connect_target(&target, request.timeout_ms).await {
        Ok(mut socket) => {
            let result = send_cdp_command(
                &mut socket,
                1,
                "Runtime.evaluate",
                serde_json::json!({
                    "expression": request.expression,
                    "returnByValue": true,
                    "awaitPromise": true,
                }),
                request.timeout_ms,
            )
            .await;
            match result {
                Ok(result) => serde_json::json!({
                    "ok": true,
                    "target": target,
                    "result": result,
                }),
                Err(error) => error,
            }
        }
        Err(error) => error,
    }
}

pub async fn dom_snapshot(request: WebIntrospectionRequest) -> serde_json::Value {
    run_introspection_command(
        request,
        "web.dom.snapshot",
        "DOMSnapshot.captureSnapshot",
        serde_json::json!({
            "computedStyles": [],
            "includeDOMRects": true,
            "includePaintOrder": true,
        }),
    )
    .await
}

pub async fn network_events(request: WebIntrospectionRequest) -> serde_json::Value {
    tracing::info!(
        debugger_url = %request.debugger_url,
        target_id = ?request.target_id,
        "web.network.events requested"
    );
    let targets = match discover_targets(&request.debugger_url, request.timeout_ms).await {
        Ok(targets) => targets,
        Err(error) => return error,
    };
    let target = select_target(&targets, request.target_id.as_deref());
    let Some(target) = target else {
        return target_not_found(targets);
    };
    let mut socket = match connect_target(&target, request.timeout_ms).await {
        Ok(socket) => socket,
        Err(error) => return error,
    };
    if let Err(error) = send_cdp_command(
        &mut socket,
        1,
        "Network.enable",
        serde_json::json!({}),
        request.timeout_ms,
    )
    .await
    {
        return error;
    }
    let wait = Duration::from_millis(request.timeout_ms.unwrap_or(1_000).clamp(100, 10_000));
    let events =
        collect_cdp_events(&mut socket, wait, |method| method.starts_with("Network.")).await;
    serde_json::json!({
        "ok": true,
        "target": target,
        "events": events,
        "wait_ms": wait.as_millis() as u64,
    })
}

pub async fn a11y_snapshot(request: WebIntrospectionRequest) -> serde_json::Value {
    run_introspection_command(
        request,
        "web.a11y.snapshot",
        "Accessibility.getFullAXTree",
        serde_json::json!({}),
    )
    .await
}

pub async fn style_inspect(request: WebIntrospectionRequest) -> serde_json::Value {
    tracing::info!(
        debugger_url = %request.debugger_url,
        target_id = ?request.target_id,
        selector = ?request.selector,
        "web.style.inspect requested"
    );
    let targets = match discover_targets(&request.debugger_url, request.timeout_ms).await {
        Ok(targets) => targets,
        Err(error) => return error,
    };
    let target = select_target(&targets, request.target_id.as_deref());
    let Some(target) = target else {
        return target_not_found(targets);
    };
    let selector = request.selector.unwrap_or_else(|| "body".into());
    let mut socket = match connect_target(&target, request.timeout_ms).await {
        Ok(socket) => socket,
        Err(error) => return error,
    };
    let _ = send_cdp_command(
        &mut socket,
        1,
        "DOM.enable",
        serde_json::json!({}),
        request.timeout_ms,
    )
    .await;
    let _ = send_cdp_command(
        &mut socket,
        2,
        "CSS.enable",
        serde_json::json!({}),
        request.timeout_ms,
    )
    .await;
    let document = match send_cdp_command(
        &mut socket,
        3,
        "DOM.getDocument",
        serde_json::json!({"depth": 1, "pierce": true}),
        request.timeout_ms,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error,
    };
    let Some(root_node_id) = document
        .get("result")
        .and_then(|result| result.get("root"))
        .and_then(|root| root.get("nodeId"))
        .and_then(|value| value.as_i64())
    else {
        return serde_json::json!({
            "ok": false,
            "target": target,
            "error": {
                "code": "cdp_document_root_missing",
                "message": "DOM.getDocument did not return a root nodeId",
            }
        });
    };
    let node = match send_cdp_command(
        &mut socket,
        4,
        "DOM.querySelector",
        serde_json::json!({"nodeId": root_node_id, "selector": selector}),
        request.timeout_ms,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error,
    };
    let Some(node_id) = node
        .get("result")
        .and_then(|result| result.get("nodeId"))
        .and_then(|value| value.as_i64())
        .filter(|value| *value != 0)
    else {
        return serde_json::json!({
            "ok": false,
            "target": target,
            "selector": selector,
            "error": {
                "code": "cdp_selector_not_found",
                "message": "DOM.querySelector did not match an element",
            }
        });
    };
    let computed_style = match send_cdp_command(
        &mut socket,
        5,
        "CSS.getComputedStyleForNode",
        serde_json::json!({"nodeId": node_id}),
        request.timeout_ms,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return error,
    };
    serde_json::json!({
        "ok": true,
        "target": target,
        "selector": selector,
        "node_id": node_id,
        "computed_style": computed_style,
    })
}

async fn run_introspection_command(
    request: WebIntrospectionRequest,
    tool_name: &'static str,
    cdp_method: &'static str,
    params: serde_json::Value,
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
    let Some(target) = target else {
        return target_not_found(targets);
    };
    let mut socket = match connect_target(&target, request.timeout_ms).await {
        Ok(socket) => socket,
        Err(error) => return error,
    };
    match send_cdp_command(&mut socket, 1, cdp_method, params, request.timeout_ms).await {
        Ok(result) => serde_json::json!({
            "ok": true,
            "target": target,
            "selector": request.selector,
            "cdp_method": cdp_method,
            "result": result,
        }),
        Err(error) => error,
    }
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

async fn connect_target(
    target: &serde_json::Value,
    timeout_ms: Option<u64>,
) -> Result<CdpWebSocket, serde_json::Value> {
    let websocket_url = target
        .get("webSocketDebuggerUrl")
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            serde_json::json!({
                "ok": false,
                "target": target,
                "error": {
                    "code": "cdp_websocket_url_missing",
                    "message": "selected CDP target does not include webSocketDebuggerUrl",
                }
            })
        })?;
    validate_local_websocket_url(websocket_url)?;
    let connect = connect_async(websocket_url);
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(5_000).clamp(500, 60_000));
    match tokio::time::timeout(timeout, connect).await {
        Ok(Ok((socket, _response))) => Ok(socket),
        Ok(Err(error)) => Err(serde_json::json!({
            "ok": false,
            "target": target,
            "error": {
                "code": "cdp_websocket_connect_failed",
                "message": format!("failed to connect to CDP WebSocket: {error}"),
            }
        })),
        Err(_) => Err(serde_json::json!({
            "ok": false,
            "target": target,
            "error": {
                "code": "cdp_websocket_connect_timeout",
                "message": format!("timed out connecting to CDP WebSocket after {} ms", timeout.as_millis()),
            }
        })),
    }
}

async fn send_cdp_command(
    socket: &mut CdpWebSocket,
    id: u64,
    method: &str,
    params: serde_json::Value,
    timeout_ms: Option<u64>,
) -> Result<serde_json::Value, serde_json::Value> {
    let request = serde_json::json!({
        "id": id,
        "method": method,
        "params": params,
    });
    socket
        .send(Message::Text(request.to_string().into()))
        .await
        .map_err(|error| {
            cdp_error(
                "cdp_websocket_send_failed",
                format!("{method} send failed: {error}"),
            )
        })?;

    let timeout = Duration::from_millis(timeout_ms.unwrap_or(5_000).clamp(500, 60_000));
    match tokio::time::timeout(timeout, read_cdp_response(socket, id)).await {
        Ok(result) => result,
        Err(_) => Err(cdp_error(
            "cdp_command_timeout",
            format!(
                "{method} did not return response id {id} before {} ms",
                timeout.as_millis()
            ),
        )),
    }
}

async fn read_cdp_response(
    socket: &mut CdpWebSocket,
    id: u64,
) -> Result<serde_json::Value, serde_json::Value> {
    while let Some(message) = socket.next().await {
        let message = message.map_err(|error| {
            cdp_error(
                "cdp_websocket_read_failed",
                format!("failed reading CDP response: {error}"),
            )
        })?;
        let text = match message {
            Message::Text(text) => text.to_string(),
            Message::Binary(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Message::Close(_) => {
                return Err(cdp_error(
                    "cdp_websocket_closed",
                    "CDP WebSocket closed before the command response arrived",
                ));
            }
            _ => continue,
        };
        let value: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
            cdp_error(
                "cdp_json_decode_failed",
                format!("failed to decode CDP WebSocket message: {error}"),
            )
        })?;
        if value.get("id").and_then(|value| value.as_u64()) == Some(id) {
            if let Some(error) = value.get("error") {
                return Err(serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": "cdp_command_failed",
                        "message": "CDP command returned an error",
                        "cdp_error": error,
                    }
                }));
            }
            return Ok(value);
        }
    }
    Err(cdp_error(
        "cdp_websocket_eof",
        "CDP WebSocket ended before the command response arrived",
    ))
}

async fn collect_cdp_events<F>(
    socket: &mut CdpWebSocket,
    wait: Duration,
    include: F,
) -> Vec<serde_json::Value>
where
    F: Fn(&str) -> bool,
{
    let started = tokio::time::Instant::now();
    let mut events = Vec::new();
    while started.elapsed() < wait {
        let remaining = wait.saturating_sub(started.elapsed());
        let message = tokio::time::timeout(remaining, socket.next()).await;
        let Ok(Some(Ok(message))) = message else {
            break;
        };
        let text = match message {
            Message::Text(text) => text.to_string(),
            Message::Binary(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            _ => continue,
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(method) = value.get("method").and_then(|value| value.as_str()) else {
            continue;
        };
        if include(method) {
            events.push(value);
        }
        if events.len() >= 500 {
            break;
        }
    }
    events
}

fn validate_local_websocket_url(raw: &str) -> Result<(), serde_json::Value> {
    let url = Url::parse(raw).map_err(|error| {
        serde_json::json!({
            "ok": false,
            "error": {
                "code": "invalid_cdp_websocket_url",
                "message": format!("invalid webSocketDebuggerUrl: {error}"),
            }
        })
    })?;
    if url.scheme() != "ws" && url.scheme() != "wss" {
        return Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "unsupported_cdp_websocket_scheme",
                "message": "CDP WebSocket endpoints must use ws or wss",
            }
        }));
    }
    let host = url.host_str().unwrap_or_default();
    let loopback = matches!(host, "localhost" | "127.0.0.1" | "::1") || host.starts_with("127.");
    if !loopback {
        return Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "cdp_websocket_not_loopback",
                "message": "CDP WebSocket endpoints must be loopback",
            }
        }));
    }
    Ok(())
}

fn target_not_found(targets: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "error": {
            "code": "cdp_target_not_found",
            "message": "requested CDP target was not found",
        },
        "targets": targets,
    })
}

fn cdp_error(code: &str, message: impl Into<String>) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message.into(),
        }
    })
}
