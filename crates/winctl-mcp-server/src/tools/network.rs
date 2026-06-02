use std::collections::HashMap;
use std::net::{IpAddr, ToSocketAddrs};
use std::time::Duration;

use crate::{NetworkFetchRequest, NetworkScrapeRequest};

const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const MAX_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MAX_BYTES: usize = 256 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

pub async fn network_fetch(
    request: NetworkFetchRequest,
    allow_private_network: bool,
) -> serde_json::Value {
    tracing::info!(
        url = %request.url,
        method = ?request.method,
        timeout_ms = ?request.timeout_ms,
        max_bytes = ?request.max_bytes,
        follow_redirects = request.follow_redirects,
        "network.fetch requested"
    );
    match fetch_bytes(request, allow_private_network).await {
        Ok(response) => serde_json::json!({
            "ok": true,
            "response": response,
        }),
        Err(error) => {
            tracing::warn!(error_code = %error.code, message = %error.message, "network.fetch failed");
            serde_json::json!({"ok": false, "error": error})
        }
    }
}

pub async fn network_scrape(
    request: NetworkScrapeRequest,
    allow_private_network: bool,
) -> serde_json::Value {
    tracing::info!(
        url = %request.url,
        timeout_ms = ?request.timeout_ms,
        max_bytes = ?request.max_bytes,
        follow_redirects = request.follow_redirects,
        "network.scrape requested"
    );
    let fetch_request = NetworkFetchRequest {
        url: request.url,
        method: Some("GET".into()),
        headers: None,
        body: None,
        timeout_ms: request.timeout_ms,
        max_bytes: request.max_bytes,
        follow_redirects: request.follow_redirects,
    };
    match fetch_bytes(fetch_request, allow_private_network).await {
        Ok(response) => {
            let text = response
                .get("text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            serde_json::json!({
                "ok": true,
                "source": response,
                "content": {
                    "title": html_title(text),
                    "links": if request.include_links { html_links(text) } else { Vec::<serde_json::Value>::new() },
                    "text": if request.include_text { Some(html_text(text)) } else { None },
                }
            })
        }
        Err(error) => {
            tracing::warn!(error_code = %error.code, message = %error.message, "network.scrape failed");
            serde_json::json!({"ok": false, "error": error})
        }
    }
}

async fn fetch_bytes(
    request: NetworkFetchRequest,
    allow_private_network: bool,
) -> Result<serde_json::Value, NetworkError> {
    let url = reqwest::Url::parse(&request.url).map_err(|error| NetworkError {
        code: "invalid_url".into(),
        message: format!("invalid URL: {error}"),
        warnings: vec![],
    })?;
    validate_url(&url)?;
    validate_destination(&url, allow_private_network)?;

    let timeout_ms = request
        .timeout_ms
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .min(MAX_TIMEOUT_MS);
    let max_bytes = request
        .max_bytes
        .unwrap_or(DEFAULT_MAX_BYTES)
        .min(MAX_RESPONSE_BYTES);
    let redirect_policy = if request.follow_redirects {
        reqwest::redirect::Policy::limited(5)
    } else {
        reqwest::redirect::Policy::none()
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .redirect(redirect_policy)
        .build()
        .map_err(|error| NetworkError {
            code: "client_build_failed".into(),
            message: format!("failed to build HTTP client: {error}"),
            warnings: vec![],
        })?;

    let method = request
        .method
        .unwrap_or_else(|| "GET".into())
        .to_uppercase();
    let method = match method.as_str() {
        "GET" => reqwest::Method::GET,
        "HEAD" => reqwest::Method::HEAD,
        "POST" => reqwest::Method::POST,
        _ => {
            return Err(NetworkError {
                code: "method_not_allowed".into(),
                message: "network.fetch allows GET, HEAD, and POST only".into(),
                warnings: vec![],
            })
        }
    };
    let mut builder = client.request(method.clone(), url.clone());
    if let Some(headers) = request.headers {
        for (key, value) in headers {
            builder = builder.header(key, value);
        }
    }
    if let Some(body) = request.body {
        if method != reqwest::Method::POST {
            return Err(NetworkError {
                code: "body_method_not_allowed".into(),
                message: "request body is only allowed with POST".into(),
                warnings: vec![],
            });
        }
        builder = builder.body(body);
    }

    let response = builder.send().await.map_err(|error| NetworkError {
        code: "request_failed".into(),
        message: format!("HTTP request failed: {error}"),
        warnings: vec![],
    })?;
    let status = response.status();
    let final_url = response.url().to_string();
    validate_url(response.url())?;
    validate_destination(response.url(), allow_private_network)?;
    let headers = response_headers(response.headers());
    if let Some(content_length) = response.content_length() {
        if content_length > max_bytes as u64 {
            return Err(NetworkError {
                code: "response_too_large".into(),
                message: format!(
                    "response content-length {content_length} exceeds max_bytes {max_bytes}"
                ),
                warnings: vec![],
            });
        }
    }

    let mut bytes = Vec::new();
    let mut response = response;
    let mut truncated = false;
    while let Some(chunk) = response.chunk().await.map_err(|error| NetworkError {
        code: "response_read_failed".into(),
        message: format!("failed to read response body: {error}"),
        warnings: vec![],
    })? {
        if bytes.len() + chunk.len() > max_bytes {
            let remaining = max_bytes.saturating_sub(bytes.len());
            bytes.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }
        bytes.extend_from_slice(&chunk);
    }
    let text = String::from_utf8(bytes.clone()).ok();
    let mut warnings: Vec<String> = Vec::new();
    if truncated {
        warnings.push("response body was truncated at max_bytes".into());
    }
    if text.is_none() && !bytes.is_empty() {
        warnings.push("response body is not valid UTF-8; text omitted".into());
    }
    Ok(serde_json::json!({
        "url": request.url,
        "final_url": final_url,
        "status": status.as_u16(),
        "headers": headers,
        "bytes_read": bytes.len(),
        "truncated": truncated,
        "utf8": text.is_some(),
        "text": text,
        "timeout_ms": timeout_ms,
        "max_bytes": max_bytes,
        "warnings": warnings,
    }))
}

fn validate_url(url: &reqwest::Url) -> Result<(), NetworkError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(NetworkError {
            code: "scheme_not_allowed".into(),
            message: "network tools allow http and https URLs only".into(),
            warnings: vec![],
        });
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(NetworkError {
            code: "url_credentials_not_allowed".into(),
            message: "network tools reject URLs containing credentials".into(),
            warnings: vec![],
        });
    }
    Ok(())
}

