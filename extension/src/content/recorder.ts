import type { ElementBounds, RecordedStep, StepAction, StepMeta } from "../types";
import { generateSelector } from "./selector";
import { showToast } from "./popover";

let active = false;
let paused = false;
let typeBuffer: { el: Element; value: string; blurHandler: (() => void) | null } | null = null;

const SENSITIVE_AUTOCOMPLETE = new Set([
  "cc-number", "cc-cvc", "cc-exp", "cc-exp-month", "cc-exp-year", "cc-name", "cc-type",
  "new-password", "current-password", "one-time-code",
]);

export function activate(): void {
  if (active) return;
  active = true;
  paused = false;
  document.addEventListener("click", onClickCapture, { capture: true });
  document.addEventListener("input", onInput, { capture: true });
  document.addEventListener("change", onChange, { capture: true });
  document.addEventListener("keydown", onKeyDown, { capture: true });
}

export function deactivate(): void {
  if (!active) return;
  active = false;
  paused = false;
  flushTypeBuffer();
  document.removeEventListener("click", onClickCapture, { capture: true });
  document.removeEventListener("input", onInput, { capture: true });
  document.removeEventListener("change", onChange, { capture: true });
  document.removeEventListener("keydown", onKeyDown, { capture: true });
}

export function pause(): void {
  paused = true;
  flushTypeBuffer();
}

export function resume(): void {
  paused = false;
}

// --- Sensitive field detection ---

function isSensitiveInput(el: Element): { sensitive: boolean; type?: string } {
  if (el.tagName !== "INPUT") return { sensitive: false };
  const input = el as HTMLInputElement;

  if (input.type === "password" || input.type === "hidden") {
    return { sensitive: true, type: input.type };
  }

  const ac = input.getAttribute("autocomplete");
  if (ac && SENSITIVE_AUTOCOMPLETE.has(ac)) {
    return { sensitive: true, type: ac.startsWith("cc-") ? "credit-card" : "password" };
  }

  return { sensitive: false };
}

// --- Element metadata capture ---

function captureElementMeta(el: Element): StepMeta {
  const meta: StepMeta = {};

  meta.tagName = el.tagName.toLowerCase();

  const text = (el as HTMLElement).textContent?.trim();
  if (text) meta.elementText = text.slice(0, 80);

  const ariaLabel = el.getAttribute("aria-label");
  if (ariaLabel) meta.ariaLabel = ariaLabel;

  const placeholder = el.getAttribute("placeholder");
  if (placeholder) meta.placeholder = placeholder;

  const name = el.getAttribute("name");
  if (name) meta.fieldName = name;

  if (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT") {
    meta.inputType = (el as HTMLInputElement).type;

    // Find associated label
    const id = el.getAttribute("id");
    if (id) {
      const label = document.querySelector(`label[for="${id}"]`);
      if (label?.textContent) meta.labelText = label.textContent.trim();
    }
    if (!meta.labelText) {
      const parentLabel = el.closest("label");
      if (parentLabel?.textContent) meta.labelText = parentLabel.textContent.trim();
    }
  }

  const sens = isSensitiveInput(el);
  if (sens.sensitive) {
    meta.sensitive = true;
    meta.sensitiveType = sens.type;
  }

  return meta;
}

// --- Click ---
function onClickCapture(e: MouseEvent): void {
  void handleClickCapture(e);
}

async function handleClickCapture(e: MouseEvent): Promise<void> {
  if (!active || paused) return;
  if (!e.isTrusted) return;
  const target = e.target as Element;
  if (!target) return;
  flushTypeBuffer();

  const step = createStep("click", target);
  const interceptedLink = findInterceptableLink(target, e);
  if (interceptedLink) {
    e.preventDefault();
    e.stopImmediatePropagation();
    e.stopPropagation();
  }

  await chrome.runtime.sendMessage({ type: "CAPTURE_STEP_SCREENSHOT", stepId: step.id }).catch(() => {});

  // For sensitive fields, strip any value data
  if (step.meta?.sensitive) {
    step.text = undefined;
    step.value = undefined;
    recordStep(step, target, "Sensitive — value not recorded");
  } else {
    recordStep(step, target);
  }

  if (interceptedLink) {
    interceptedLink.click();
  }
}

