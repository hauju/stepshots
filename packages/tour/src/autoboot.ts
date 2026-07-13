import { startTour } from "./player";
import type { AutoBootOptions, TourHandle, TourOptions, TourTrack } from "./types";

declare global {
  interface Window {
    __STEPSHOTS_TOURS?: Record<string, TourTrack>;
  }
}

/** Pick the active track key from the URL param, falling back to a stashed one. */
function resolveKey(
  tracks: Record<string, TourTrack>,
  param: string,
  storageKey: string,
): string | null {
  const q = new URLSearchParams(window.location.search).get(param);
  if (q && tracks[q]) {
    try {
      sessionStorage.setItem(storageKey, q);
    } catch {}
    return q;
  }
  // Survive SPA navigations that dropped the query string.
  try {
    const stored = sessionStorage.getItem(storageKey);
    if (stored && tracks[stored]) return stored;
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

/** Start a track and clear the sessionStorage stash when it ends. */
function begin(track: TourTrack, storageKey: string, opts: TourOptions): TourHandle {
  return startTour(track, {
    ...opts,
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

  // 1. Explicit or resumed tour — highest priority.
  const key = resolveKey(tracks, param, storageKey);
  if (key) return begin(tracks[key], storageKey, opts);

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
        begin(tracks[fr.key], storageKey, opts);
      });
    }
  }
  return null;
}
