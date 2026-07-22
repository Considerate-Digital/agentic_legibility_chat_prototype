---
name: change_state
description: "Move the conversation to a different workflow state. Use this whenever the conversation has clearly progressed past the current state's purpose (e.g. a plan is needed, or the user has approved a plan and wants to act on it)."
parameters:
  - name: target_state
    type: string
    description: "The state to transition to. Must be one of the current state's valid transitions, listed in that state's system prompt frontmatter."
  - name: reason
    type: string
    description: "One short sentence explaining why this state is now appropriate — 'User has named three benefits and asked for an action plan', 'User confirmed the plan is ready to execute'. Helps the user understand the assistant's reasoning."
---

## Extended Description

Transitions the workflow to a new state. Each state has a list of valid next states declared in its own definition; transitions outside that list are rejected. There are three states: `Advice`, `Plan`, and `Execute`.

Only call this tool when actually moving to a different state. Staying in the current state for another turn needs no tool call — just keep responding there.

## When to call this tool

- **From `Advice`** (the entry state) — if a step-by-step plan would help, transition to `Plan`. If the user wants to act on something right now, transition to `Execute`.
- **From `Plan`** — once the user is ready to act ("go" / "let's do it" / "start"), transition to `Execute`. If they realise they just need a quick answer, transition back to `Advice`.
- **From `Execute`** — if the plan itself needs revising, transition to `Plan`. If the user's question has shifted away from execution entirely, transition to `Advice`.

## When NOT to call this tool — common mistakes to avoid

- **Do not demote from `Plan` to `Advice` just because the user asked for specifics** ("show me the endpoints for this service", "what does getPlan return?", "what does the auth service do?"). If the question is about a service, plan, or endpoint you have already identified, use the tools available in `Plan` (`get_service`, `get_endpoint`, `get_plan`, `list_endpoints`, `search_specs`, etc.) to find the answer — see `Plan`'s "Stay in Plan for follow-up detail questions" section. The same applies to `Execute` and `Advice`: each has its own full set of spec tools, so a detail question about something already identified never needs a transition.
- **Do not skip ahead** without completing the current state's purpose. Each state's system prompt describes what needs to happen there before transitioning out.
- **Do not oscillate** between two states every turn. If you transitioned to `Plan` last turn, don't jump back to `Advice` this turn unless the user has genuinely changed what they need.

## Errors

- "X is not a recognised state" — the target state name doesn't match `Advice`, `Plan`, or `Execute`.
- "cannot transition from A to B" — the transition isn't in the current state's `valid_transitions`. Pick a state from the list shown in the error.

## Example

User: "I think I understand my options now. What should I do first?"

Call:

```json
{ "target_state": "Plan", "reason": "User has enough information; ready to draft an action plan" }
```
