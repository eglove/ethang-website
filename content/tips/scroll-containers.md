---
title: "Easy Sticky Header/Footer"
description: "A very simple approach to creating both a sticky header and sticky footer using grid."
---

A very simple approach to creating both a "sticky header" and "sticky footer" using grid.

## CSS

```css
    .container {
      display: grid;
      height: 100vb;
      grid-template-rows: auto 1fr auto;
    }

    .content {
      overflow: auto;
    }
```

## HTML

```html
    <div class="container">
      <header>Header</header>
      <main class="content">Content</main>
      <footer>Footer</footer>
    </div>
```

## Tailwind

```html
    <div class="grid h-[100vb] grid-rows-[auto_1fr_auto]">
      <header>Header</header>
      <main class="overflow-auto">Content</main>
      <footer>Footer</footer>
    </div>
```

## Demo

The demo below is plain markup. The header and footer stay put because the
middle row is `1fr` and scrolls; there is no script involved.

<div class="tip-demo">
  <h3 class="tip-demo__bar">Header</h3>
  <section class="tip-demo__scroll" tabindex="0" aria-label="Scroll container demo">
    <p>Peter Piper picked a peck of pickled peppers.</p>
    <p>A peck of pickled peppers Peter Piper picked.</p>
    <p>If Peter Piper picked a peck of pickled peppers,</p>
    <p>Where&rsquo;s the peck of pickled peppers Peter Piper picked?</p>
    <p>Peter Piper picked a peck of pickled peppers.</p>
    <p>A peck of pickled peppers Peter Piper picked.</p>
    <p>If Peter Piper picked a peck of pickled peppers,</p>
    <p>Where&rsquo;s the peck of pickled peppers Peter Piper picked?</p>
    <p>Peter Piper picked a peck of pickled peppers.</p>
    <p>A peck of pickled peppers Peter Piper picked.</p>
    <p>If Peter Piper picked a peck of pickled peppers,</p>
    <p>Where&rsquo;s the peck of pickled peppers Peter Piper picked?</p>
    <p>Peter Piper picked a peck of pickled peppers.</p>
    <p>A peck of pickled peppers Peter Piper picked.</p>
    <p>If Peter Piper picked a peck of pickled peppers,</p>
    <p>Where&rsquo;s the peck of pickled peppers Peter Piper picked?</p>
    <p>Peter Piper picked a peck of pickled peppers.</p>
    <p>A peck of pickled peppers Peter Piper picked.</p>
    <p>If Peter Piper picked a peck of pickled peppers,</p>
    <p>Where&rsquo;s the peck of pickled peppers Peter Piper picked?</p>
  </section>
  <div class="tip-demo__foot">
    <p>Footer</p>
  </div>
</div>
