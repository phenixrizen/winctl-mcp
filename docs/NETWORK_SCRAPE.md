# network.scrape

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Fetch and extract basic title, link, and text content from an HTTP/HTTPS page under network policy.

## Inputs

- `url`: HTTP or HTTPS URL.
- `timeout_ms`: optional timeout, capped by the server.
- `max_bytes`: optional response byte limit, capped by the server.
- `follow_redirects`: follow up to five redirects when true.
- `include_links`: include extracted `href` values.
- `include_text`: include tag-stripped text.

## Notes

This is a bounded diagnostic scrape, not browser DOM extraction. It uses the same private-network, scheme, redirect, timeout, and size policy as `network.fetch`.
