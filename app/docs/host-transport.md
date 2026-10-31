# Host transport integration

`PairingLink.parse(jsonText)` accepts a JSON object with string fields `endpoint`, `fingerprint`, and `secret`. The endpoint must be an HTTPS URI with a literal IPv4 or bracketed IPv6 address and explicit port, with no user info, path (including `/`), query, or fragment. The fingerprint is exactly 64 hexadecimal characters representing SHA-256 of the TLS certificate DER. The secret must be nonempty. Invalid input throws `FormatException` (including JSON parsing errors).

Call `await RemoteHost.pair(link)` to exchange the secret. The returned `RemoteHost` implements `HostTransport` and `EngineApi`; `clientId` identifies the paired client. Keep the host only in memory and discard the pairing link after pairing. `await host.disconnect()` closes the socket and clears the token; reconnect after that requires pairing again. Ordinary socket loss reconnects on the next engine operation to the same pinned endpoint. HTTP authorization failures report denied/revoked; socket failures propagate to the caller. Remote administration and `quitHost` are unavailable (`supportsAdministration` is false).

Inject a `HostTransport` into `ShellRecorderStore(host: host, names: ...)`. Use `recorderRequest(path, body: ..., timeout: ...)` for recorder API operations: an omitted body is GET, a supplied JSON map is POST. Local desktop hosts expose `supportsAdministration` and `quitHost`; their existing startup and discovery are unchanged. The transport owns credentials and request connections; callers must not construct URLs or authorization headers themselves.

Each remote HTTP request and WebSocket connection uses a direct TLS socket with an empty trust store. The certificate callback accepts only the supplied fingerprint, and the resulting peer certificate is checked again before the HTTP client can send request bytes. Redirects are disabled for HTTP requests.

The desktop recorder panel administers remote access through authenticated local `/api/remote/v1` requests. Enter a numeric LAN address before enabling and creating pairing JSON; the listener binds all IPv4 interfaces, while the entered address is used in the shared endpoint. Pairing secrets are held in memory and cleared on expiry or disable. The panel lists and revokes paired client IDs. Remote transports cannot access these controls.

## Headless smoke (macOS)

From the repository root, build `cargo build -p openfader-serve --locked`, then from `app` run `dart run tool/host_transport_smoke.dart ../target/debug/openfader-serve` with Flutter's Dart on PATH. The runner starts a separate desktop host with a temporary home, pairs over loopback TLS, checks pin rejection and one-use pairing, recorder access, a simulator mute event on a second client, revocation, disconnect, and listener disable. It removes the temporary home on exit. It does not exercise mobile UI or hardware. The Rust `remote_host_routes_and_lifecycle` test independently checks WebSocket closure on revoke and disable.
