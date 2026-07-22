---
name: KeyFacts
description: "Show when there are important numbers, deadlines, thresholds, or criteria the user should remember."
relevant_states: [Advice, Plan, Execute]
---

Extract 3–5 key facts, figures, deadlines, or criteria from the conversation that the user should remember. Each item is a labelled row: uppercase grey label on the left, value on the right.

Use **only** these classes from `../../service_creator/src/app.css` — do not introduce any colours, borders, or other styling of your own:

- `.card` — outer wrapper (white surface, grey border, `--radius` corners)
- `.card-head` — header row
- `.card-name` — header text (bold, grey-900)
- `.card-desc` — body container holding the rows
- `.meta-row` — each label/value row (flex row, aligns baseline)
- `.field-label` — the uppercase, small, grey-500 label on the left
- `.field` — the value surface on the right (page-bg surface, grey-900 text)
- `.grow` — applied to the label so it fills the available space before the value

Use `.grow` on the label so the value aligns to the right edge. Only include facts that are concrete and specific — not general advice.

Template:

```html
<div class="card">
  <div class="card-head">
    <span class="card-name">Key facts</span>
  </div>
  <div class="card-desc">
    <div class="meta-row">
      <span class="field-label grow">Weekly income limit</span>
      <span class="field">£184.00</span>
    </div>
    <div class="meta-row">
      <span class="field-label grow">Application deadline</span>
      <span class="field">30 days from today</span>
    </div>
    <div class="meta-row">
      <span class="field-label grow">Qualifying period</span>
      <span class="field">2 years UK residency</span>
    </div>
  </div>
</div>
```