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
# or: npm install @stepshots/tour
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

## First-run auto-start

Offer a tour automatically the first time a user reaches a given state — e.g. an
empty "no projects yet" screen. Render a marker element only in that state;
`autoBoot` watches for it, shows a consent card, and starts the tour if the user
accepts. Either way the offer is remembered (localStorage), so it never auto-starts
again.

```ts
StepshotsTour.autoBoot({
  firstRun: {
    key: "create-project",
    marker: '[data-stepshots-firstrun="create-project"]',
    intro: { title: "Welcome 👋", body: "Want a quick tour of creating your first project?" },
  },
});
```

Omit `intro` to start the tour outright instead of asking first.

## Show-me triggers (FAQ / help center)

Any element carrying `data-stepshots-tour-trigger="<key>"` starts that track on
click — turn FAQ answers, help menus, and empty states into launchers that show
instead of tell:

```html
<details>
  <summary>How do I create a project?</summary>
  <button data-stepshots-tour-trigger="create-project">Show me</button>
</details>
```

Bound once by `autoBoot` via event delegation, so triggers rendered later (SPA
views, accordions) work without re-binding. A click always starts fresh at step 0.

When the flow starts on a different page, add `data-stepshots-tour-url`: the
click stashes the tour and navigates there; the destination page's `autoBoot`
starts it on arrival (both pages need the registry + `autoBoot`):

```html
<button
  data-stepshots-tour-trigger="invite-teammate"
  data-stepshots-tour-url="/settings/team"
>Show me</button>
```

See `examples/faq-show-me.html` in the repo for a complete page.

## Onboarding checklist

Bundle your activation tours into a persistent "Getting started · 2/5" launcher.
It expands into a checklist; clicking an item runs its tour, and the item is
checked off when the tour completes. Progress persists per browser
(localStorage), and once everything is done the panel offers "Hide checklist"
(also remembered).

```ts
StepshotsTour.createChecklist({
  title: "Getting started",
  items: [
    { tour: "create-project", label: "Create your first project" },
    { tour: "invite-teammate", label: "Invite a teammate", url: "/settings/team" },
  ],
  onAllDone: () => confetti(),
});
```

Give an item a `url` when its flow starts on a different page: clicking
navigates there and the checklist on the destination page starts the tour on
arrival — the same jump mechanism as `data-stepshots-tour-url`. Mount the
checklist (and the registry) on every page and cross-page items and mid-tour
reloads resume seamlessly.

The returned handle has `open()` / `close()`, `destroy()`, and
`markDone(tour)` — mark an item done without running its tour, for when the
user already did the thing organically.

## API

- `startTour(track, options?) → { stop() }` — render a track now.
- `autoBoot(options?) → handle | null` — URL-param-driven entry for the embed; `tracks`
  defaults to `window.__STEPSHOTS_TOURS`. Extra options: `param` (query param selecting
  the track, default `"tour"`), `storageKey` (sessionStorage key that keeps the run
  alive across SPA navigations), `firstRun` (auto-start on a marker, see above).
- `showIntro(intro, options, onChoice)` — render the consent card standalone;
  `onChoice(accepted)` tells you the user's pick.
- `createChecklist(options) → { open(), close(), markDone(tour), destroy() }` —
  the onboarding checklist (see above). Extra options: `items`, `title`,
  `storageKey`, `tracks` (defaults to `window.__STEPSHOTS_TOURS`), `position`
  (`"bottom-right"` | `"bottom-left"`), `onAllDone`; tour options below are
  forwarded to every launched run.
- Options: `theme` (`accent`, `dim`, `cardBg`, `cardFg`, `cardMuted`), `skipLabel`,
  `waitTimeoutMs`, `inputSettleMs` (quiet time before an input step advances, default
  1200ms), `onComplete(reason)`, `onEvent(event)` (lifecycle analytics:
  `start` / `step` / `done` / `skip` / `lost`), `resumeKey` (sessionStorage key so a
  full page reload resumes mid-tour), `badge` (`{ label, href }` attribution link in
  the card footer).
