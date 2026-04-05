import type { RecordingState, RecordedStep, Highlight, Settings, StepAction } from "../types";
import type { Message } from "../background/messages";
import { generateStepSummary } from "../utils/step-summary";

// DOM elements
const viewSetup = document.getElementById("view-setup")!;
const viewRecording = document.getElementById("view-recording")!;
const viewExport = document.getElementById("view-export")!;
const viewSettings = document.getElementById("view-settings")!;

const tabInfo = document.getElementById("tab-info")!;
const setupGuidance = document.getElementById("setup-guidance") as HTMLDetailsElement;
const inputTitle = document.getElementById("tutorial-title") as HTMLInputElement;
const inputDesc = document.getElementById("tutorial-desc") as HTMLInputElement;
const setupStatus = document.getElementById("setup-status")!;
const btnStart = document.getElementById("btn-start")!;
const btnSettingsToggle = document.getElementById("btn-settings-toggle")!;

const stepCount = document.getElementById("step-count")!;
const baseUrl = document.getElementById("base-url")!;
const stepList = document.getElementById("step-list")!;
const recordingDot = document.getElementById("recording-dot")!;
const recordingStatusText = document.getElementById("recording-status-text")!;
const btnCapture = document.getElementById("btn-capture")!;
const btnPause = document.getElementById("btn-pause")!;
const btnStop = document.getElementById("btn-stop")!;

const vpWidth = document.getElementById("vp-width") as HTMLInputElement;
const vpHeight = document.getElementById("vp-height") as HTMLInputElement;
const exportSummary = document.getElementById("export-summary")!;
const btnExport = document.getElementById("btn-export")!;
const btnCopy = document.getElementById("btn-copy")!;
const btnUploadStepshots = document.getElementById("btn-upload-stepshots")!;
const uploadStatus = document.getElementById("upload-status")!;
const uploadResult = document.getElementById("upload-result")!;
const btnNew = document.getElementById("btn-new")!;

const settingStepshotsUrl = document.getElementById("setting-stepshots-url") as HTMLInputElement;
const settingApiKey = document.getElementById("setting-api-key") as HTMLInputElement;
const btnSettingsSave = document.getElementById("btn-settings-save")!;
const btnSettingsBack = document.getElementById("btn-settings-back")!;

// State
let currentState: RecordingState | null = null;
let exportedJson = "";
let expandedStepId: string | null = null;
let previousStepCount = 0;
let previousView: "setup" | "recording" | "export" = "setup";

// Undo state
let undoState: { step: RecordedStep; index: number } | null = null;
let undoTimer: number | null = null;

// Drag state
let dragFromIndex: number | null = null;
let isDragging = false;

// Undo bar (appended once to the recording view)
const undoBar = document.createElement("div");
undoBar.className = "undo-bar";
undoBar.hidden = true;
undoBar.innerHTML = `<span class="undo-text">Step deleted</span><button class="undo-btn">Undo</button>`;
document.getElementById("view-recording")!.appendChild(undoBar);

// --- Setup guidance (first-time expanded) ---
chrome.storage.local.get("setupGuidanceSeen").then((result) => {
  if (!result.setupGuidanceSeen) {
    setupGuidance.open = true;
  }
});

// --- Tab info ---
function loadTabInfo(): void {
  chrome.tabs.query({ active: true, currentWindow: true }).then(([tab]) => {
    if (!tab) return;
    const favicon = tab.favIconUrl
      ? `<img src="${tab.favIconUrl}" class="tab-favicon" alt="">`
      : `<span class="tab-favicon-placeholder">&#x1F310;</span>`;
    const domain = tab.url ? new URL(tab.url).hostname : "Unknown page";
    const title = tab.title ? escapeHtml(truncate(tab.title, 40)) : "";
    tabInfo.innerHTML = `${favicon}<div class="tab-info-text"><span class="tab-domain">${escapeHtml(domain)}</span>${title ? `<span class="tab-title">${title}</span>` : ""}</div>`;
  });
}

// --- View switching ---
function showView(view: "setup" | "recording" | "export" | "settings"): void {
  viewSetup.hidden = view !== "setup";
  viewRecording.hidden = view !== "recording";
  viewExport.hidden = view !== "export";
  viewSettings.hidden = view !== "settings";

  if (view === "setup") {
    loadTabInfo();
  }
}

