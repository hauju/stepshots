import { DEFAULT_THEME, startTour } from "./player";
import type { TourOptions, TourTrack } from "./types";

/** One entry of an onboarding checklist: a labelled launcher for a tour. */
export interface ChecklistItem {
  /** Track key in the registry ({@link ChecklistOptions.tracks}). */
  tour: string;
  /** Label shown in the panel, e.g. "Create your first project". */
  label: string;
  /**
   * Page where the flow starts. When set, clicking the item navigates there
   * first; the checklist mounted on the destination page starts the tour on
   * arrival — the same jump mechanism as `data-stepshots-tour-url`.
   */
  url?: string;
}

/**
 * Options for {@link createChecklist}. Extends {@link TourOptions}: `theme`
 * styles both the checklist and the tours it launches, and the tour callbacks
 * (`onComplete`, `onEvent`, …) are forwarded to every launched run.
 */
export interface ChecklistOptions extends TourOptions {
  /** The items, in the order they should be completed. */
  items: ChecklistItem[];
  /** Launcher and panel heading (default: "Getting started"). */
  title?: string;
  /**
   * localStorage key persisting which tours are done (default:
   * "stepshots_checklist"). Derived keys: `<key>:active` / `<key>:step`
   * (sessionStorage, the in-flight run) and `<key>:hidden` (user dismissed
   * the completed checklist).
   */
  storageKey?: string;
  /** Registry of named tracks. Defaults to `window.__STEPSHOTS_TOURS`. */
  tracks?: Record<string, TourTrack>;
  /** Which corner the launcher sits in (default: "bottom-right"). */
  position?: "bottom-left" | "bottom-right";
  /** Called once, when the last item completes. */
  onAllDone?: () => void;
}

/** Handle returned by `createChecklist`. */
export interface ChecklistHandle {
  /** Expand the panel. */
  open(): void;
  /** Collapse the panel back to the launcher. */
  close(): void;
  /**
   * Mark an item done without running its tour — for when the user did the
   * thing organically (e.g. created a project before taking the tour).
   */
  markDone(tour: string): void;
  /** Remove the checklist UI. Persisted completion state is kept. */
  destroy(): void;
}

// Completion is a plain string array in localStorage — guarded like every
// other storage access in this package (private mode / quota / disabled).
function readDone(key: string): string[] {
  try {
    const raw = localStorage.getItem(key);
    const arr = raw ? JSON.parse(raw) : [];
    return Array.isArray(arr) ? arr.filter((k): k is string => typeof k === "string") : [];
  } catch {
    return [];
  }
}
function writeDone(key: string, doneKeys: string[]) {
  try {
    localStorage.setItem(key, JSON.stringify(doneKeys));
  } catch {}
}

/**
 * Render a persistent "Getting started · 2/5" launcher that expands into a
 * checklist of tours. Clicking an item runs its tour (jumping to `item.url`
 * first when the flow starts elsewhere); an item is checked off when its tour
 * completes — progress persists per browser (localStorage).
 *
 * The checklist launches tours itself, so it works with or without `autoBoot`:
 * mount it on every page (alongside the registry) and cross-page items resume
 * on arrival, mid-tour reloads resume mid-tour. Once every item is done the
 * panel offers a "Hide checklist" action; hiding is remembered, and later
 * `createChecklist` calls render nothing.
 */
