// The tour "track" — the data contract between Stepshots (which produces it from
// a recording) and this player (which renders it on a live DOM). A track is plain
// JSON; the player has no dependency on Stepshots to run one.

/** How a step advances to the next one. */
export type Advance =
  /** Advance when the user clicks inside the target element. */
  | { type: "click" }
  /** Advance when the user types a non-empty value into the target element. */
  | { type: "input" }
  /** Advance when the user changes the target's value, e.g. picks a dropdown option. */
  | { type: "change" };

/**
 * Resilient anchors used when `selector` no longer matches the live DOM (UI
 * drift since the recording). Captured from the target element at record time.
 */
export interface TourFallback {
  /** Trimmed text content of the target at record time. */
  text?: string;
  /** aria-label of the target at record time. */
  aria?: string;
}

/** One step of a live tour: anchor to `selector`, show `title`/`body`, advance on interaction. */
export interface TourStep {
  /** CSS selector of the element to spotlight (re-resolved against the live DOM). */
  selector: string;
  /** Callout heading. */
  title: string;
  /** Callout body copy. */
  body: string;
  /** How the user advances past this step. */
  advance: Advance;
  /** Text/aria anchors the player falls back to when `selector` misses. */
  fallback?: TourFallback;
}

/** An ordered set of steps. Produced from a Stepshots recording, or hand-authored. */
export interface TourTrack {
  steps: TourStep[];
}

/** Visual theming for the overlay. All optional; sensible neutral defaults apply. */
export interface TourTheme {
  /** Spotlight ring color. */
  accent?: string;
  /** Backdrop dim color (the box-shadow that darkens everything but the target). */
  dim?: string;
  /** Callout card background. */
  cardBg?: string;
  /** Callout card foreground (title). */
  cardFg?: string;
  /** Callout card muted text (body / step counter). */
  cardMuted?: string;
}

/**
 * A tour lifecycle event, delivered to {@link TourOptions.onEvent}. Lets a host
 * report anonymous progress (start → per-step → done/skip/lost) for analytics.
 */
export type TourEvent =
  /** The tour began at step 0. Not emitted on a resume (a run continuing after a full page reload via {@link TourOptions.resumeKey}) — that fires only the resumed `step`. */
  | { type: "start" }
  /** A step became active (fires for every step, including step 0). */
  | { type: "step"; index: number }
  /** The user completed the last step. */
  | { type: "done" }
  /** The user dismissed the tour while step `index` was active. */
  | { type: "skip"; index: number }
  /** The recovery card was shown at step `index` (its target never appeared). */
  | { type: "lost"; index: number };

export interface TourOptions {
  theme?: TourTheme;
  /** Label for the dismiss control (default: "Skip tour"). */
  skipLabel?: string;
  /** How long to wait for a step's target to appear before showing the recovery card (ms, default 12000). */
  waitTimeoutMs?: number;
  /** Called when the tour ends. `done` = finished the last step; `skip` = user dismissed it. */
  onComplete?: (reason: "done" | "skip") => void;
  /** Called at each tour lifecycle point (start / step / done / skip / lost) — for progress analytics. */
  onEvent?: (event: TourEvent) => void;
  /** sessionStorage key under which the player persists the current step index so a full page reload resumes mid-tour; cleared when the tour ends. */
  resumeKey?: string;
  /** When set, renders a small attribution link (label → href) in the callout card footer. Neutral: the host decides the text and destination; the player carries no branding of its own. */
  badge?: { label: string; href: string };
}

/** Handle returned by `startTour` — call `stop()` to tear the tour down early. */
export interface TourHandle {
  stop(): void;
}

/**
 * Auto-start a tour the first time a user reaches a given state — e.g. an empty
 * "no projects yet" screen — and never again. The app renders `marker` only in
 * that state; the player watches for it, starts the tour once, and remembers.
 */
export interface FirstRunTrigger {
  /** Track key to auto-start. */
  key: string;
  /** CSS selector whose appearance signals first-run (watched until it mounts). */
  marker: string;
  /** localStorage key recording that the tour was shown, so it never auto-starts again (default: `stepshots_tour_seen:<key>`). */
  seenKey?: string;
}

/** Options for the `<script>`-embed convenience. See `autoBoot`. */
export interface AutoBootOptions extends TourOptions {
  /** Registry of named tracks. Defaults to `window.__STEPSHOTS_TOURS`. */
  tracks?: Record<string, TourTrack>;
  /** URL query param that selects a track by key (default: "tour"). */
  param?: string;
  /** sessionStorage key used to keep a tour alive across SPA navigations that drop the query string (default: "stepshots_active_tour"). */
  storageKey?: string;
  /** Auto-start a tour once, the first time a state (marker) appears. See {@link FirstRunTrigger}. */
  firstRun?: FirstRunTrigger;
}