function showSetupStatus(message: string, tone: "error" | "progress" | "success" = "error"): void {
  setupStatus.hidden = false;
  setupStatus.textContent = message;
  setupStatus.className = `record-status ${tone}`;
}

function hideSetupStatus(): void {
  setupStatus.hidden = true;
  setupStatus.textContent = "";
  setupStatus.className = "record-status";
}

function showUploadStatus(message: string, tone: "error" | "progress" | "success" = "progress"): void {
  uploadStatus.hidden = false;
  uploadStatus.textContent = message;
  uploadStatus.className = `record-status ${tone}`;
}

function resetUploadFeedback(): void {
  uploadResult.hidden = true;
  uploadResult.innerHTML = "";
  uploadStatus.hidden = true;
  uploadStatus.textContent = "";
  uploadStatus.className = "record-status";
}

function renderState(state: RecordingState): void {
  currentState = state;

  if (state.isRecording) {
    showView("recording");
    baseUrl.textContent = state.baseUrl + state.startPath;
    stepCount.textContent = `${state.steps.length} action${state.steps.length !== 1 ? "s" : ""}`;

    // Pause/resume state in panel
    if (state.isPaused) {
      recordingDot.classList.add("recording-dot-paused");
      recordingStatusText.textContent = "Paused";
      btnPause.textContent = "Resume";
    } else {
      recordingDot.classList.remove("recording-dot-paused");
      recordingStatusText.textContent = "Recording";
      btnPause.textContent = "Pause";
    }

    // Auto-expand newest step if a new step was added
    if (state.steps.length > previousStepCount && state.steps.length > 0) {
      expandedStepId = state.steps[state.steps.length - 1].id;
    }
    if (state.steps.length !== previousStepCount) {
      hideUndoBar();
    }
    previousStepCount = state.steps.length;

    if (!isDragging) {
      renderSteps(state.steps);
    }
  } else if (state.steps.length > 0) {
    showView("export");
    vpWidth.value = String(state.viewport.width);
    vpHeight.value = String(state.viewport.height);

    // Show summary
    exportSummary.innerHTML = `
      <p class="export-title">${escapeHtml(state.tutorialTitle || "Untitled")}</p>
      ${state.tutorialDescription ? `<p class="export-desc">${escapeHtml(state.tutorialDescription)}</p>` : ""}
      <p class="export-steps-count">${state.steps.length} action${state.steps.length !== 1 ? "s" : ""} recorded</p>
    `;

    resetUploadFeedback();
  } else {
    showView("setup");
  }
}

const DEFAULT_ACTIONS: StepAction[] = ["click", "type", "key", "wait", "select"];

