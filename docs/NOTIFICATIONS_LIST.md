# notifications.list

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Return Windows notification inspection status and any available provider-backed notifications.

## Inputs

- `max_items`: optional item limit.

## Notes

On Windows, this uses `Windows.UI.Notifications.Management.UserNotificationListener`. If notification-listener access is not granted, the tool still returns `ok: true`, `provider_enabled: true`, an empty `notifications` array, and an `access` object explaining whether the state is `allowed`, `denied`, or `unspecified`.

The tool does not automatically request notification-listener consent. Grant access in Windows privacy settings, then call the tool again to read available toast notifications.
