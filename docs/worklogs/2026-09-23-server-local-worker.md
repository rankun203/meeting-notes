---
date: 2026-09-23
title: Meeting Notes Server with local worker transport
status: implemented
---

Problem: The monorepo server needs to execute transcription through a local standalone worker while retaining the existing RunPod deployment option and browser OAuth origin.

Implemented solution: The server component in apps/server is named @meeting-notes/server and displays Meeting Notes Server. TRANSCRIPTION_PROVIDER selects runpod (default) or local. Local mode requires LOCAL_WORKER_URL and LOCAL_WORKER_API_TOKEN; POST /run and GET /status/:id retain the established job protocol, reject redirects, and use the machine token only on calls to the configured worker. Local submissions include task UUID as Idempotency-Key. SERVER_INTERNAL_URL optionally rewrites the origin of validated owned audio capabilities and generated task callbacks for Docker networking. Public SERVER_URL, OAuth scopes/issuer, upload authorization and persisted collection identities are unchanged. Existing queue recovery and durable result persistence are shared by both transports.

Reasoning: Reusing one task execution state machine preserves recovery behavior and prevents independent local and remote job semantics. Explicit trusted origin configuration supports private container names without accepting internal URLs from user submissions. The worker machine credential is never included in callback data or exposed as user API authorization.

Technical debt: The existing runpodJobId database field now also stores local worker job IDs; the user explicitly deferred model changes. Drain pending jobs before changing providers/endpoints. Future task modeling should store provider identity per task and rename this field with a migration. Lost submission acknowledgement retains conservative SUBMISSION_UNKNOWN handling despite local worker idempotency support; future recovery can safely retry local submissions once provider identity is persisted.

Validation: Normal frozen pnpm installation completed. All 33 server tests passed, including local machine auth, internal URL rewriting, task-only callback capability, redirect rejection configuration, original RunPod execution, and invalid origin/credential configuration. TypeScript passed. Next production build passed, including page generation and route compilation.

Final display-name sweep: MCP server identity/tool description, consent identity label, sign-in error, and CMS audio guidance now use Meeting Notes Server. Regenerated Payload types retain matching descriptions. Existing auth/MCP tests passed (11/11), route type generation and TypeScript passed; protocol paths, scopes, session strategy and database identifiers remain unchanged.
