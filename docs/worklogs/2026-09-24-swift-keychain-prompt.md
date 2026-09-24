---
date: 2026-09-24
title: Explain credentials in native Keychain prompts
status: implemented-automated-checks-passed
---

## Problem

The system Keychain dialog identified saved credentials only as `com.gdaymeetings.macos`, leaving their purpose unclear.

## Implemented solution

`SecurityCredentialStorage` now names saved items `Gday Meetings — AI API key`, `Gday Meetings — transcription API key`, or `Gday Meetings — server sign-in tokens`. New items receive the descriptive name in both their label and their native access descriptor. Existing items receive it when next saved; only secret-read ACL descriptions change. Trusted applications, authorization tags, prompt flags, and non-prompt/partition metadata remain intact. No credential values or service/account identifiers change as part of the naming policy.

## Reasoning

The pictured file-based Keychain dialog belongs to macOS; the app cannot replace its complete wording with an arbitrary explanatory paragraph. Apple's Security SDK explicitly defines the access descriptor as the name displayed in security dialogs. A short purpose-specific name supplies the requested context inside the native prompt, following [HIG Privacy](https://developer.apple.com/design/human-interface-guidelines/privacy). Existing items may show their old name before macOS authorizes the next save/description update. Read-only startup does not rename them. No extra app-owned startup alert was added.

## Technical debt

File-based Keychain prompt customization requires deprecated SecAccess/SecACL APIs. Accepted to keep existing credentials and trust policies compatible with this ad-hoc-signed client; builds report the deprecation warnings. Future migration to the data-protection Keychain with a stable signing/entitlement strategy can use LAContext.localizedReason and retire this compatibility code after validating credential migration. Startup still eagerly reads credentials; deferring online access remains a separate UX improvement. An existing value can save successfully before its description update fails; the error explicitly reports that case, and no credential is removed to recover from it.

## Validation

53 tests in 17 suites passed. Added an in-memory Security ACL regression covering preserved application lists (including an empty list), authorization tags, prompt flags, untouched non-prompt descriptions, and idempotent renaming. Tests never read real user credentials or open a keychain. Native dialog rendering/wording and existing-item renaming with user authorization remain untested live; do not claim an already displayed prompt changed.

Release packaging, plist and ad-hoc signature verification passed. The updated app relaunched. A subsequent Settings save encountered a native Keychain prompt that the automation tool is forbidden to inspect or operate; user handling and live wording confirmation are pending.
