# Chat Feature — End-to-End Testing

## Prerequisites

1. Server running with `--web-ui` flag
2. At least one session with a transcript available
3. An OpenRouter API key (or any OpenAI-compatible API)

## Setup

### Configure LLM API key and model

```bash
curl -X PUT http://127.0.0.1:33487/api/settings \
  -H 'Content-Type: application/json' \
  -d '{
    "llm_api_key": "sk-or-v1-YOUR_KEY_HERE",
    "llm_model": "openai/gpt-oss-120b",
    "llm_host": "https://openrouter.ai/api/v1"
  }'
```

Verify:
```bash
curl http://127.0.0.1:33487/api/settings | jq '.llm_api_key_set, .llm_model, .llm_host'
# Expected: true, "openai/gpt-oss-120b", "https://openrouter.ai/api/v1"
```

The API key is stored in `{data_dir}/secrets.json` (0600 permissions), never returned in API responses.

## Test Cases

### 1. Create conversation

```bash
curl -X POST http://127.0.0.1:33487/api/conversations \
  -H 'Content-Type: application/json' \
  -d '{}'
```

Expected: `201` with `{ id, title, created_at, updated_at, messages: [] }`

### 2. List conversations

```bash
curl http://127.0.0.1:33487/api/conversations
```

Expected: `{ conversations: [{ id, title, message_count, last_message_preview, size_bytes, ... }] }`

### 3. Send plain message (no context)

```bash
CONV_ID="<id from step 1>"
curl -N -X POST "http://127.0.0.1:33487/api/conversations/${CONV_ID}/messages" \
  -H 'Content-Type: application/json' \
  -d '{"content": "Hello! Reply in one sentence.", "mentions": []}'
```

Expected SSE stream:
```
event: delta
data: {"content":"..."}
...
event: done
data: {"message_id":"msg_..."}
```

### 4. Send message with session context (@ mention)

Find a session with a transcript:
```bash
curl http://127.0.0.1:33487/api/sessions?limit=5 | jq '.sessions[] | {id, name, transcript_available}'
```

Send with a session mention:
```bash
curl -N -X POST "http://127.0.0.1:33487/api/conversations/${CONV_ID}/messages" \
  -H 'Content-Type: application/json' \
  -d '{
    "content": "What was discussed in this meeting?",
    "mentions": [{"kind": "session", "id": "SESSION_ID", "label": "Session Name"}]
  }'
```

Expected SSE stream:
```
event: context_loaded
data: {"chunk_count": N, "session_count": 1}

event: delta
data: {"content":"..."}
...
event: done
data: {"message_id":"msg_..."}
```

### 5. Follow-up message (reuses context)

```bash
curl -N -X POST "http://127.0.0.1:33487/api/conversations/${CONV_ID}/messages" \
  -H 'Content-Type: application/json' \
  -d '{"content": "What action items were mentioned?", "mentions": []}'
```

Expected: `context_loaded` event (reuses last context), then delta stream with relevant answer.

### 6. Get conversation (verify messages saved)

```bash
curl "http://127.0.0.1:33487/api/conversations/${CONV_ID}" | jq '.messages | length'
```

Expected: All user + assistant + context_result messages present. Context chunks have `words` array stripped.

### 7. Delete conversation

```bash
curl -X DELETE "http://127.0.0.1:33487/api/conversations/${CONV_ID}"
```

Expected: `204 No Content`

### 8. List models

```bash
curl http://127.0.0.1:33487/api/llm/models | jq '.data | length'
```

Expected: Array of available models from the configured LLM host.

## Web UI Testing

1. Open `http://127.0.0.1:33487` in browser
2. Click the blue chat bubble (bottom-right corner)
3. Chat panel opens — type a message, press Enter
4. Verify streaming response appears with typing cursor
5. Type `@` in the input — verify mention popup appears with Tags, People, Sessions
6. Select a session mention, send message — verify "Context loaded" indicator appears
7. Send a follow-up question — verify it uses the previous context
8. Drag the bubble to different corners — verify snap animation
9. Go to Settings > Services > AI Chat — verify host, model, API key fields
10. Go to Settings > Conversations — verify list with file sizes, delete works
11. Test on mobile viewport (< 768px) — verify near-fullscreen panel
12. Test dark mode — verify all elements styled correctly

## Troubleshooting

- **"LLM API key not configured"**: Set the key via `PUT /api/settings` with `llm_api_key`
- **Streaming hangs**: Check server logs for connection errors to the LLM host
- **Context not loading**: Ensure the referenced session has a `transcript.json` file
- **Model not found**: Verify the model ID matches what the LLM host supports (use `GET /api/llm/models`)
