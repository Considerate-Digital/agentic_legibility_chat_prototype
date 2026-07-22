---
name: ActionChecklist
description: "Show when the user has a set of concrete actions to complete, especially during Plan or Execute."
relevant_states: [Plan, Execute]
---

Generate a checklist of the concrete next actions the user needs to take, based on the conversation.

Use **only** these classes from `../../service_creator/src/app.css` — do not introduce any colours, borders, or other styling of your own:

- `.card` — outer wrapper (white surface, grey border, `--radius` corners)
- `.card-head` — header row
- `.card-name` — header text (bold, grey-900)
- `.card-desc` — body container
- `.row-desc` — each checklist row label
- `.table-row.selected` — applied via `class="selected"` on a row to mark it completed (gives it a green left border and `--green-bg` background, reusing the table primitive)

Mark items as completed by adding `class="selected"` to the `<label>`. Use HTML checkboxes (`<input type="checkbox" disabled>`) so they appear as status indicators rather than interactive inputs.

Include 3–6 items. Start with any already completed. End with the most important next step.

Template:

```html
<div class="card">
  <div class="card-head">
    <span class="card-name">Next steps</span>
  </div>
  <div class="card-desc">
    <label class="row-desc selected"><input type="checkbox" disabled checked> Confirm identity with passport</label>
    <label class="row-desc selected"><input type="checkbox" disabled checked> Submit share code to DWP</label>
    <label class="row-desc"><input type="checkbox" disabled> Wait 5 working days for decision letter</label>
  </div>
</div>
```