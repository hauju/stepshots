import type { TourEvent, TourFallback, TourHandle, TourOptions, TourStep, TourTrack } from "./types";

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

/** Honor the OS "reduce motion" setting (guarded — matchMedia is absent in some test envs). */
function reduceMotion(): boolean {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

// Selectors come from recorded (possibly stale or hand-authored) data, so a
// malformed one must never throw out of an event handler or animation frame —
// it would fire on every click/keystroke of the host page. These swallow
// `SyntaxError` and let the caller fall through to the text/aria fallback.
function safeQueryAll(selector: string): ArrayLike<HTMLElement> {
  try {
    return document.querySelectorAll<HTMLElement>(selector);
  } catch {
    return [];
  }
}
function safeClosest(target: Element, selector: string): boolean {
  try {
    return !!target.closest(selector);
  } catch {
    return false;
  }
}
function safeMatches(target: Element, selector: string): boolean {
  try {
    return !!target.matches?.(selector);
  } catch {
    return false;
  }
}

/** Find the first element matching `selector` that is actually rendered (non-zero box). */
function findVisible(selector: string): HTMLElement | null {
  const els = safeQueryAll(selector);
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
    const candidates = safeQueryAll("a,button,[role='button'],[data-testid]");
    // Exact text wins. A substring ("contains") match is only trusted when it's
    // unambiguous — otherwise "Save" would silently pick one of several buttons
    // (or match "Save and exit"), advancing the tour on the wrong element.
    const partial: HTMLElement[] = [];
    for (let i = 0; i < candidates.length; i++) {
      const el = candidates[i];
      if (!isVisible(el)) continue;
      const text = (el.textContent || "").replace(/\s+/g, " ").trim().toLowerCase();
      if (!text) continue;
      if (text === wantText) return el;
      if (text.includes(wantText)) partial.push(el);
    }
    if (partial.length === 1) return partial[0];
  }
  return null;
}

/** Resolve a step's live target: CSS selector first, recorded identity as fallback. */
function resolveTarget(step: TourStep): HTMLElement | null {
  return findVisible(step.selector) ?? findByFallback(step.fallback);
}

// Resume persistence — sessionStorage can throw (disabled/quota) or be absent, so
// every access is guarded. A missing/unparseable value reads as NaN (start fresh).
function readResume(key: string): number {
  try {
    return parseInt(sessionStorage.getItem(key) ?? "", 10);
  } catch {
    return NaN;
  }
}
function writeResume(key: string, idx: number) {
  try {
    sessionStorage.setItem(key, String(idx));
  } catch {}
}
function clearResume(key: string) {
  try {
    sessionStorage.removeItem(key);
  } catch {}
}