// --- Type (input on text fields) ---
function onInput(e: Event): void {
  if (!active || paused) return;
  const target = e.target as HTMLInputElement | HTMLTextAreaElement;
  if (!target || !isTextInput(target)) return;

  // Skip sensitive fields entirely — click already recorded
  if (isSensitiveInput(target).sensitive) return;

  if (typeBuffer && typeBuffer.el !== target) {
    flushTypeBuffer();
  }

  if (!typeBuffer) {
    const blurHandler = () => flushTypeBuffer();
    target.addEventListener("blur", blurHandler, { once: true });
    typeBuffer = { el: target, value: target.value, blurHandler };
  } else {
    typeBuffer.value = target.value;
  }
}

function flushTypeBuffer(): void {
  if (!typeBuffer) return;
  const { el, value, blurHandler } = typeBuffer;
  if (blurHandler) {
    el.removeEventListener("blur", blurHandler);
  }
  typeBuffer = null;

  if (!value) return;

  const step = createStep("type", el);
  step.text = value;
  recordStep(step, el);
}

// --- Select (change on <select>) ---
function onChange(e: Event): void {
  if (!active || paused) return;
  const target = e.target as HTMLSelectElement;
  if (target.tagName !== "SELECT") return;
  flushTypeBuffer();

  const step = createStep("select", target);
  step.value = target.value;
  recordStep(step, target);
}

// --- Key (non-printable) ---
function onKeyDown(e: KeyboardEvent): void {
  if (!active || paused) return;
  if (e.altKey && e.shiftKey && e.key.toLowerCase() === "s") {
    e.preventDefault();
    e.stopPropagation();
    flushTypeBuffer();
    chrome.runtime.sendMessage({ type: "CAPTURE_SCREEN" }).catch(() => {});
    const step: RecordedStep = {
      id: crypto.randomUUID(),
      action: "wait",
      meta: { captureOnly: true },
      timestamp: Date.now(),
    };
    showToast(step, document.body, "Screen captured");
    return;
  }
  const nonPrintable = ["Enter", "Escape", "Tab", "Backspace", "Delete", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"];
  if (!nonPrintable.includes(e.key) && !e.metaKey && !e.ctrlKey) return;

  if (["Meta", "Control", "Shift", "Alt"].includes(e.key)) return;

  const editingKeys = ["Backspace", "Delete", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"];
  if (editingKeys.includes(e.key) && !e.metaKey && !e.ctrlKey) {
    const target = e.target as Element;
    if (target && isTextInput(target)) return;
  }

  flushTypeBuffer();

  const parts: string[] = [];
  if (e.metaKey) parts.push("cmd");
  if (e.ctrlKey) parts.push("ctrl");
  if (e.shiftKey) parts.push("shift");
  if (e.altKey) parts.push("alt");
  parts.push(e.key);

  const step: RecordedStep = {
    id: crypto.randomUUID(),
    action: "key",
    key: parts.join("+"),
    timestamp: Date.now(),
  };

  sendStep(step);
}

// --- Helpers ---

function createStep(action: StepAction, el: Element): RecordedStep {
  return {
    id: crypto.randomUUID(),
    action,
    selector: generateSelector(el),
    targetBounds: captureTargetBounds(el),
    meta: captureElementMeta(el),
    timestamp: Date.now(),
  };
}

function captureTargetBounds(el: Element): ElementBounds | undefined {
  const rect = el.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) {
    return undefined;
  }
  return {
    x: Math.round(rect.left),
    y: Math.round(rect.top),
    width: Math.round(rect.width),
    height: Math.round(rect.height),
  };
}

function recordStep(step: RecordedStep, target: Element, toastMessage?: string): void {
  sendStep(step);
  showToast(step, target, toastMessage);
}

function sendStep(step: RecordedStep): void {
  chrome.runtime.sendMessage({ type: "STEP_RECORDED", step }).catch(() => {});
}

function isTextInput(el: Element): boolean {
  if (el.tagName === "TEXTAREA") return true;
  if (el.tagName === "INPUT") {
    const type = (el as HTMLInputElement).type;
    return ["text", "email", "search", "url", "tel", "number"].includes(type);
  }
  if ((el as HTMLElement).isContentEditable) return true;
  return false;
}

function findInterceptableLink(target: Element, e: MouseEvent): HTMLAnchorElement | null {
  if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) {
    return null;
  }

  const anchor = target.closest("a[href]") as HTMLAnchorElement | null;
  if (!anchor) {
    return null;
  }

  if (anchor.hasAttribute("download")) {
    return null;
  }

  const targetAttr = anchor.getAttribute("target");
  if (targetAttr && targetAttr !== "_self") {
    return null;
  }

  const href = anchor.href;
  if (!href) {
    return null;
  }

  try {
    const url = new URL(href, location.href);
    if (!/^https?:$/.test(url.protocol)) {
      return null;
    }
  } catch {
    return null;
  }

  return anchor;
}
