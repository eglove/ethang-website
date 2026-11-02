---
title: "scrollbar-gutter"
description: "Avoid content layout shifts when moving from content without a scroll to content with a scroll."
---

Avoid content layout shifts when moving from content without a scroll to content with a scroll.
[MDN](https://developer.mozilla.org/en-US/docs/Web/CSS/scrollbar-gutter) [Spec](https://drafts.csswg.org/css-overflow/#scrollbar-gutter-property)

## CSS

```css
html {
  scrollbar-gutter: stable both-edges;
}
```

## Demo

The two containers hold identical text. The first reserves the scrollbar's
gutter, so the content does not shift when the scrollbar appears; the second
does not, so it does.

<div class="tip-demo-pair">
  <div class="tip-demo-panel">
    <div class="tip-demo-panel__label">With scrollbar-gutter</div>
    <section class="tip-demo-panel__scroll tip-demo-panel__scroll--stable" tabindex="0" aria-label="With scrollbar-gutter demo">
      <p>This container has <code>scrollbar-gutter: stable</code> applied.</p>
      <p>When content exceeds the container height, a scrollbar appears without shifting the content.</p>
    </section>
  </div>
  <div class="tip-demo-panel">
    <div class="tip-demo-panel__label">Without scrollbar-gutter</div>
    <section class="tip-demo-panel__scroll tip-demo-panel__scroll--auto" tabindex="0" aria-label="Without scrollbar-gutter demo">
      <p>This container does not have scrollbar-gutter applied.</p>
      <p>When content exceeds the container height, a scrollbar appears and shifts the content.</p>
    </section>
  </div>
</div>
