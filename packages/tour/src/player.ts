import type {
  FirstRunIntro,
  TourEvent,
  TourFallback,
  TourHandle,
  TourOptions,
  TourStep,
  TourTrack,
} from "./types";

export const DEFAULT_THEME = {
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
    // Attribution badge: a muted, low-opacity footer link (set only when opts.badge is present).
    `.badge{display:block;margin-top:10px;font-size:10px;color:${t.cardMuted};opacity:.55;text-decoration:none;letter-spacing:.02em}` +
    ".badge:hover{opacity:1;text-decoration:underline}" +
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

  // Optional attribution link in the card footer. Rendered only when the host
  // opts in, and always via DOM APIs (textContent/href) — never innerHTML — so an
  // arbitrary label/href can't inject markup. Lives inside the card (pointer-
  // events:auto) and outside the step row, so it doesn't affect advance detection.
  if (opts.badge) {
    const badge = document.createElement("a");
    badge.className = "badge";
    badge.setAttribute("part", "badge");
    badge.textContent = opts.badge.label;
    badge.href = opts.badge.href;
    badge.target = "_blank";
    badge.rel = "noopener noreferrer";
    card.appendChild(badge);
  }

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
 * Show a consent card before a first-run tour: a centered invitation with an
 * accept and a decline button, so an auto-started tour never dims the screen
 * unannounced. Calls `onChoice(true)` when the user accepts, `false` when they
 * decline (button or Escape). The card tears itself down on either choice.
 *
 * Non-modal like the rest of the player: the dim is visual only and the page
 * stays clickable — but the choice itself is explicit (no dismiss-by-click-away,
 * since declining is permanent for the browser).
 */
export function showIntro(
  intro: FirstRunIntro,
  options: TourOptions,
  onChoice: (accepted: boolean) => void,
): void {
  const t = { ...DEFAULT_THEME, ...(options.theme ?? {}) };
  const host = document.createElement("div");
  host.setAttribute("data-stepshots-intro", "");
  host.style.cssText = "position:fixed;inset:0;z-index:2147483000;pointer-events:none;";
  const root = host.attachShadow({ mode: "open" });
  root.innerHTML =
    "<style>" +
    ":host{all:initial}" +
    `.dim{position:fixed;inset:0;background:${t.dim};opacity:0;transition:opacity .18s ease;pointer-events:none}` +
    `.card{position:fixed;top:50%;left:50%;transform:translate(-50%,-50%);max-width:340px;width:calc(100vw - 48px);pointer-events:auto;background:${t.cardBg};color:${t.cardFg};border-radius:12px;padding:18px 20px;box-shadow:0 12px 34px rgba(2,6,23,.32);font-family:ui-sans-serif,system-ui,sans-serif;opacity:0;transition:opacity .18s ease}` +
    ".card h4{margin:0 0 6px;font-size:16px;font-weight:700}" +
    `.card p{margin:0;font-size:13px;line-height:1.5;color:${t.cardMuted}}` +
    ".row{display:flex;align-items:center;justify-content:flex-end;gap:14px;margin-top:16px}" +
    `.dismiss{background:none;border:0;color:${t.cardMuted};opacity:.8;font-size:12px;cursor:pointer;padding:0}` +
    ".dismiss:hover{opacity:1}" +
    `.start{background:${t.accent};color:#fff;border:0;border-radius:8px;font-size:13px;font-weight:600;padding:8px 14px;cursor:pointer}` +
    ".start:hover{filter:brightness(1.08)}" +
    `.badge{display:block;margin-top:12px;font-size:10px;color:${t.cardMuted};opacity:.55;text-decoration:none;letter-spacing:.02em}` +
    ".badge:hover{opacity:1;text-decoration:underline}" +
    ".show{opacity:1}" +
    "@media (prefers-reduced-motion:reduce){.dim,.card{transition:none}}" +
    "</style>" +
    '<div class="dim"></div>' +
    '<div class="card" role="dialog" aria-label="Guided tour invitation"><h4></h4><p></p>' +
    '<div class="row"><button class="dismiss" type="button"></button><button class="start" type="button"></button></div></div>';
  document.body.appendChild(host);

  const dim = root.querySelector<HTMLElement>(".dim")!;
  const card = root.querySelector<HTMLElement>(".card")!;
  // All copy is host-supplied — set via textContent so it can't inject markup.
  root.querySelector<HTMLElement>("h4")!.textContent = intro.title;
  root.querySelector<HTMLElement>("p")!.textContent = intro.body;
  const startBtn = root.querySelector<HTMLElement>(".start")!;
  const dismissBtn = root.querySelector<HTMLElement>(".dismiss")!;
  startBtn.textContent = intro.startLabel ?? "Show me around";
  dismissBtn.textContent = intro.dismissLabel ?? "I'll explore on my own";

  // Same attribution treatment as the step card (see createOverlay).
  if (options.badge) {
    const badge = document.createElement("a");
    badge.className = "badge";
    badge.setAttribute("part", "badge");
    badge.textContent = options.badge.label;
    badge.href = options.badge.href;
    badge.target = "_blank";
    badge.rel = "noopener noreferrer";
    card.appendChild(badge);
  }

  let decided = false;
  function choose(accepted: boolean) {
    if (decided) return;
    decided = true;
    document.removeEventListener("keydown", onKey, true);
    host.remove();
    onChoice(accepted);
  }
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") choose(false);
  }
  document.addEventListener("keydown", onKey, true);
  startBtn.addEventListener("click", () => choose(true));
  dismissBtn.addEventListener("click", () => choose(false));
  startBtn.focus();
  // Double-rAF so the fade-in transition plays after insertion.
  requestAnimationFrame(() =>
    requestAnimationFrame(() => {
      dim.classList.add("show");
      card.classList.add("show");
    }),
  );
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
  const inputSettleMs = options.inputSettleMs ?? 1200;
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
  let settleTimer: ReturnType<typeof setTimeout> | null = null;
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
    clearSettle();
    if (observer) {
      observer.disconnect();
      observer = null;
    }
  }

  function clearSettle() {
    if (settleTimer) {
      clearTimeout(settleTimer);
      settleTimer = null;
    }
  }

  function teardown() {
    clearStep();
    document.removeEventListener("click", onClick, true);
    document.removeEventListener("input", onInput, true);
    document.removeEventListener("change", onChange, true);
    document.removeEventListener("focusout", onFocusOut, true);
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
  // Like hitsActive, but for value-bearing steps (input/change): the fallback
  // requires the target itself to MATCH the selector, not just sit inside it.
  function hitsActiveField(target: Element): boolean {
    if (activeEl) return activeEl === target || activeEl.contains(target);
    return !!active && safeMatches(target, active.selector);
  }
  /** `true` when the element carries a string value that is empty/whitespace. */
  function fieldEmpty(el: Element): boolean {
    const v = (el as HTMLInputElement).value;
    return typeof v === "string" && v.trim().length === 0;
  }
  /**
   * A passed input step whose requirement no longer holds (its field is empty
   * again — e.g. the tour resumed on a freshly remounted form). Returns the
   * earliest such step index before `upTo`, or `upTo` when everything holds.
   * A step whose target can't be resolved right now can't be verified — skip it.
   */
  function earliestUnmetInput(upTo: number): number {
    for (let i = 0; i < upTo; i++) {
      const s = steps[i];
      if (s.advance.type !== "input") continue;
      const el = resolveTarget(s);
      if (el && fieldEmpty(el)) return i;
    }
    return upTo;
  }
  function onClick(e: MouseEvent) {
    if (active?.advance.type === "click" && hitsActive(e.target as Element | null)) advance();
  }
  function onInput(e: Event) {
    const target = e.target as Element | null;
    if (!target) return;
    if (active?.advance.type === "input" && hitsActiveField(target)) {
      // Never advance on the first keystroke — that would yank the spotlight
      // away mid-typing. Advance once the value settles; Enter or leaving the
      // field (onKey / onFocusOut) commits immediately.
      clearSettle();
      if (fieldEmpty(target)) return; // emptied — nothing pending
      settleTimer = setTimeout(() => {
        settleTimer = null;
        if (!done) advance();
      }, inputSettleMs);
      return;
    }
    // Typing in a field owned by an EARLIER input step (e.g. the user cleared
    // the project name while the tour already points at Create): if it's empty
    // now, walk back so the tour never asks for an action that can't succeed.
    if (fieldEmpty(target)) {
      for (let i = 0; i < idx; i++) {
        const s = steps[i];
        if (s.advance.type === "input" && safeMatches(target, s.selector)) {
          idx = i;
          startStep();
          return;
        }
      }
    }
  }
  /** Enter / blur on the active input step commits a non-empty value right away. */
  function commitInput(target: Element) {
    if (fieldEmpty(target)) return;
    clearSettle();
    advance();
  }
  function onFocusOut(e: Event) {
    if (active?.advance.type !== "input") return;
    const target = e.target as Element | null;
    if (target && hitsActiveField(target)) commitInput(target);
  }
  function onChange(e: Event) {
    // A `change` event means a committed value (e.g. a picked dropdown option),
    // so there's no emptiness check like `input`.
    if (active?.advance.type !== "change") return;
    const target = e.target as Element | null;
    if (target && hitsActiveField(target)) advance();
  }
  function onKey(e: KeyboardEvent) {
    // Esc dismisses like Skip. Don't preventDefault/stopPropagation — the host
    // page may also react to Esc and that's fine.
    if (e.key === "Escape") return finish("skip");
    if (e.key === "Enter" && active?.advance.type === "input") {
      const target = e.target as Element | null;
      if (target && hitsActiveField(target)) commitInput(target);
    }
  }
  document.addEventListener("click", onClick, true);
  document.addEventListener("input", onInput, true);
  document.addEventListener("change", onChange, true);
  document.addEventListener("focusout", onFocusOut, true);
  document.addEventListener("keydown", onKey, true);

  function startStep(): void {
    clearStep(); // stop the previous step's loop/observer/timer first
    // Walk back before announcing anything: a passed input step whose field is
    // empty again (resumed on a reset form) must be redone before this one.
    idx = earliestUnmetInput(idx);
    active = steps[idx];
    activeEl = null;
    if (resumeKey) writeResume(resumeKey, idx); // so a reload re-enters at this step
    emit({ type: "step", index: idx });
    overlay.hide();
    overlay.setText(active.title, active.body, idx, steps.length);
    const el = resolveTarget(active);
    if (el) return acquire(el);
    // Not mounted yet (SPA nav / loading gate) — wait for it.
    observer = new MutationObserver(() => {
      if (done) return;
      const found = resolveTarget(active!);
      if (found) {
        clearStep();
        acquire(found);
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

  // A step target is live — but before spotlighting it, make sure the steps that
  // led here still hold (a resumed run may re-enter a form that reset in the
  // meantime). If an earlier input step's field is empty again, walk back to it
  // instead of asking for an action that can't succeed.
  function acquire(el: HTMLElement): void {
    const back = earliestUnmetInput(idx);
    if (back < idx) {
      idx = back;
      return startStep();
    }
    track_(el);
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
        if (activeEl === null) {
          // Re-acquired after the target went away (SPA nav away and back). The
          // form may have remounted empty — re-run the walk-back check first.
          const back = earliestUnmetInput(idx);
          if (back < idx) {
            idx = back;
            startStep();
            return; // startStep took over; don't reschedule this loop
          }
        }
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