function renderSteps(steps: RecordedStep[]): void {
  stepList.innerHTML = "";
  for (let i = 0; i < steps.length; i++) {
    const step = steps[i];
    const item = document.createElement("div");
    item.className = "step-item";
    if (step.id === expandedStepId) item.classList.add("expanded");
    item.dataset.stepId = step.id;
    item.dataset.index = String(i);
    item.draggable = true;

    const summary = generateStepSummary(step);
    const rawPreview = step.selector ?? step.url ?? step.key ?? step.text ?? "";
    const hasHighlight = !!step.highlight?.callout;
    const highlight = step.highlight;
    const position = highlight?.position ?? "bottom";
    const arrow = highlight?.arrow ?? false;
    const callout = highlight?.callout ?? "";
    const isSensitive = !!step.meta?.sensitive;
    const captureMeta = step.meta?.captureOnly
      ? `<div class="step-meta-row"><span class="step-meta-chip">Scene capture</span><span class="step-meta-chip shortcut">Alt+Shift+S</span></div>`
      : "";

    // Action dropdown options
    const availableActions = DEFAULT_ACTIONS.includes(step.action)
      ? DEFAULT_ACTIONS
      : [...DEFAULT_ACTIONS, step.action];
    const actionOptions = availableActions.map(a =>
      `<option value="${a}"${a === step.action ? " selected" : ""}>${a}</option>`
    ).join("");

    // Step editing fields (context-aware)
    let stepFields = `
      <label>Action</label>
      <select data-field="action">${actionOptions}</select>
    `;

    if (step.selector !== undefined || ["click", "type", "hover", "select"].includes(step.action)) {
      stepFields += `
        <label>Selector</label>
        <input type="text" data-field="selector" value="${escapeHtml(step.selector ?? "")}" placeholder="CSS selector">
      `;
    }

    if (step.text !== undefined || step.action === "type") {
      stepFields += `
        <label>Text</label>
        <input type="text" data-field="text" value="${escapeHtml(step.text ?? "")}" placeholder="Typed text">
      `;
    }

    if (step.value !== undefined || step.action === "select") {
      stepFields += `
        <label>Value</label>
        <input type="text" data-field="value" value="${escapeHtml(step.value ?? "")}" placeholder="Selected value">
      `;
    }

    if (step.url !== undefined || step.action === "navigate") {
      stepFields += `
        <label>URL</label>
        <input type="text" data-field="url" value="${escapeHtml(step.url ?? "")}" placeholder="/path">
      `;
    }

    if (step.key !== undefined || step.action === "key") {
      stepFields += `
        <label>Key</label>
        <input type="text" data-field="key" value="${escapeHtml(step.key ?? "")}" placeholder="e.g. Enter, cmd+k">
      `;
    }

    item.innerHTML = `
      <div class="step-summary">
        <span class="drag-handle" title="Drag to reorder">&#x2630;</span>
        <span class="step-number">${i + 1}</span>
        <span class="step-action${isSensitive ? " step-action-sensitive" : ""}">${isSensitive ? "sensitive" : step.action}</span>
        <span class="step-summary-text" title="${escapeHtml(rawPreview)}">${escapeHtml(summary)}</span>
        ${hasHighlight ? '<span class="step-highlight-icon" title="Has highlight">&#9998;</span>' : ""}
        <button class="step-delete" data-step-id="${step.id}" title="Delete step">&times;</button>
      </div>
      <div class="step-detail">
        ${stepFields}
        ${captureMeta}
        <hr class="detail-divider">
        <label>Callout text</label>
        <input type="text" data-field="callout" placeholder="e.g. Click the submit button" value="${escapeHtml(callout)}">
        <label>Position</label>
        <div class="position-picker">
          <button class="position-btn${position === "top" ? " active" : ""}" data-pos="top">Top</button>
          <button class="position-btn${position === "bottom" ? " active" : ""}" data-pos="bottom">Bottom</button>
          <button class="position-btn${position === "left" ? " active" : ""}" data-pos="left">Left</button>
          <button class="position-btn${position === "right" ? " active" : ""}" data-pos="right">Right</button>
        </div>
        <div class="arrow-toggle">
          <input type="checkbox" data-field="arrow" ${arrow ? "checked" : ""}>
          <span>Show arrow</span>
        </div>
        <button class="btn-save-step" data-action="save-step">Save</button>
      </div>
    `;
    stepList.appendChild(item);
  }

  // Auto-scroll to bottom
  stepList.scrollTop = stepList.scrollHeight;
}

// --- Undo ---

function showUndoBar(step: RecordedStep, index: number): void {
  if (undoTimer) clearTimeout(undoTimer);
  undoState = { step, index };
  undoBar.hidden = false;
  undoTimer = window.setTimeout(() => hideUndoBar(), 5000);
}

function hideUndoBar(): void {
  undoBar.hidden = true;
  undoState = null;
  if (undoTimer) { clearTimeout(undoTimer); undoTimer = null; }
}

undoBar.querySelector(".undo-btn")!.addEventListener("click", async () => {
  if (!undoState) return;
  const state = await sendMessage({
    type: "INSERT_STEP",
    step: undoState.step,
    index: undoState.index,
  });
  hideUndoBar();
  if (state) renderState(state as RecordingState);
});

// --- Drag and Drop ---

stepList.addEventListener("dragstart", (e) => {
  const handle = (e.target as HTMLElement).closest(".drag-handle");
  if (!handle) { e.preventDefault(); return; }

  const item = handle.closest(".step-item") as HTMLElement;
  if (!item?.dataset.index) { e.preventDefault(); return; }

  dragFromIndex = parseInt(item.dataset.index, 10);
  isDragging = true;
  item.classList.add("dragging");
  e.dataTransfer!.effectAllowed = "move";
  e.dataTransfer!.setData("text/plain", String(dragFromIndex));
});

