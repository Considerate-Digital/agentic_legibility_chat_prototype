---
name: fetch
description: "Make an HTTP request to a FLEX API endpoint and return the response body. Use this to read or write data from any endpoint listed in live_resources/endpoints/."
parameters:
  - name: url
    type: string
    description: "The full URL to request — e.g. 'http://localhost:8127/dvla/v1/driver-summary'. Must match an endpoint from live_resources/endpoints/."
  - name: method
    type: string
    description: "HTTP method: GET, POST, PATCH, DELETE. Defaults to GET if omitted."
    required: false
  - name: headers
    type: string
    description: "Optional JSON object of request headers — e.g. '{\"Content-Type\": \"application/json\", \"Authorization\": \"Bearer <token>\"}'. Must be a valid JSON string."
    required: false
  - name: body
    type: string
    description: "Optional request body string. For JSON payloads, serialise to a string and set Content-Type: application/json in headers."
    required: false
---

## Extended Description

Executes a single HTTP request against a FLEX API endpoint and returns the HTTP status code followed by the response body (truncated to ~4000 characters if very large). The response is returned verbatim — it is not parsed or summarised.

This is the primary tool for interacting with the data layer during service execution. Every step in a service schema maps to one or more `fetch` calls.

## When to call this tool

Call `fetch` when:

- You are executing a step in a service plan and need to **read data** from an API (GET requests for driver summaries, notifications, vehicle details, local authority records, etc.).
- You need to **write or mutate data** — creating share codes, patching notification preferences, unlinking a service, cancelling a code.
- You need to **retrieve identity or session tokens** for a service before making downstream calls.

Always consult `live_resources/endpoints/` for the correct URL, method, and expected request/response fields before calling this tool.

## When NOT to call this tool

Do **not** call `fetch`:

- To reach URLs outside the FLEX API (external websites, third-party services, GOV.UK) — this tool is for FLEX endpoints only. Use `search_specs`/`get_service` for the spec corpus, or general knowledge for GOV.UK content outside it.
- When you don't yet know which endpoint to call — use `report_service_step` to declare which step you are at, then call `fetch` for the actual request.
- If the response you need is already in the conversation context — avoid redundant calls.

## Example

Fetching the DVLA driver summary for the current user:

```json
{ "url": "http://localhost:8127/dvla/v1/driver-summary", "method": "GET" }
```

Creating a new share code:

```json
{
  "url": "http://localhost:8127/dvla/v1/share-code",
  "method": "POST",
  "headers": "{\"Content-Type\": \"application/json\"}",
  "body": "{\"shareCodeType\": \"DRIVING_LICENCE\"}"
}
```
