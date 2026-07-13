import type { TourFallback, TourHandle, TourOptions, TourStep, TourTrack } from "./types";

const DEFAULT_THEME = {
  accent: "#3b82f6",
  dim: "rgba(15,23,42,.55)",
  cardBg: "#ffffff",
  cardFg: "#0f172a",
  cardMuted: "#475569",
};

/** Is the element actually rendered (non-zero box)? */
function isVisible(el: HTMLElement): boolean {
  const r = el.getBoundingClientRect();
  return r.width > 0 && r.height > 0;
}

/** Find the first element matching `selector` that is actually rendered (non-zero box). */
function findVisible(selector: string): HTMLElement | null {
  const els = document.querySelectorAll<HTMLElement>(selector);
  for (let i = 0; i < els.length; i++) {
    if (isVisible(els[i])) return els[i];
  }
  return null;
}

/**
 * Recover a step's target by its recorded identity (aria-label / visible text)
 * when the CSS selector no longer matches — the UI drifted since recording.
 * aria-label is the strongest anchor and is tried first; text falls back to an
 * exact match, then containment. Best-effort: returns null if nothing fits.
 */
function findByFallback(fallback?: TourFallback): HTMLElement | null {
  if (!fallback) return null;
  const wantAria = fallback.aria?.trim().toLowerCase();
  const wantText = fallback.text?.trim().toLowerCase();
  if (!wantAria && !wantText) return null;

  if (wantAria) {
    const els = document.querySelectorAll<HTMLElement>("[aria-label]");
    for (let i = 0; i < els.length; i++) {
      const aria = (els[i].getAttribute("aria-label") || "").trim().toLowerCase();
      if (aria === wantAria && isVisible(els[i])) return els[i];
    }
  }

  if (wantText) {
    const candidates = document.querySelectorAll<HTMLElement>(
      "a,button,[role='button'],[data-testid]",
    );
    let contains: HTMLElement | null = null;
    for (let i = 0; i < candidates.length; i++) {
      const el = candidates[i];
      if (!isVisible(el)) continue;
      const text = (el.textContent || "").replace(/\s+/g, " ").trim().toLowerCase();
      if (!text) continue;
      if (text === wantText) return el;
      if (!contains && text.includes(wantText)) contains = el;
    }
    if (contains) return contains;
  }
  return null;
}

/** Resolve a step's live target: CSS selector first, recorded identity as fallback. */
function resolveTarget(step: TourStep): HTMLElement | null {
  return findVisible(step.selector) ?? findByFallback(step.fallback);
}

interface Overlay {
  onSkip: (() => void) | null;
  hide(): void;
  setText(title: string, body: string, idx: number, total: number): void;
  position(rect: DOMRect): void;
  destroy(): void;
}

function createOverlay(opts: TourOptions): Overlay {
  const t = { ...DEFAULT_THEME, ...(opts.theme ?? {}) };
  const host = document.createElement("div");
  host.setAttribute("data-stepshots-tour", "");
  // Fixed full-viewport, click-through by default; interactive bits opt back in.
  host.style.cssText = "position:fixed;inset:0;z-index:2147483000;pointer-events:none;";
  const root = host.attachShadow({ mode: "open" });
  root.innerHTML =
    "<style>" +
    ":host{all:initial}" +
    // Spotlight: a transparent box whose huge box-shadow dims everything else.
    `.spot{position:fixed;border-radius:10px;pointer-events:none;box-shadow:0 0 0 9999px ${t.dim};outline:2px solid ${t.accent};outline-offset:3px;transition:all .18s ease;opacity:0}` +
    `.card{position:fixed;max-width:300px;pointer-events:auto;background:${t.cardBg};color:${t.cardFg};border-radius:12px;padding:14px 16px;box-shadow:0 12px 34px rgba(2,6,23,.32);font-family:ui-sans-serif,system-ui,sans-serif;opacity:0;transition:opacity .18s ease}` +
    ".card h4{margin:0 0 4px;font-size:14px;font-weight:700}" +
    `.card p{margin:0;font-size:13px;line-height:1.45;color:${t.cardMuted}}` +
    ".row{display:flex;align-items:center;justify-content:space-between;margin-top:12px}" +
    `.step{font-size:11px;color:${t.cardMuted};opacity:.8;letter-spacing:.04em}` +
    `.skip{background:none;border:0;color:${t.cardMuted};opacity:.8;font-size:12px;cursor:pointer;padding:0}` +
    ".skip:hover{opacity:1}" +
    ".show{opacity:1}" +
    "</style>" +
    '<div class="spot" part="spot"></div>' +
    '<div class="card"><h4></h4><p></p>' +
    '<div class="row"><span class="step"></span>' +
    `<button class="skip" type="button">${opts.skipLabel ?? "Skip tour"}</button></div></div>`;
  document.body.appendChild(host);

  const spot = root.querySelector<HTMLElement>(".spot")!;
  const card = root.querySelector<HTMLElement>(".card")!;
  const h4 = root.querySelector<HTMLElement>("h4")!;
  const p = root.querySelector<HTMLElement>("p")!;
  const stepLabel = root.querySelector<HTMLElement>(".step")!;

  const overlay: Overlay = {
    onSkip: null,
    hide() {
      spot.classList.remove("show");
      card.classList.remove("show");
    },
    setText(title, body, idx, total) {
      h4.textContent = title;
      p.textContent = body;
      stepLabel.textContent = "Step " + (idx + 1) + " of " + total;
    },
    position(rect) {
      const pad = 6;
      spot.style.top = rect.top - pad + "px";
      spot.style.left = rect.left - pad + "px";
      spot.style.width = rect.width + pad * 2 + "px";
      spot.style.height = rect.height + pad * 2 + "px";
      spot.classList.add("show");
      // Card below the target, flipping above if it would overflow the viewport.
      const cardH = card.offsetHeight || 120;
      const below = rect.bottom + 12;
      const top =
        below + cardH > window.innerHeight && rect.top - cardH - 12 > 0
          ? rect.top - cardH - 12
          : below;
      const left = Math.min(
        Math.max(12, rect.left),
        window.innerWidth - (card.offsetWidth || 300) - 12,
      );
      card.style.top = top + "px";
      card.style.left = left + "px";
      card.classList.add("show");
    },
    destroy() {
      host.remove();
    },
  };

  root.querySelector<HTMLElement>(".skip")!.addEventListener("click", () => overlay.onSkip?.());
  return overlay;
}

