import type { RecordedStep } from "../types";
// @ts-ignore - Bun handles CSS text imports via build
import toastStyles from "./styles.css";

let hostEl: HTMLElement | null = null;
let shadowRoot: ShadowRoot | null = null;

function ensureHost(): ShadowRoot {
  if (hostEl && shadowRoot) return shadowRoot;

  hostEl = document.createElement("div");
  hostEl.id = "rc-toast-host";
  shadowRoot = hostEl.attachShadow({ mode: "closed" });

  const style = document.createElement("style");
  style.textContent = toastStyles;
  shadowRoot.appendChild(style);

  document.body.appendChild(hostEl);
  return shadowRoot;
}

export function setToastVisible(visible: boolean): void {
  if (hostEl) hostEl.style.display = visible ? "" : "none";
}

export function showToast(step: RecordedStep, _target: Element, message?: string): void {
  const root = ensureHost();

  // Remove any existing toast
  const existing = root.querySelector(".rc-toast");
  if (existing) existing.remove();

  const isSensitive = step.meta?.sensitive;
  const badge = isSensitive ? step.meta?.sensitiveType ?? "password" : step.action;
  const text = message ?? "Step recorded";

  const toast = document.createElement("div");
  toast.className = `rc-toast${isSensitive ? " rc-toast-sensitive" : ""}`;

  toast.innerHTML = `
    <span class="rc-toast-badge${isSensitive ? " rc-toast-badge-sensitive" : ""}">${badge}</span>
    <span class="rc-toast-text">${text}</span>
  `;

  root.appendChild(toast);

  // Trigger fade-out after 1.5s
  setTimeout(() => {
    toast.classList.add("rc-toast-fade-out");
    toast.addEventListener("transitionend", () => toast.remove());
  }, 1500);
}
