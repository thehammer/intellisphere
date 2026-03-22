# Hello World Example

Minimal IntelliSphere setup: one built-in tool, default configuration.

## Quick Start

1. Copy the environment file:
   ```bash
   cp ../../.env.example .env
   # Edit .env with your ANTHROPIC_API_KEY
   ```

2. Start the services:
   ```bash
   docker compose -f ../../docker-compose.yaml up
   ```

3. Test the chat endpoint:
   ```bash
   curl -X POST http://localhost:8080/v1/chat \
     -H "Content-Type: application/json" \
     -d '{
       "messages": [
         {"role": "user", "content": "Hello! Can you echo back the message \"testing 123\" using the intellisphere_echo tool?"}
       ]
     }'
   ```

4. Test the health endpoint:
   ```bash
   curl http://localhost:8080/health
   ```

## What This Demonstrates

- Sphere + Core running together via Docker Compose
- External HTTP API (`POST /v1/chat`)
- Input sanitization filter (control characters stripped)
- Built-in `intellisphere_echo` tool (validates the full tool call loop)
- Health endpoint aggregating Sphere + Core status

## Using Mock LLM (No API Key)

For testing without an API key, use the dev compose override:

```bash
docker compose -f ../../docker-compose.yaml -f ../../docker-compose.dev.yaml up
```

This uses the mock LLM adapter which returns deterministic responses.
