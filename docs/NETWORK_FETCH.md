# network.fetch

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Fetch an HTTP/HTTPS URL with timeout, response-size, redirect, and private-network guards.

## Inputs

- `url`: HTTP or HTTPS URL.
- `method`: optional `GET`, `HEAD`, or `POST`; default is `GET`.
- `headers`: optional request headers.
- `body`: optional POST body.
- `timeout_ms`: optional timeout, capped by the server.
- `max_bytes`: optional response byte limit, capped by the server.
- `follow_redirects`: follow up to five redirects when true.

## Policy

Private, loopback, link-local, multicast, unspecified, and local hostnames are blocked unless `WINCTL_ALLOW_PRIVATE_NETWORK=1` is set. URLs containing credentials are rejected.
