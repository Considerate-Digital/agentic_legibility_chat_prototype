---
name: report_service_step
description: "Signal which step of a service schema you are currently executing. Call this before making the API request for each step so the UI can track progress through the service."
parameters:
  - name: service_id
    type: string
    description: "The UUID of the service being executed — taken from the 'id' field in the service's frontmatter."
  - name: step_number
    type: integer
    description: "The 1-based index of the step you are about to execute (or have just completed)."
  - name: status
    type: string
    description: "The step's current status: 'starting' (before the API call), 'completed' (after a successful response), 'skipped' (step not applicable to this user), or 'failed' (API error or bad response)."
---

## Extended Description

Emits a progress event to the UI indicating which numbered step of a service schema is being executed and what its status is. The host reads the service file from `live_resources/services/`, resolves the step to its endpoint name, department, and required/optional status, and displays that context alongside the status update.

This tool does not perform any API call itself — it is purely a signalling mechanism. Always pair it with a `fetch` call that does the actual work.

## When to call this tool

Call `report_service_step` **twice per step**:

1. **Before** the `fetch` call, with `"status": "starting"` — so the user can see which endpoint is about to be contacted.
2. **After** the `fetch` call, with `"status": "completed"`, `"status": "skipped"`, or `"status": "failed"` — to close out the step in the UI.

This gives a real-time trace of service execution that the user can follow without reading raw API responses.

## When NOT to call this tool

Do **not** call `report_service_step`:

- Outside of an active service execution — it requires a valid `service_id` from `live_resources/services/`.
- For ad-hoc `fetch` calls that are not part of a service schema step (exploratory lookups, token refresh, etc.).
- More than once per status per step — duplicate events are ignored by the UI but waste tokens.

## Calling sequence for a single step

```
report_service_step { service_id: "...", step_number: 2, status: "starting" }
fetch { url: "http://localhost:8127/dvla/v1/share-codes", method: "GET" }
report_service_step { service_id: "...", step_number: 2, status: "completed" }
```

## Example

Starting step 1 of a vehicle check service:

```json
{ "service_id": "a3f9e12b-4c71-4d88-b9a2-7e3f1c0d5e2a", "step_number": 1, "status": "starting" }
```

Marking it complete after a successful API call:

```json
{ "service_id": "a3f9e12b-4c71-4d88-b9a2-7e3f1c0d5e2a", "step_number": 1, "status": "completed" }
```
