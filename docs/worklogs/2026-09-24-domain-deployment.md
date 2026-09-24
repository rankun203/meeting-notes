---
date: 2026-09-24
scope: server-worker-deployment-domain
status: complete
owner: native-services-agent
---

## Problem

The owned production domain is now `gdaymeetings.com`. Active server, MCP, worker and deployment examples still used generic public-host placeholders and did not explain the production origin or its effect on OAuth.

## Implemented solution

Updated the server environment example, README, API/MCP documentation, worker audio URL examples, central deployment guide and client/server integration guide to use `https://gdaymeetings.com` as the production origin. MCP uses `/mcp`. Worker and private server subdomains are explicitly labeled examples requiring separate provisioning. Added deployment-origin migration notes for OAuth issuer/audience, client reconnection, outstanding tasks and signed audio URLs.

## Reasoning

Retained localhost runtime and Compose defaults for development and self-hosting; choosing the owned domain in documentation must not silently redirect local credentials or callbacks to a public server. Preserved independent third-party identity-provider examples, RunPod/Hugging Face links, test-only reserved hosts, real GitHub/container identifiers and historical release/worklog records. The audit found generic public-host examples rather than an active hardcoded legacy production deployment in the owned server/worker scope.

No DNS, TLS, hosting or external deployment was changed or assumed operational.

## Technical debt

None introduced or retained by these documentation/example changes. An actual deployment cutover remains separate operational work: provision DNS/TLS, configure `SERVER_URL`, reconnect OAuth clients and coordinate existing task/audio origins before removing the previous origin.

## Validation

Inspected the complete seven-file documentation/example diff. Scoped `git diff --check` passes. A follow-up search found no remaining old public-server/MCP/worker example hosts in active owned documentation. No runtime source or dependency changed, so server/worker execution tests were not needed. Git index and commits remain coordinated by the root agent.