/**
 * Start a live guided tour. Renders a shadow-DOM spotlight that anchors to each
 * step's element on the real page, waiting for nodes that mount late (SPA nav /
 * hydration), and advances when the user performs the step's action.
 *
 * Framework-agnostic: operates purely on the rendered DOM. Returns a handle whose
 * `stop()` tears everything down.
 */
export function startTour(track: TourTrack, options: TourOptions = {}): TourHandle {
  const steps = track.steps;
  const waitTimeoutMs = options.waitTimeoutMs ?? 12000;
  const overlay = createOverlay(options);
  let idx = 0;
  let active: TourStep | null = null;
  // The element currently spotlighted (via selector OR fallback). Lets advance
  // detection work even when the step's CSS selector no longer matches.
  let activeEl: HTMLElement | null = null;
  let rafId: number | null = null;
  let waitTimer: ReturnType<typeof setTimeout> | null = null;
  let done = false;

  function teardown() {
    if (rafId) cancelAnimationFrame(rafId);
    if (waitTimer) clearTimeout(waitTimer);
    document.removeEventListener("click", onClick, true);
    document.removeEventListener("input", onInput, true);
    overlay.destroy();
  }

  function finish(reason: "done" | "skip") {
    if (done) return;
    done = true;
    teardown();
    options.onComplete?.(reason);
  }

  overlay.onSkip = () => finish("skip");

  function advance() {
    idx += 1;
    if (idx >= steps.length) return finish("done");
    startStep();
  }

  function hitsActive(target: Element | null): boolean {
    if (!target) return false;
    if (active && target.closest(active.selector)) return true;
    // Selector drifted: accept the element we actually spotlighted via fallback.
    return !!activeEl && (activeEl === target || activeEl.contains(target));
  }
  function onClick(e: MouseEvent) {
    if (active?.advance.type === "click" && hitsActive(e.target as Element | null)) advance();
  }
  function onInput(e: Event) {
    const target = e.target as HTMLInputElement | null;
    if (
      active?.advance.type === "input" &&
      (target?.matches?.(active.selector) || (!!activeEl && activeEl === target)) &&
      String(target?.value || "").trim().length > 0
    )
      advance();
  }
  document.addEventListener("click", onClick, true);
  document.addEventListener("input", onInput, true);

  function startStep() {
    active = steps[idx];
    activeEl = null;
    overlay.hide();
    overlay.setText(active.title, active.body, idx, steps.length);
    const el = resolveTarget(active);
    if (el) return track_(el);
    // Not mounted yet (SPA nav / loading gate) — wait for it.
    let stopped = false;
    const observer = new MutationObserver(() => {
      if (stopped) return;
      const found = resolveTarget(active!);
      if (found) {
        stopped = true;
        observer.disconnect();
        if (waitTimer) clearTimeout(waitTimer);
        track_(found);
      }
    });
    observer.observe(document.body, { childList: true, subtree: true });
    waitTimer = setTimeout(() => {
      if (stopped) return;
      stopped = true;
      observer.disconnect();
      // Lost the trail: show a recovery card instead of a spotlight.
      overlay.setText(
        "Hmm, we lost the trail",
        "The next step didn't show up. You can keep going on your own.",
        idx,
        steps.length,
      );
    }, waitTimeoutMs);
  }

  function track_(el: HTMLElement) {
    activeEl = el;
    el.scrollIntoView({ block: "center", behavior: "smooth" });
    if (rafId) cancelAnimationFrame(rafId);
    const loop = () => {
      // Re-resolve each frame so re-renders / detaches don't strand us.
      const cur = document.body.contains(el) ? el : resolveTarget(active!);
      if (!cur) {
        activeEl = null;
        overlay.hide();
      } else {
        el = cur;
        activeEl = cur;
        const r = el.getBoundingClientRect();
        if (r.width > 0 && r.height > 0) overlay.position(r);
      }
      rafId = requestAnimationFrame(loop);
    };
    loop();
  }

  startStep();
  return { stop: () => finish("skip") };
}
