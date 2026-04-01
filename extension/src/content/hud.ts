// @ts-ignore - Bun handles CSS text imports via build
import hudStyles from "./hud-styles.css";

let hostEl: HTMLElement | null = null;
let shadowRoot: ShadowRoot | null = null;
let hudEl: HTMLElement | null = null;

// Drag state
let isDragging = false;
let dragOffsetX = 0;
let dragOffsetY = 0;

function ensureHost(): ShadowRoot {
  if (hostEl && shadowRoot) return shadowRoot;

  hostEl = document.createElement("div");
  hostEl.id = "rc-hud-host";
  shadowRoot = hostEl.attachShadow({ mode: "closed" });

  const style = document.createElement("style");
  style.textContent = hudStyles;
  shadowRoot.appendChild(style);

  document.body.appendChild(hostEl);
  return shadowRoot;
}

export function showHud(): void {
  const root = ensureHost();

  if (hudEl) hudEl.remove();

  hudEl = document.createElement("div");
  hudEl.className = "rc-hud";
  hudEl.innerHTML = `
    <div class="rc-hud-drag" title="Drag to move">
      <span class="rc-hud-drag-dots">&#x2630;</span>
    </div>
    <div class="rc-hud-body">
      <div class="rc-hud-status-row">
        <span class="rc-hud-dot"></span>
        <span class="rc-hud-status-text">Recording</span>
        <span class="rc-hud-step-count">0 steps</span>
      </div>
      <div class="rc-hud-last-action"></div>
      <div class="rc-hud-controls">
        <button class="rc-hud-btn rc-hud-btn-pause" title="Pause recording">
          <span class="rc-hud-btn-icon">&#x23F8;</span> Pause
        </button>
        <button class="rc-hud-btn rc-hud-btn-stop" title="Stop recording">
          <span class="rc-hud-btn-icon">&#x25A0;</span> Stop
        </button>
      </div>
    </div>
  `;

  root.appendChild(hudEl);

  // Drag behavior
  const dragHandle = hudEl.querySelector(".rc-hud-drag") as HTMLElement;
  dragHandle.addEventListener("pointerdown", onDragStart);

  // Pause/Resume button
  hudEl.querySelector(".rc-hud-btn-pause")!.addEventListener("click", () => {
    chrome.runtime.sendMessage({ type: "PAUSE_RECORDING" }).catch(() => {});
  });

  // Stop button
  hudEl.querySelector(".rc-hud-btn-stop")!.addEventListener("click", () => {
    chrome.runtime.sendMessage({ type: "STOP_RECORDING" }).catch(() => {});
  });
}

export function hideHud(): void {
  if (hudEl) {
    hudEl.remove();
    hudEl = null;
  }
}

export function setHudVisible(visible: boolean): void {
  if (hostEl) hostEl.style.display = visible ? "" : "none";
}

export function updateHud(data: { stepCount: number; lastAction?: string; isPaused: boolean }): void {
  if (!hudEl) return;

  const dot = hudEl.querySelector(".rc-hud-dot") as HTMLElement;
  const statusText = hudEl.querySelector(".rc-hud-status-text") as HTMLElement;
  const stepCount = hudEl.querySelector(".rc-hud-step-count") as HTMLElement;
  const lastAction = hudEl.querySelector(".rc-hud-last-action") as HTMLElement;
  const pauseBtn = hudEl.querySelector(".rc-hud-btn-pause") as HTMLElement;

  if (data.isPaused) {
    dot.classList.add("rc-hud-dot-paused");
    statusText.textContent = "Paused";
    pauseBtn.innerHTML = `<span class="rc-hud-btn-icon">&#x25B6;</span> Resume`;
    pauseBtn.onclick = () => {
      chrome.runtime.sendMessage({ type: "RESUME_RECORDING" }).catch(() => {});
    };
  } else {
    dot.classList.remove("rc-hud-dot-paused");
    statusText.textContent = "Recording";
    pauseBtn.innerHTML = `<span class="rc-hud-btn-icon">&#x23F8;</span> Pause`;
    pauseBtn.onclick = () => {
      chrome.runtime.sendMessage({ type: "PAUSE_RECORDING" }).catch(() => {});
    };
  }

  stepCount.textContent = `${data.stepCount} step${data.stepCount !== 1 ? "s" : ""}`;

  if (data.lastAction) {
    lastAction.textContent = data.lastAction;
    lastAction.style.display = "";
  } else {
    lastAction.style.display = "none";
  }
}

// --- Drag ---

function onDragStart(e: PointerEvent): void {
  if (!hudEl) return;
  isDragging = true;
  const rect = hudEl.getBoundingClientRect();
  dragOffsetX = e.clientX - rect.left;
  dragOffsetY = e.clientY - rect.top;
  document.addEventListener("pointermove", onDragMove);
  document.addEventListener("pointerup", onDragEnd);
}

function onDragMove(e: PointerEvent): void {
  if (!isDragging || !hudEl) return;
  const x = Math.max(0, Math.min(e.clientX - dragOffsetX, window.innerWidth - hudEl.offsetWidth));
  const y = Math.max(0, Math.min(e.clientY - dragOffsetY, window.innerHeight - hudEl.offsetHeight));
  hudEl.style.left = `${x}px`;
  hudEl.style.top = `${y}px`;
  hudEl.style.right = "auto";
  hudEl.style.bottom = "auto";
}

function onDragEnd(): void {
  isDragging = false;
  document.removeEventListener("pointermove", onDragMove);
  document.removeEventListener("pointerup", onDragEnd);
}