interface Overlay {
  onSkip: (() => void) | null;
  hide(): void;
  setText(title: string, body: string, idx: number, total: number): void;
  position(rect: DOMRect): void;
  /** Show the card with no spotlight, centered — for the "lost the trail" recovery state. */
  showCard(): void;
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
    // Reduced-motion: drop the spotlight/card tweens for users who ask for it.
    "@media (prefers-reduced-motion:reduce){.spot,.card{transition:none}}" +
    "</style>" +
    '<div class="spot" part="spot"></div>' +
    // role=dialog + aria-label names the callout; aria-live announces each step's
    // text on setText. No aria-modal — the page stays interactive by design.
    '<div class="card" role="dialog" aria-label="Guided tour" aria-live="polite" aria-atomic="true"><h4></h4><p></p>' +
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
    showCard() {
      // No anchor element — hide the spotlight and float the card near top-center
      // so the recovery message is actually visible.
      spot.classList.remove("show");
      const w = card.offsetWidth || 300;
      card.style.top = "24px";
      card.style.left = Math.max(12, (window.innerWidth - w) / 2) + "px";
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
// One live tour at a time per bundle instance — a second startTour() (or a
// duplicated embed) would stack overlays and double-advance.
let tourActive = false;

export function startTour(track: TourTrack, options: TourOptions = {}): TourHandle {
  if (tourActive) return { stop() {} };
  tourActive = true;

  const steps = track.steps;
  const waitTimeoutMs = options.waitTimeoutMs ?? 12000;
  const overlay = createOverlay(options);
  // Resume mid-tour after a full page reload: an in-range stored index (> 0) means
  // the same run continues at that step. Anything else (missing/NaN/0/out of range
  // — e.g. the track changed since) starts fresh at 0 and overwrites the stash.
  const resumeKey = options.resumeKey;
  const stored = resumeKey ? readResume(resumeKey) : NaN;
  const resuming = Number.isInteger(stored) && stored > 0 && stored < steps.length;
  let idx = resuming ? stored : 0;
  let active: TourStep | null = null;
  // The element currently spotlighted (via selector OR fallback). Lets advance
  // detection work even when the step's CSS selector no longer matches.
  let activeEl: HTMLElement | null = null;
  let rafId: number | null = null;
  let waitTimer: ReturnType<typeof setTimeout> | null = null;
  let observer: MutationObserver | null = null;
  let done = false;

  // A throwing consumer callback must never break the player — the tour keeps
  // running even if the host's analytics reporting fails.
  function emit(event: TourEvent) {
    try {
      options.onEvent?.(event);
    } catch {}
  }

  // Cancel everything the CURRENT step owns — rAF loop, wait timer, and the
  // wait-state MutationObserver — so nothing outlives its step (no leaked
  // observers or zombie animation loops). Run on every transition and teardown.
  function clearStep() {
    if (rafId != null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
    if (waitTimer) {
      clearTimeout(waitTimer);
      waitTimer = null;
    }
    if (observer) {
      observer.disconnect();
      observer = null;
    }
  }

  function teardown() {
    clearStep();
    document.removeEventListener("click", onClick, true);
    document.removeEventListener("input", onInput, true);
    document.removeEventListener("keydown", onKey, true);
    overlay.destroy();
  }

  function finish(reason: "done" | "skip") {
    if (done) return;
    done = true;
    tourActive = false;
    teardown();
    if (resumeKey) clearResume(resumeKey); // the run ended — don't resume it next load
    emit(reason === "done" ? { type: "done" } : { type: "skip", index: idx });
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
    // Prefer the element we actually spotlighted: advancing must require hitting
    // THAT element, not any element a broad recorded selector happens to match.
    if (activeEl) return activeEl === target || activeEl.contains(target);
    // No spotlight resolved (recovery state) — best-effort selector match.
    return !!active && safeClosest(target, active.selector);
  }
  function onClick(e: MouseEvent) {
    if (active?.advance.type === "click" && hitsActive(e.target as Element | null)) advance();
  }
  function onInput(e: Event) {
    if (active?.advance.type !== "input") return;
    const target = e.target as HTMLInputElement | null;
    if (!target || String(target.value || "").trim().length === 0) return;
    if (activeEl) {
      if (activeEl === target || activeEl.contains(target)) advance();
    } else if (active && safeMatches(target, active.selector)) {
      advance();
    }
  }
  function onKey(e: KeyboardEvent) {
    // Esc dismisses like Skip. Don't preventDefault/stopPropagation — the host
    // page may also react to Esc and that's fine.
    if (e.key === "Escape") finish("skip");
  }
  document.addEventListener("click", onClick, true);
  document.addEventListener("input", onInput, true);
  document.addEventListener("keydown", onKey, true);

  function startStep() {
    clearStep(); // stop the previous step's loop/observer/timer first
    active = steps[idx];
    activeEl = null;
    if (resumeKey) writeResume(resumeKey, idx); // so a reload re-enters at this step
    emit({ type: "step", index: idx });
    overlay.hide();
    overlay.setText(active.title, active.body, idx, steps.length);
    const el = resolveTarget(active);
    if (el) return track_(el);
    // Not mounted yet (SPA nav / loading gate) — wait for it.
    observer = new MutationObserver(() => {
      if (done) return;
      const found = resolveTarget(active!);
      if (found) {
        clearStep();
        track_(found);
      }
    });
    observer.observe(document.body, { childList: true, subtree: true });
    waitTimer = setTimeout(() => {
      if (done) return;
      clearStep();
      emit({ type: "lost", index: idx });
      // Lost the trail: show a VISIBLE recovery card (no spotlight to anchor to).
      overlay.setText(
        "Hmm, we lost the trail",
        "The next step didn't show up. You can keep going on your own.",
        idx,
        steps.length,
      );
      overlay.showCard();
    }, waitTimeoutMs);
  }

  function track_(el: HTMLElement) {
    activeEl = el;
    el.scrollIntoView({ block: "center", behavior: reduceMotion() ? "auto" : "smooth" });
    if (rafId != null) cancelAnimationFrame(rafId);
    const loop = () => {
      if (done) return; // don't reschedule after teardown
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

  // A resume is the same run continuing, so it emits only its step, not "start".
  if (!resuming) emit({ type: "start" });
  startStep();
  return { stop: () => finish("skip") };
}
