---
name: CaseProgress
description: "Show the user's progress through a multi-step process or plan, especially during Plan or Execute."
relevant_states: [Plan, Execute]
---

Generate a card showing the user's progress through their case or plan. Use a row of stage cards inside `.card-grid`; completed stages get `class="selected"` (which applies the green border via the existing `.card.selected` rule).

Use **only** these classes from `../../service_creator/src/app.css` — do not introduce any colours, borders, or other styling of your own:

- `.card-grid` — the 3-column grid wrapper
- `.card` — each stage card (white surface, grey border, `--radius` corners)
- `.card.selected` — applied to completed stages (green border)
- `.card-head` — header row of each stage
- `.card-name` — stage title (bold, grey-900)
- `.card-desc` — stage body text
- `.card-foot` — footer container holding the next-action line
- `.card-step` — the next-action text (uses the existing `--green` colour via the app)
- `.card-path` — monospace path string when a next step involves an endpoint path
- `.row-desc` — the trailing "next action" paragraph beneath the grid

Include 3–5 stages. Set each stage's `class="selected"` if it is complete. Add one clear next-action line beneath the grid using `.card-foot` / `.card-step` (or, when no grid is appropriate, `.row-desc`).

Template:

```html
<div class="card-grid">
  <div class="card selected">
    <div class="card-head"><span class="card-name">1. Verify identity</span></div>
    <div class="card-desc">Done — confirmed via passport.</div>
  </div>
  <div class="card">
    <div class="card-head"><span class="card-name">2. Submit share code</span></div>
    <div class="card-desc">Awaiting your submission.</div>
  </div>
  <div class="card">
    <div class="card-head"><span class="card-name">3. Await decision</span></div>
    <div class="card-desc">~5 working days from submission.</div>
  </div>
</div>
<p class="row-desc">Next: submit your share code at <code class="card-path">/v1/dwp/share-code</code>.</p>
```