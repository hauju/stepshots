// The tour "track" — the data contract between Stepshots (which produces it from
// a recording) and this player (which renders it on a live DOM). A track is plain
// JSON; the player has no dependency on Stepshots to run one.

/** How a step advances to the next one. */
export type Advance =
  /** Advance when the user clicks inside the target element. */
  | { type: "click" }
  /** Advance when the user types a non-empty value into the target element. */
  | { type: "input" };

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

export interface TourOptions {
  theme?: TourTheme;
  /** Label for the dismiss control (default: "Skip tour"). */
  skipLabel?: string;
  /** How long to wait for a step's target to appear before showing the recovery card (ms, default 12000). */
  waitTimeoutMs?: number;
  /** Called when the tour ends. `done` = finished the last step; `skip` = user dismissed it. */
  onComplete?: (reason: "done" | "skip") => void;
}

/** Handle returned by `startTour` — call `stop()` to tear the tour down early. */
export interface TourHandle {
  stop(): void;
}

/** Options for the `<script>`-embed convenience. See `autoBoot`. */
export interface AutoBootOptions extends TourOptions {
  /** Registry of named tracks. Defaults to `window.__STEPSHOTS_TOURS`. */
  tracks?: Record<string, TourTrack>;
  /** URL query param that selects a track by key (default: "tour"). */
  param?: string;
  /** sessionStorage key used to keep a tour alive across SPA navigations that drop the query string (default: "stepshots_active_tour"). */
  storageKey?: string;
}
