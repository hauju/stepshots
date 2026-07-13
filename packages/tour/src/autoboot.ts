import { startTour } from "./player";
import type { AutoBootOptions, TourHandle, TourTrack } from "./types";

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

/**
 * Zero-config entry point for the `<script>` embed. Selects a track by URL param
 * (e.g. `?tour=create-project`), keeps it alive across SPA navigations via
 * sessionStorage, and starts it. No-op when no matching track is active.
 *
 * `tracks` defaults to `window.__STEPSHOTS_TOURS` — the registry a Stepshots
 * export writes. Returns the tour handle, or null if nothing started.
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
  const key = resolveKey(tracks, param, storageKey);
  if (!key) return null;
  return startTour(tracks[key], {
    ...opts,
    onComplete: (reason) => {
      try {
        sessionStorage.removeItem(storageKey);
      } catch {}
      opts.onComplete?.(reason);
    },
  });
}