stepList.addEventListener("dragover", (e) => {
  e.preventDefault();
  e.dataTransfer!.dropEffect = "move";

  const item = (e.target as HTMLElement).closest(".step-item") as HTMLElement;
  if (!item) return;

  stepList.querySelectorAll(".drop-above, .drop-below").forEach(el => {
    el.classList.remove("drop-above", "drop-below");
  });

  const rect = item.getBoundingClientRect();
  const midY = rect.top + rect.height / 2;
  if (e.clientY < midY) {
    item.classList.add("drop-above");
  } else {
    item.classList.add("drop-below");
  }
});

stepList.addEventListener("dragleave", (e) => {
  const item = (e.target as HTMLElement).closest(".step-item") as HTMLElement;
  if (item) {
    item.classList.remove("drop-above", "drop-below");
  }
});

stepList.addEventListener("dragend", () => {
  isDragging = false;
  dragFromIndex = null;
  stepList.querySelectorAll(".dragging, .drop-above, .drop-below").forEach(el => {
    el.classList.remove("dragging", "drop-above", "drop-below");
  });
});

stepList.addEventListener("drop", async (e) => {
  e.preventDefault();
  if (dragFromIndex === null) return;

  const item = (e.target as HTMLElement).closest(".step-item") as HTMLElement;
  if (!item) return;

  const allItems = Array.from(stepList.querySelectorAll(".step-item"));
  let toIndex = allItems.indexOf(item);

  const rect = item.getBoundingClientRect();
  const midY = rect.top + rect.height / 2;
  if (e.clientY >= midY) toIndex++;

  if (dragFromIndex < toIndex) toIndex--;

  const maxIndex = (currentState?.steps.length ?? 1) - 1;
  toIndex = Math.max(0, Math.min(toIndex, maxIndex));

  isDragging = false;

  if (dragFromIndex !== toIndex) {
    const state = await sendMessage({
      type: "REORDER_STEPS",
      fromIndex: dragFromIndex,
      toIndex,
    });
    if (state) renderState(state as RecordingState);
  } else {
    stepList.querySelectorAll(".dragging, .drop-above, .drop-below").forEach(el => {
      el.classList.remove("dragging", "drop-above", "drop-below");
    });
  }

  dragFromIndex = null;
});

// --- Events ---

// Step list click delegation
stepList.addEventListener("click", async (e) => {
  const target = e.target as HTMLElement;

  if (target.closest(".drag-handle")) return;

  // Delete button
  const stepId = target.dataset.stepId;
  if (stepId && target.classList.contains("step-delete")) {
    const idx = currentState?.steps.findIndex(s => s.id === stepId) ?? -1;
    const deletedStep = idx >= 0 ? currentState?.steps[idx] : undefined;

    const state = await sendMessage({ type: "DELETE_STEP", stepId });
    if (state) renderState(state as RecordingState);

    if (deletedStep && idx >= 0) {
      showUndoBar(deletedStep, idx);
    }
    return;
  }

  // Position picker
  if (target.dataset.pos) {
    const picker = target.closest(".position-picker");
    if (picker) {
      picker.querySelectorAll(".position-btn").forEach((b) => b.classList.remove("active"));
      target.classList.add("active");
    }
    return;
  }

  // Save step
  if (target.dataset.action === "save-step") {
    const item = target.closest(".step-item") as HTMLElement;
    if (!item?.dataset.stepId) return;

    const calloutInput = item.querySelector('[data-field="callout"]') as HTMLInputElement;
    const arrowInput = item.querySelector('[data-field="arrow"]') as HTMLInputElement;
    const activePos = item.querySelector(".position-btn.active") as HTMLElement;
    const actionSelect = item.querySelector('[data-field="action"]') as HTMLSelectElement;
    const selectorInput = item.querySelector('[data-field="selector"]') as HTMLInputElement;
    const textInput = item.querySelector('[data-field="text"]') as HTMLInputElement;
    const valueInput = item.querySelector('[data-field="value"]') as HTMLInputElement;
    const urlInput = item.querySelector('[data-field="url"]') as HTMLInputElement;
    const keyInput = item.querySelector('[data-field="key"]') as HTMLInputElement;

    const highlight: Highlight = {
      callout: calloutInput?.value.trim() || undefined,
      position: (activePos?.dataset.pos as Highlight["position"]) ?? "bottom",
      arrow: arrowInput?.checked ?? false,
    };

    const msg: Record<string, unknown> = {
      type: "UPDATE_STEP",
      stepId: item.dataset.stepId,
      highlight,
    };

    if (actionSelect) msg.action = actionSelect.value;
    if (selectorInput) msg.selector = selectorInput.value;
    if (textInput) msg.text = textInput.value;
    if (valueInput) msg.value = valueInput.value;
    if (urlInput) msg.url = urlInput.value;
    if (keyInput) msg.key = keyInput.value;

    const state = await sendMessage(msg as Message);
    if (state) renderState(state as RecordingState);
    return;
  }

  // Toggle expand/collapse on summary click
  const summaryEl = target.closest(".step-summary") as HTMLElement;
  if (summaryEl && !target.classList.contains("step-delete")) {
    const item = summaryEl.closest(".step-item") as HTMLElement;
    if (!item?.dataset.stepId) return;

    if (expandedStepId === item.dataset.stepId) {
      expandedStepId = null;
    } else {
      expandedStepId = item.dataset.stepId;
    }

    if (currentState) renderSteps(currentState.steps);
  }
});

