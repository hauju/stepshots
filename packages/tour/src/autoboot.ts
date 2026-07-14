import { startTour } from "./player";
import type { AutoBootOptions, TourHandle, TourOptions, TourTrack } from "./types";

declare global {
  interface Window {
    __STEPSHOTS_TOURS?: Record<string, TourTrack>;
  }
}

/**
 * Pick the active track key from the URL param, falling back to a stashed one.
 * `fromParam` distinguishes an explicit `?tour=` request (a deliberate fresh
 * start) from a resumed stash, so the caller can reset the step index for the
 * former only.
 */
function resolveKey(
  tracks: Record<string, TourTrack>,
  param: string,
  storageKey: string,
): { key: string; fromParam: boolean } | null {
  const q = new URLSearchParams(window.location.search).get(param);
  if (q && tracks[q]) {
    try {
      sessionStorage.setItem(storageKey, q);
    } catch {}
    return { key: q, fromParam: true };
  }
  // Survive SPA navigations that dropped the query string.
  try {
    const stored = sessionStorage.getItem(storageKey);
    if (stored && tracks[stored]) return { key: stored, fromParam: false };
  } catch {}
  return null;
}

function seen(key: string): boolean {
  try {
    return !!localStorage.getItem(key);
  } catch {
    return false;
  }
}
function markSeen(key: string) {
  try {
    localStorage.setItem(key, "1");
  } catch {}
}

/** Call `cb` when `selector` first appears in the DOM (immediately if already present). */
function waitForSelector(selector: string, cb: () => void, timeoutMs = 20000) {
  if (document.querySelector(selector)) return cb();
  let stopped = false;
  const observer = new MutationObserver(() => {
    if (stopped) return;
    if (document.querySelector(selector)) {
      stopped = true;
      observer.disconnect();
      clearTimeout(timer);
      cb();
    }
  });
  observer.observe(document.body, { childList: true, subtree: true });
  const timer = setTimeout(() => {
    stopped = true;
    observer.disconnect();
  }, timeoutMs);
}

/** Start a track and clear the sessionStorage stash when it ends. `resumeKey` lets
 * the player persist the step index so a full page reload resumes mid-tour. */
function begin(
  track: TourTrack,
  storageKey: string,
  resumeKey: string,
  opts: TourOptions,
): TourHandle {
  return startTour(track, {
    ...opts,
    resumeKey,
    onComplete: (reason) => {
      try {
        sessionStorage.removeItem(storageKey);
      } catch {}
      opts.onComplete?.(reason);
    },
  });
}

/**
 * Zero-config entry point for the `<script>` embed. In priority order it:
 *   1. starts the track named by the URL param (e.g. `?tour=create-project`),
 *      resuming across SPA navigations via sessionStorage; else
 *   2. if `firstRun` is set, auto-starts that tour the first time its `marker`
 *      appears — once per browser (localStorage).
 *
 * `tracks` defaults to `window.__STEPSHOTS_TOURS`. Returns a handle when a tour
 * starts synchronously (the param path), else null (first-run starts async).
 */
export function autoBoot(opts: AutoBootOptions = {}): TourHandle | null {
  // Body must exist before we can mount the overlay.
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => autoBoot(opts), { once: true });
    return null;
  }
  const tracks = opts.tracks ?? window.__STEPSHOTS_TOURS ?? {};
  const param = opts.param ?? "tour";
  const storageKey = opts.storageKey ?? "stepshots_active_tour";
  // Persist the step index alongside the key stash so a full page reload resumes.
  const resumeKey = `${storageKey}:step`;

  // 1. Explicit or resumed tour — highest priority.
  const hit = resolveKey(tracks, param, storageKey);
  if (hit) {
    // An explicit ?tour=<key> is a deliberate (re)start — drop any persisted step
    // so it begins at 0, not where a prior run left off. A resumed stash keeps it.
    if (hit.fromParam) {
      try {
        sessionStorage.removeItem(resumeKey);
      } catch {}
    }
    return begin(tracks[hit.key], storageKey, resumeKey, opts);
  }

  // 2. First-run auto-start, once per browser.
  const fr = opts.firstRun;
  if (fr && tracks[fr.key]) {
    const seenKey = fr.seenKey ?? `stepshots_tour_seen:${fr.key}`;
    if (!seen(seenKey)) {
      waitForSelector(fr.marker, () => {
        if (seen(seenKey)) return;
        // Mark shown at start; sessionStorage resumes across a mid-tour reload.
        markSeen(seenKey);
        try {
          sessionStorage.setItem(storageKey, fr.key);
        } catch {}
        begin(tracks[fr.key], storageKey, resumeKey, opts);
      });
    }
  }
  return null;
}