fn validate_destination(
    url: &reqwest::Url,
    allow_private_network: bool,
) -> Result<(), NetworkError> {
    if allow_private_network {
        return Ok(());
    }
    let host = url.host_str().ok_or_else(|| NetworkError {
        code: "missing_host".into(),
        message: "URL must include a host".into(),
        warnings: vec![],
    })?;
    if host.eq_ignore_ascii_case("localhost")
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".localdomain")
    {
        return Err(private_network_error(host, None));
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_destination(ip) {
            return Err(private_network_error(host, Some(ip)));
        }
        return Ok(());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|error| NetworkError {
            code: "dns_resolution_failed".into(),
            message: format!("failed to resolve {host}: {error}"),
            warnings: vec![],
        })?;
    for addr in addrs {
        if is_private_destination(addr.ip()) {
            return Err(private_network_error(host, Some(addr.ip())));
        }
    }
    Ok(())
}

fn is_private_destination(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_unspecified()
                || ip.is_multicast()
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || matches!(ip.segments()[0] & 0xfe00, 0xfc00)
                || matches!(ip.segments()[0] & 0xffc0, 0xfe80)
        }
    }
}

fn private_network_error(host: &str, ip: Option<IpAddr>) -> NetworkError {
    NetworkError {
        code: "private_network_blocked".into(),
        message: format!(
            "network tools block private/local destinations by default: host={host}, ip={}",
            ip.map(|ip| ip.to_string())
                .unwrap_or_else(|| "unresolved".into())
        ),
        warnings: vec![
            "enable allow_private_network in config or WINCTL_ALLOW_PRIVATE_NETWORK=1 to allow local/private destinations".into(),
        ],
    }
}

fn response_headers(headers: &reqwest::header::HeaderMap) -> HashMap<String, String> {
    headers
        .iter()
        .filter_map(|(key, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (key.as_str().to_ascii_lowercase(), value.to_string()))
        })
        .collect()
}

fn html_title(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let after_open = lower[start..].find('>')? + start + 1;
    let end = lower[after_open..].find("</title>")? + after_open;
    Some(decode_entities(text[after_open..end].trim()))
}

fn html_links(text: &str) -> Vec<serde_json::Value> {
    let lower = text.to_ascii_lowercase();
    let mut links = Vec::new();
    let mut offset = 0usize;
    while let Some(href_index) = lower[offset..].find("href=") {
        let href_index = offset + href_index + 5;
        let Some(quote) = text[href_index..].chars().next() else {
            break;
        };
        if quote != '"' && quote != '\'' {
            offset = href_index;
            continue;
        }
        let value_start = href_index + quote.len_utf8();
        let Some(value_end_delta) = text[value_start..].find(quote) else {
            break;
        };
        let value_end = value_start + value_end_delta;
        links.push(serde_json::json!({"href": decode_entities(&text[value_start..value_end])}));
        offset = value_end + quote.len_utf8();
        if links.len() >= 200 {
            break;
        }
    }
    links
}

fn html_text(text: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    decode_entities(&out.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

#[derive(Debug, serde::Serialize)]
struct NetworkError {
    code: String,
    message: String,
    warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_private_ips_by_default() {
        assert!(is_private_destination("127.0.0.1".parse().unwrap()));
        assert!(is_private_destination("10.0.0.1".parse().unwrap()));
        assert!(!is_private_destination("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn extracts_simple_html_title_links_and_text() {
        let html = r#"<html><head><title>A &amp; B</title></head><body><a href="https://example.com/a">x</a><p>Hello <b>world</b></p></body></html>"#;
        assert_eq!(html_title(html), Some("A & B".into()));
        assert_eq!(html_links(html).len(), 1);
        assert!(html_text(html).contains("Hello world"));
    }
}