btnStart.addEventListener("click", async () => {
  const title = inputTitle.value.trim();
  if (!title) {
    inputTitle.style.borderColor = "#d93025";
    showSetupStatus("Add a title before you start recording.");
    return;
  }
  inputTitle.style.borderColor = "";
  hideSetupStatus();

  previousStepCount = 0;
  expandedStepId = null;
  hideUndoBar();

  // Mark guidance as seen
  chrome.storage.local.set({ setupGuidanceSeen: true });

  const state = await sendMessage({
    type: "START_RECORDING",
    tutorialName: slugify(title),
    tutorialTitle: title,
    tutorialDescription: inputDesc.value.trim(),
  });

  if (state && !("error" in state)) {
    renderState(state as RecordingState);
  } else if (state?.error) {
    showSetupStatus(state.error);
  }
});

btnPause.addEventListener("click", async () => {
  if (!currentState) return;
  const msgType = currentState.isPaused ? "RESUME_RECORDING" : "PAUSE_RECORDING";
  const state = await sendMessage({ type: msgType });
  if (state) renderState(state as RecordingState);
});

btnCapture.addEventListener("click", async () => {
  await chrome.runtime.sendMessage({ type: "CAPTURE_SCREEN" });
});

btnStop.addEventListener("click", async () => {
  hideUndoBar();
  const state = await sendMessage({ type: "STOP_RECORDING" });
  if (state) renderState(state as RecordingState);
});

btnExport.addEventListener("click", async () => {
  const result = await sendMessage({
    type: "EXPORT_CONFIG",
    viewport: {
      width: parseInt(vpWidth.value) || 1280,
      height: parseInt(vpHeight.value) || 800,
    },
  });

  if (result?.json) {
    exportedJson = result.json;
    downloadJson(exportedJson, "stepshots.config.json");
  }
});

btnCopy.addEventListener("click", async () => {
  if (!exportedJson) {
    const result = await sendMessage({
      type: "EXPORT_CONFIG",
      viewport: {
        width: parseInt(vpWidth.value) || 1280,
        height: parseInt(vpHeight.value) || 800,
      },
    });
    if (result?.json) exportedJson = result.json;
  }

  if (exportedJson) {
    await navigator.clipboard.writeText(exportedJson);
    btnCopy.textContent = "Copied!";
    setTimeout(() => { btnCopy.textContent = "Copy config"; }, 1500);
  }
});

