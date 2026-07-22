---
name: ui_input
description: "Pause the current task and ask the user to provide a typed value via a form in the UI. Use this when you need a piece of information that is not available in context and cannot be retrieved from an API."
parameters:
  - name: input_type
    type: string
    description: "The kind of input to collect: 'text' (free text), 'number' (numeric), 'date' (calendar picker), 'email' (validated email address), or 'select' (one choice from a fixed list)."
  - name: name
    type: string
    description: "A short identifier for the field — used internally to label the value in the response, e.g. 'national_insurance_number', 'date_of_birth', 'preferred_contact'."
  - name: description
    type: string
    description: "The label shown to the user above the input field. Write it as a clear, plain-English prompt — e.g. 'Enter your National Insurance number', 'What is your date of birth?', 'How would you prefer to be contacted?'."
  - name: options
    type: array
    description: "Required when input_type is 'select': the list of choices the user can pick from — e.g. ['Email', 'Phone', 'Post']. Ignored for all other input types."
    required: false
---

## Extended Description

Suspends the task loop and renders an input form directly in the chat UI. The task resumes only after the user submits a value. The submitted value is returned as the tool result and is added to the conversation context so subsequent tool calls and reasoning can reference it.

The form is rendered inline (not as a blocking modal) so the user can still see the conversation history above it.

## When to call this tool

Call `ui_input` when:

- You need a **piece of information the user must supply** — a National Insurance number, date of birth, vehicle registration, postcode, or similar — that is not already in context.
- The user needs to make an **explicit choice** between a fixed set of options (e.g. which address to use, which service to proceed with) — use `input_type: "select"` with the `options` list.
- You are collecting a value that will be sent as part of a `fetch` request body in the next step.

## When NOT to call this tool

Do **not** call `ui_input`:

- When the information is **already in the conversation** — extract it from context instead of asking again.
- When the information **can be retrieved from an API** — call `fetch` for the relevant endpoint rather than asking the user.
- For **confirmations** ("Are you sure?") — phrase those as a plain message and wait for the user's next chat turn; `ui_input` is for data collection, not yes/no gates.
- More than once in a row without using the collected value — batch your questions where possible.

## Example

Collecting a National Insurance number before a benefits eligibility check:

```json
{
  "input_type": "text",
  "name": "national_insurance_number",
  "description": "Enter your National Insurance number (e.g. AB 12 34 56 C)"
}
```

Asking the user to choose a contact method:

```json
{
  "input_type": "select",
  "name": "contact_preference",
  "description": "How would you prefer to receive updates about your application?",
  "options": ["Email", "Phone", "Post"]
}
```