export function createChecklist(opts: ChecklistOptions): ChecklistHandle {
  const storageKey = opts.storageKey ?? "stepshots_checklist";
  const activeKey = `${storageKey}:active`;
  const resumeKey = `${storageKey}:step`;
  const hiddenKey = `${storageKey}:hidden`;
  const title = opts.title ?? "Getting started";
  const items = opts.items;

  const done = new Set(readDone(storageKey));
  const allDone = () => items.length > 0 && items.every((i) => done.has(i.tour));
  // A checklist that mounts already-complete must not re-fire onAllDone on
  // every page load — only the transition into completeness fires it.
  let allDoneFired = allDone();
  let destroyed = false;
  let host: HTMLElement | null = null;
  let render: (() => void) | null = null;
  let setOpen: ((open: boolean) => void) | null = null;

  function hidden(): boolean {
    try {
      return !!localStorage.getItem(hiddenKey);
    } catch {
      return false;
    }
  }

  function markDone(tour: string) {
    if (!done.has(tour)) {
      done.add(tour);
      writeDone(storageKey, [...done]);
      render?.();
    }
    if (!allDoneFired && allDone()) {
      allDoneFired = true;
      try {
        opts.onAllDone?.();
      } catch {}
    }
  }

  /** Start a track now, checking its item off when the run completes. */
  function run(tour: string, track: TourTrack) {
    setOpen?.(false); // the panel must not cover the spotlight
    startTour(track, {
      ...opts,
      resumeKey,
      onComplete: (reason) => {
        try {
          sessionStorage.removeItem(activeKey);
        } catch {}
        if (reason === "done") markDone(tour);
        opts.onComplete?.(reason);
      },
    });
  }

  function launch(item: ChecklistItem) {
    // Stash the run before anything else, so a jump — and a mid-tour reload —
    // can pick it back up. A click is a deliberate fresh start (step 0).
    try {
      sessionStorage.setItem(activeKey, item.tour);
      sessionStorage.removeItem(resumeKey);
    } catch {}
    if (item.url) {
      window.location.assign(item.url);
      return;
    }
    // Re-read the registry at click time (like autoBoot's triggers), so tracks
    // merged after mount still resolve.
    const tracks = opts.tracks ?? window.__STEPSHOTS_TOURS ?? {};
    const track = tracks[item.tour];
    if (!track) {
      try {
        sessionStorage.removeItem(activeKey);
      } catch {}
      return;
    }
    run(item.tour, track);
  }

  /** Resume/start the stashed run (jump arrival or mid-tour reload). */
  function resumePending() {
    let key: string | null = null;
    try {
      key = sessionStorage.getItem(activeKey);
    } catch {}
    if (!key) return;
    const tracks = opts.tracks ?? window.__STEPSHOTS_TOURS ?? {};
    const track = tracks[key];
    // No track here: leave the stash — a page carrying the registry picks it up.
    if (track) run(key, track);
  }

  function mount() {
    if (destroyed || hidden()) return;
    const t = { ...DEFAULT_THEME, ...(opts.theme ?? {}) };
    const left = opts.position === "bottom-left";
    host = document.createElement("div");
    host.setAttribute("data-stepshots-checklist", "");
    // Fixed corner, UNDER the tour overlay (z 2147483000) so the spotlight and
    // callout always win; only the widgets themselves take pointer events.
    host.style.cssText = `position:fixed;bottom:20px;${left ? "left" : "right"}:20px;z-index:2147482000;pointer-events:none;`;
    const root = host.attachShadow({ mode: "open" });
    root.innerHTML =
      "<style>" +
      ":host{all:initial}" +
      `.wrap{display:flex;flex-direction:column;align-items:${left ? "flex-start" : "flex-end"};gap:10px;font-family:ui-sans-serif,system-ui,sans-serif}` +
      `.chip{pointer-events:auto;display:inline-flex;align-items:center;gap:8px;background:${t.cardBg};color:${t.cardFg};border:0;border-radius:999px;padding:10px 16px;font-size:13px;font-weight:600;box-shadow:0 12px 34px rgba(2,6,23,.32);cursor:pointer}` +
      `.chip .count{color:${t.accent};font-variant-numeric:tabular-nums}` +
      `.panel{pointer-events:auto;width:280px;background:${t.cardBg};color:${t.cardFg};border-radius:12px;padding:14px 16px;box-shadow:0 12px 34px rgba(2,6,23,.32)}` +
      ".panel[hidden]{display:none}" +
      ".head{display:flex;align-items:center;justify-content:space-between;margin-bottom:8px}" +
      ".head h4{margin:0;font-size:14px;font-weight:700}" +
      `.head .count{font-size:12px;color:${t.cardMuted};font-variant-numeric:tabular-nums}` +
      ".bar{height:4px;border-radius:2px;background:rgba(100,116,139,.25);overflow:hidden;margin-bottom:6px}" +
      `.bar>div{height:100%;background:${t.accent};transition:width .18s ease}` +
      `.item{display:flex;align-items:center;gap:10px;width:100%;background:none;border:0;padding:8px 0;font-size:13px;color:${t.cardFg};cursor:pointer;text-align:left}` +
      `.item .mark{flex:none;width:18px;height:18px;border-radius:50%;border:2px solid rgba(100,116,139,.5);display:inline-flex;align-items:center;justify-content:center;color:#fff;font-size:11px;line-height:1;box-sizing:border-box}` +
      `.item.done .mark{background:${t.accent};border-color:${t.accent}}` +
      `.item.done .label{color:${t.cardMuted}}` +
      ".item:hover .label{text-decoration:underline}" +
      `.hide{background:none;border:0;color:${t.cardMuted};opacity:.8;font-size:12px;cursor:pointer;padding:6px 0 0;display:block}` +
      ".hide:hover{opacity:1}" +
      // Attribution badge — same muted treatment as the player card's.
      `.badge{display:block;margin-top:10px;font-size:10px;color:${t.cardMuted};opacity:.55;text-decoration:none;letter-spacing:.02em}` +
      ".badge:hover{opacity:1;text-decoration:underline}" +
      "@media (prefers-reduced-motion:reduce){.bar>div{transition:none}}" +
      "</style>" +
      '<div class="wrap">' +
      '<div class="panel" hidden><div class="head"><h4></h4><span class="count"></span></div><div class="bar"><div></div></div><div class="list"></div></div>' +
      '<button class="chip" type="button" aria-expanded="false"><span class="label"></span><span class="count"></span></button>' +
      "</div>";
    document.body.appendChild(host);

    const chip = root.querySelector<HTMLButtonElement>(".chip")!;
    const panel = root.querySelector<HTMLElement>(".panel")!;
    const list = root.querySelector<HTMLElement>(".list")!;
    const barFill = root.querySelector<HTMLElement>(".bar>div")!;
    const chipCount = chip.querySelector<HTMLElement>(".count")!;
    const headCount = root.querySelector<HTMLElement>(".head .count")!;
    // Host-supplied copy always lands via textContent — never innerHTML — so a
    // title/label can't inject markup (same rule as the player card).
    chip.querySelector<HTMLElement>(".label")!.textContent = title;
    root.querySelector<HTMLElement>(".head h4")!.textContent = title;

    // Optional attribution link in the panel footer (opts.badge also flows to
    // every launched tour via the TourOptions spread). Same rules as the
    // player: host-supplied, DOM APIs only, never innerHTML.
    if (opts.badge) {
      const badge = document.createElement("a");
      badge.className = "badge";
      badge.setAttribute("part", "badge");
      badge.textContent = opts.badge.label;
      badge.href = opts.badge.href;
      badge.target = "_blank";
      badge.rel = "noopener noreferrer";
      panel.appendChild(badge);
    }

    setOpen = (open: boolean) => {
      if (open) panel.removeAttribute("hidden");
      else panel.setAttribute("hidden", "");
      chip.setAttribute("aria-expanded", String(open));
    };
    chip.addEventListener("click", () => setOpen!(panel.hasAttribute("hidden")));

    render = () => {
      const doneCount = items.filter((i) => done.has(i.tour)).length;
      const counter = `${doneCount}/${items.length}`;
      chipCount.textContent = counter;
      headCount.textContent = counter;
      barFill.style.width = items.length ? `${(doneCount / items.length) * 100}%` : "0%";
      list.textContent = "";
      for (const item of items) {
        const isDone = done.has(item.tour);
        const btn = document.createElement("button");
        btn.type = "button";
        btn.className = isDone ? "item done" : "item";
        btn.setAttribute("aria-label", item.label + (isDone ? " — done" : ""));
        const mark = document.createElement("span");
        mark.className = "mark";
        mark.setAttribute("aria-hidden", "true");
        mark.textContent = isDone ? "✓" : "";
        const label = document.createElement("span");
        label.className = "label";
        label.textContent = item.label;
        btn.append(mark, label);
        btn.addEventListener("click", () => launch(item));
        list.appendChild(btn);
      }
      if (allDone()) {
        // The completed checklist must be dismissable, or the chip squats in
        // the corner forever. Hiding is per-browser and permanent.
        const hide = document.createElement("button");
        hide.type = "button";
        hide.className = "hide";
        hide.textContent = "Hide checklist";
        hide.addEventListener("click", () => {
          try {
            localStorage.setItem(hiddenKey, "1");
          } catch {}
          host?.remove();
          host = null;
        });
        list.appendChild(hide);
      }
    };
    render();
    resumePending();
  }

  // Body must exist before we can mount (same deferral as autoBoot).
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => mount(), { once: true });
  } else {
    mount();
  }

  return {
    open: () => setOpen?.(true),
    close: () => setOpen?.(false),
    markDone,
    destroy() {
      destroyed = true;
      host?.remove();
      host = null;
    },
  };
}