btnUploadStepshots.addEventListener("click", async () => {
  // Check for API key first
  const settingsResult = await sendMessage({ type: "GET_SETTINGS" });
  if (!settingsResult?.apiKey) {
    uploadStatus.hidden = false;
    uploadStatus.innerHTML = `Add an API key in <a href="#" id="upload-go-settings" class="status-link">Settings</a> to upload directly. You can still download the config below.`;
    uploadStatus.className = "record-status error";
    uploadStatus.querySelector("#upload-go-settings")?.addEventListener("click", (e) => {
      e.preventDefault();
      previousView = "export";
      showView("settings");
      sendMessage({ type: "GET_SETTINGS" }).then((s: Settings) => {
        if (s) {
          settingStepshotsUrl.value = s.stepshotsUrl;
          settingApiKey.value = s.apiKey || "";
        }
      });
    });
    return;
  }

  resetUploadFeedback();
  showUploadStatus("Preparing your demo upload…");
  btnUploadStepshots.setAttribute("disabled", "true");

  try {
    const result = await sendMessage({
      type: "UPLOAD_TO_STEPSHOTS",
      viewport: {
        width: parseInt(vpWidth.value) || 1280,
        height: parseInt(vpHeight.value) || 800,
      },
    });

    if (result?.ok && result.editorUrl) {
      showUploadStatus("Upload complete. Opening your demo in the editor…", "success");
      uploadResult.hidden = false;
      uploadResult.innerHTML = `
        <p class="upload-success-text">Upload complete.</p>
        <a href="${result.editorUrl}" class="upload-editor-link" target="_blank">Open in editor &rarr;</a>
      `;
      uploadResult.querySelector(".upload-editor-link")?.addEventListener("click", (e) => {
        e.preventDefault();
        chrome.tabs.create({ url: result.editorUrl });
      });
    } else {
      showUploadStatus(result?.error || "Upload failed.", "error");
    }
  } catch {
    showUploadStatus("Upload failed. Check your connection and try again.", "error");
  } finally {
    btnUploadStepshots.removeAttribute("disabled");
  }
});

btnNew.addEventListener("click", () => {
  currentState = null;
  exportedJson = "";
  previousStepCount = 0;
  expandedStepId = null;
  hideUndoBar();
  hideSetupStatus();
  inputTitle.value = "";
  inputDesc.value = "";
  resetUploadFeedback();
  showView("setup");
});

// --- Settings ---

btnSettingsToggle.addEventListener("click", () => {
  previousView = viewSetup.hidden ? (viewExport.hidden ? "recording" : "export") : "setup";
  showView("settings");
  sendMessage({ type: "GET_SETTINGS" }).then((s: Settings) => {
    if (s) {
      settingStepshotsUrl.value = s.stepshotsUrl;
      settingApiKey.value = s.apiKey || "";
    }
  });
});

btnSettingsSave.addEventListener("click", async () => {
  await sendMessage({
    type: "SAVE_SETTINGS",
    settings: {
      stepshotsUrl: settingStepshotsUrl.value.trim() || "https://stepshots.com",
      apiKey: settingApiKey.value.trim() || undefined,
      cliServerUrl: "http://localhost:8124",
    },
  });
  btnSettingsSave.textContent = "Saved!";
  setTimeout(() => { btnSettingsSave.textContent = "Save Settings"; }, 1500);
});

btnSettingsBack.addEventListener("click", () => {
  if (currentState && currentState.steps.length > 0 && !currentState.isRecording) {
    showView("export");
  } else if (currentState?.isRecording) {
    showView("recording");
  } else {
    showView("setup");
  }
});

// Listen for state updates from background
chrome.runtime.onMessage.addListener((message: Message) => {
  if (message.type === "STATE_UPDATE") {
    renderState(message.state);
  } else if (message.type === "UPLOAD_PROGRESS") {
    showUploadStatus(message.message, message.stage === "finalize" ? "success" : "progress");
  }
});

// --- Helpers ---

function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

async function sendMessage(message: Message | Record<string, unknown>): Promise<any> {
  return chrome.runtime.sendMessage(message);
}

function downloadJson(json: string, filename: string): void {
  const blob = new Blob([json], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

function escapeHtml(str: string): string {
  return str.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function truncate(str: string, max: number): string {
  return str.length > max ? str.slice(0, max) + "..." : str;
}

// --- Init ---
sendMessage({ type: "GET_STATE" }).then((state) => {
  if (state) renderState(state as RecordingState);
}).catch(() => {});
