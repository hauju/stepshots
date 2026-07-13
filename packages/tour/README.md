# @stepshots/tour

Framework-agnostic **live guided-tour player** for Stepshots recordings. It "lights
the way" on your real app — spotlighting the next element to click and anchoring a
callout to it — so a recorded flow becomes an interactive walkthrough a user
performs on their own account, not just a passive screenshot demo.

- **No framework, no runtime dependency.** Operates on the rendered DOM, so it works
  on React, Vue, Svelte, Dioxus/WASM, or plain HTML. Renders in a closed-styled
  shadow DOM so host CSS can't leak in.
- **Survives SPA navigation & hydration.** Each step waits (MutationObserver) for its
  target to mount, then anchors — targets that appear after a route change or a
  loading gate are handled.
- **Advances on real interaction.** Capture-phase listeners are additive (no
  `preventDefault`), so the user's click/typing both advances the tour *and* does its
  normal thing.

The player needs only a **track** (plain JSON). Stepshots produces tracks from a
recording (record once → screenshot demo *and* live tour); you can also hand-author one.

## Install

```sh
bun add @stepshots/tour
```

## Programmatic use

```ts
import { startTour } from "@stepshots/tour";

const handle = startTour(
  {
    steps: [
      { selector: '[data-testid="new-project"]', title: "Create a project",
        body: "Start here.", advance: { type: "click" } },
      { selector: '#project-name', title: "Name it",
        body: "Type a name.", advance: { type: "input" } },
    ],
  },
  { theme: { accent: "#6366f1" }, onComplete: (why) => console.log(why) },
);
// handle.stop() to end early
```

## `<script>` embed (zero-config)

Load the IIFE build, register your tracks, and call `autoBoot`. It starts the tour
named by `?tour=<key>` and keeps it alive across SPA navigations.

```html
<script>window.__STEPSHOTS_TOURS = { "create-project": { steps: [/* ... */] } };</script>
<script src="https://unpkg.com/@stepshots/tour"></script>
<script>StepshotsTour.autoBoot();</script>
```

Opening `…/dashboard?tour=create-project` then runs the tour.

## API

- `startTour(track, options?) → { stop() }` — render a track now.
- `autoBoot(options?) → handle | null` — URL-param-driven entry for the embed; `tracks`
  defaults to `window.__STEPSHOTS_TOURS`.
- Options: `theme` (`accent`, `dim`, `cardBg`, `cardFg`, `cardMuted`), `skipLabel`,
  `waitTimeoutMs`, `onComplete(reason)`.
