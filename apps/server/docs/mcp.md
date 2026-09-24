# Shared login and MCP

Gday Meetings Server owns user management. Administrators create and manage users in Payload Admin. The identity layer provides standard OpenID Connect/OAuth, using those canonical accounts. Optional external providers are linked sign-in methods; they do not grant application permissions or create administrators.

An OAuth-capable Streamable HTTP MCP client connects to `https://gdaymeetings.com/mcp` for the production domain, or `/mcp` on its configured self-hosted origin. Discovery starts browser login and consent. The single `mcp:read` permission grants `search_meetings`, which applies the authorizing user's Payload collection access rules. The client needs no local MCP process or deployment-wide secret.

The Rust gday-meetings client uses the same login but requests `meetings:read` and `meetings:write` for the platform API. These permissions allow uploads and submission of server-managed transcription tasks. An MCP search token cannot upload files or start processing.

The authorization issuer is `SERVER_URL/api/auth`. OpenID Connect metadata is available at `/api/auth/.well-known/openid-configuration`. MCP resource metadata is at `/.well-known/oauth-protected-resource/mcp`. Clients use discovery rather than constructing authorization/token URLs themselves. Public clients use PKCE S256 and exact resource audiences; refresh tokens must be saved after rotation.

For the owned production domain, configure DNS/TLS and set `SERVER_URL=https://gdaymeetings.com`. Self-hosted installations use their own stable public HTTPS `SERVER_URL`. Preserve Host and Authorization through the reverse proxy, and finish first-admin setup before exposing the service. Use HTTP only for loopback development. Back up persistent authentication data together with Payload data, audio, and deployment secrets.

Client access requires OAuth across MCP and the platform API; deployment-wide API/MCP keys are not supported. The server provides task-scoped capabilities directly to workers for output callbacks. Desktop clients never receive callback credentials.

Access tokens expire after five minutes. Browser sign-out invalidates its session-bound access tokens immediately; disabling/deleting the canonical account immediately denies resource access. Revoking only a refresh token prevents renewal but an already issued access token may remain valid until its five-minute expiry. Desktop sign-out clears its private credential file and attempts remote refresh revocation.

SQLite authentication state is stored in `DATA_DIR/auth.db` alongside the Payload database and audio. Postgres uses prefixed `gday_auth_*` tables in the same database by default. `AUTH_DATABASE_URI` may override the identity database. The maintained provider migrates its schema on startup; coordinate a single migrating instance and back up before upgrades.
