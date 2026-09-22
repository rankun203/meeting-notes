---
date: 2026-09-22
title: Gday Meetings user sign-in and server-owned transcription
status: complete
---

## Problem

The desktop daemon required shared file-drop and RunPod keys and submitted GPU jobs
itself. Users need to sign in to their Gday account and let Gday own transcription,
while retaining durable recovery when the local app closes or loses a response.

## Implemented solution

- Added a Services sign-in panel and local OAuth routes, using Gday's OIDC discovery,
  native public client registration, authorization code PKCE S256, one-use expiring
  browser-bound state, and `openidconnect` signature/issuer/audience/nonce validation.
- Access/rotating refresh tokens remain in a separate atomically written 0600 daemon
  file. Refresh is serialized, logout revokes remotely when available and clears local
  state, and API responses expose identity/connection status without tokens.
- Signed-in transcription uploads audio under the user's grant, asks Gday to execute a
  task, and polls its persisted output. No local RunPod key is needed. Exact upload URLs,
  options and idempotency key are persisted before submission; retries/restarts reuse
  them after a lost acknowledgement. Existing direct RunPod mode remains when signed out.
- Startup resumes user tasks with their owning Gday origin and refreshed user credentials.

## Reasoning

Use the established OpenID Connect library for token validation instead of decoding
unverified JWT claims. The provider is discovered rather than coupled to Better Auth's
route internals. Same-origin login/logout and an HttpOnly cookie defend local auth routes
within the daemon's otherwise permissive CORS setup. Inputs must be replayed exactly,
since re-uploading creates new URLs and changes the platform's idempotency fingerprint.

## Technical debt

Private refresh-token storage uses the existing daemon filesystem model rather than an
OS credential vault. Accepted to support the daemon's portable data directory; access is
limited to its OS owner (0600). A future shared credential-store abstraction can migrate
these tokens to Keychain/Secret Service while preserving portable opt-in storage.

Dynamic client registration currently creates a new public client per login rather than
reusing registrations. This keeps callback/issuer pairing explicit; repeated logins retain
provider-side client records. Cache registrations by issuer and callback once lifecycle
cleanup and client revocation policy are defined.

## Notes

Validation: 37 Rust library tests passed; targeted OIDC tests passed after adding real
local HTTP login/callback coverage; both changed frontend modules pass Node syntax
checks. The local signed OIDC provider test covers PKCE enforcement, wrong-browser, signed
nonce-mismatch and replay rejection, token refresh concurrency and private persistence, and real HTTP
upload/task submission with lost acknowledgement, daemon restart and transcript creation.
No production accounts, tokens or RunPod calls are used. The RSA key under tests/fixtures
is generated solely for this local test provider and has no production use.

Follow-up validation against the actual maintained Gday Better Auth provider passed:
native registration, canonical-user password login, consent, real EdDSA ID-token
verification, initial and refreshed platform access, and logout. This found a Gday
root-discovery handler returning `{}`; the platform fix now forwards the provider
Response. Added subject-checked standard UserInfo fallback when email is absent from
an ID token. The optional cross-repository test and disposable fixture are documented
in `docs/gday-meetings.md`; they use only synthetic identities and temporary databases.


Reproduce the maintained-provider check with the disposable fixture command in the
setup guide, then run `GDAY_TEST_ORIGIN=http://127.0.0.1:PORT cargo test
maintained_gday_provider_contract -- --ignored --nocapture`. Final result: 37 default
tests passed (one opt-in test ignored), plus the opt-in maintained-provider test passed.
