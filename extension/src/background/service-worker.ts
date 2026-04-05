import type { Message } from "./messages";
import { type RecordingState, type Settings, DEFAULT_STATE, DEFAULT_SETTINGS } from "../types";
import { buildConfig } from "../utils/config-builder";
import { buildBundle } from "./bundle-builder";
import { generateStepSummary } from "../utils/step-summary";

let state: RecordingState = { ...DEFAULT_STATE };
let settings: Settings = { ...DEFAULT_SETTINGS };

// In-memory screenshot store (too large for chrome.storage.session)
const screenshots = new Map<string, string>(); // stepId -> data URL
const pendingCaptures = new Set<Promise<void>>();

// Persist state to session storage for service worker restarts
async function saveState(): Promise<void> {
  await chrome.storage.session.set({ recordingState: state });
}

async function loadState(): Promise<void> {
  const result = await chrome.storage.session.get("recordingState");
  if (result.recordingState) {
    state = result.recordingState as RecordingState;
  }
}

async function loadSettings(): Promise<void> {
  const result = await chrome.storage.sync.get("settings");
  if (result.settings) {
    settings = result.settings as Settings;
  }
}

async function saveSettings(): Promise<void> {
  await chrome.storage.sync.set({ settings });
}

// Broadcast state to popup
function broadcastState(): void {
  chrome.runtime.sendMessage({ type: "STATE_UPDATE", state }).catch(() => {
    // Popup may not be open — ignore
  });
}

function broadcastUploadProgress(
  stage: "bundle" | "upload" | "finalize",
  message: string,
): void {
  chrome.runtime.sendMessage({ type: "UPLOAD_PROGRESS", stage, message }).catch(() => {
    // Popup may not be open — ignore
  });
}

// Send HUD update to the content script
function sendHudUpdate(): void {
  if (!state.recordingTabId) return;
  const lastStep = state.steps[state.steps.length - 1];
  const msg: Message = {
    type: "HUD_UPDATE",
    stepCount: state.steps.length,
    lastAction: lastStep ? generateStepSummary(lastStep) : undefined,
    isPaused: state.isPaused,
  };
  chrome.tabs.sendMessage(state.recordingTabId, msg).catch(() => {});
}

// Capture a screenshot of the recording tab
function captureScreenshot(key: string, delayMs: number): Promise<void> {
  const task = (async () => {
    if (!state.isRecording || !state.recordingTabId) return;

    await new Promise((r) => setTimeout(r, delayMs));

    if (!state.isRecording || !state.recordingTabId) return;

    try {
      // Ask the content script to hide HUD/toast and wait for repaint confirmation
      await chrome.tabs.sendMessage(state.recordingTabId, { type: "HIDE_OVERLAYS" });

      const dataUrl = await chrome.tabs.captureVisibleTab(undefined, { format: "jpeg", quality: 90 });
      if (dataUrl) {
        screenshots.set(key, dataUrl);
      }

      // Restore HUD/toast
      chrome.tabs.sendMessage(state.recordingTabId, { type: "SHOW_OVERLAYS" }).catch(() => {});
    } catch (err) {
      console.warn("Screenshot capture failed for", key, err);
    }
  })();

  pendingCaptures.add(task);
  task.finally(() => pendingCaptures.delete(task));
  return task;
}

async function waitForPendingCaptures(): Promise<void> {
  while (pendingCaptures.size > 0) {
    await Promise.allSettled(Array.from(pendingCaptures));
  }
}

// URLs where content scripts cannot be injected
function isRestrictedUrl(url: string | undefined): boolean {
  if (!url) return true;
  return url.startsWith("chrome://") || url.startsWith("chrome-extension://")
    || url.startsWith("about:") || url.startsWith("chrome-search://");
}

// Ensure content script is injected, then send a message
async function ensureContentScript(tabId: number): Promise<void> {
  const tab = await chrome.tabs.get(tabId);
  if (isRestrictedUrl(tab.url)) {
    console.warn("Cannot inject content script into restricted URL:", tab.url);
    return;
  }

  try {
    await chrome.tabs.sendMessage(tabId, { type: "GET_STATE" });
  } catch {
    await chrome.scripting.executeScript({
      target: { tabId },
      files: ["dist/content.js"],
    });
  }
}

async function sendToContentScript(tabId: number, message: Message): Promise<any> {
  try {
    return await chrome.tabs.sendMessage(tabId, message);
  } catch {
    console.warn("Failed to send message to content script in tab", tabId);
    return undefined;
  }
}

// Get the active tab
async function getActiveTab(): Promise<chrome.tabs.Tab | null> {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  return tab ?? null;
}

// Handle messages from popup and content script
chrome.runtime.onMessage.addListener(
  (message: Message, sender, sendResponse) => {
    handleMessage(message, sender).then(sendResponse);
    return true; // Keep message channel open for async response
  }
);

async function handleMessage(
  message: Message,
  sender?: chrome.runtime.MessageSender,
): Promise<unknown> {
  switch (message.type) {
    case "GET_STATE": {
      return state;
    }

    case "GET_SETTINGS": {
      return settings;
    }

    case "SAVE_SETTINGS": {
      settings = message.settings;
      await saveSettings();
      return settings;
    }

    case "START_RECORDING": {
      const tab = await getActiveTab();
      if (!tab?.url || !tab.id) return { error: "No active tab" };

      if (isRestrictedUrl(tab.url)) {
        return { error: "Cannot record on chrome:// or internal pages. Navigate to a website first." };
      }

      const url = new URL(tab.url);
      screenshots.clear();
      await ensureContentScript(tab.id);
      // Activate content script and get actual viewport dimensions
      const activateResponse = await sendToContentScript(tab.id, { type: "ACTIVATE_CONTENT_SCRIPT" });
      const actualViewport = activateResponse?.viewport ?? { width: 1280, height: 800 };

      state = {
        isRecording: true,
        isPaused: false,
        tutorialName: message.tutorialName,
        tutorialTitle: message.tutorialTitle,
        tutorialDescription: message.tutorialDescription,
        baseUrl: url.origin,
        startPath: url.pathname + url.search,
        steps: [],
        viewport: actualViewport,
        recordingTabId: tab.id,
      };
      await saveState();
      broadcastState();

      return state;
    }

    case "STOP_RECORDING": {
      if (state.recordingTabId) {
        await sendToContentScript(state.recordingTabId, { type: "DEACTIVATE_CONTENT_SCRIPT" });
        // Deactivation flushes the buffered type step in the content script.
        // Give that message a turn to reach the worker before closing recording.
        await new Promise((r) => setTimeout(r, 50));
      }
      await waitForPendingCaptures();
      state.isRecording = false;
      state.isPaused = false;
      state.recordingTabId = undefined;
      await saveState();
      broadcastState();
      return state;
    }

    case "PAUSE_RECORDING": {
      if (!state.isRecording || state.isPaused) return state;
      state.isPaused = true;
      await saveState();
      if (state.recordingTabId) {
        await sendToContentScript(state.recordingTabId, { type: "PAUSE_CONTENT_SCRIPT" });
      }
      broadcastState();
      sendHudUpdate();
      return state;
    }

    case "RESUME_RECORDING": {
      if (!state.isRecording || !state.isPaused) return state;
      state.isPaused = false;
      await saveState();
      if (state.recordingTabId) {
        await sendToContentScript(state.recordingTabId, { type: "RESUME_CONTENT_SCRIPT" });
      }
      broadcastState();
      sendHudUpdate();
      return state;
    }

    case "CAPTURE_SCREEN": {
      if (!state.isRecording || state.isPaused) return state;

      const step = {
        id: crypto.randomUUID(),
        action: "wait" as const,
        meta: { captureOnly: true },
        timestamp: Date.now(),
      };

      state.steps.push(step);
      await saveState();
      broadcastState();
      sendHudUpdate();

      captureScreenshot(step.id, 100);
      return state;
    }

    case "CAPTURE_STEP_SCREENSHOT": {
      if (!state.isRecording || state.isPaused) return state;
      if (state.recordingTabId && sender?.tab?.id !== state.recordingTabId) return state;
      await captureScreenshot(message.stepId, 0);
      return { ok: true };
    }

    case "STEP_RECORDED": {
      if (!state.isRecording || state.isPaused) return;
      // Only accept steps from the recording tab
      if (state.recordingTabId && sender?.tab?.id !== state.recordingTabId) return;
      state.steps.push(message.step);
      await saveState();
      broadcastState();
      sendHudUpdate();

      // Capture screenshot after the action settles when the content script
      // did not already request a pre-action capture for this step.
      if (!screenshots.has(message.step.id)) {
        const delay = message.step.action === "type" ? 100 : 200;
        captureScreenshot(message.step.id, delay);
      }

      return state;
    }

    case "UPDATE_STEP": {
      const step = state.steps.find((s) => s.id === message.stepId);
      if (!step) return state;

      if (message.highlight !== undefined) step.highlight = message.highlight;
      if (message.action !== undefined) step.action = message.action;
      if (message.selector !== undefined) step.selector = message.selector || undefined;
      if (message.text !== undefined) step.text = message.text || undefined;
      if (message.value !== undefined) step.value = message.value || undefined;
      if (message.url !== undefined) step.url = message.url || undefined;
      if (message.key !== undefined) step.key = message.key || undefined;

      await saveState();
      broadcastState();
      return state;
    }

    case "DELETE_STEP": {
      screenshots.delete(message.stepId);
      state.steps = state.steps.filter((s) => s.id !== message.stepId);
      await saveState();
      broadcastState();
      return state;
    }

    case "INSERT_STEP": {
      const idx = Math.min(message.index, state.steps.length);
      state.steps.splice(idx, 0, message.step);
      await saveState();
      broadcastState();
      return state;
    }

    case "REORDER_STEPS": {
      const { fromIndex, toIndex } = message;
      if (fromIndex < 0 || fromIndex >= state.steps.length) return state;
      if (toIndex < 0 || toIndex >= state.steps.length) return state;
      const [moved] = state.steps.splice(fromIndex, 1);
      state.steps.splice(toIndex, 0, moved);
      await saveState();
      broadcastState();
      return state;
    }

    case "EXPORT_CONFIG": {
      state.viewport = {
        ...message.viewport,
        deviceScaleFactor: state.viewport.deviceScaleFactor,
      };
      const config = buildConfig(state);
      const json = JSON.stringify(config, null, 2);
      return { json };
    }

    case "UPLOAD_TO_STEPSHOTS": {
      state.viewport = {
        ...message.viewport,
        deviceScaleFactor: state.viewport.deviceScaleFactor,
      };
      const stepshotsUrl = settings.stepshotsUrl.replace(/\/$/, "");

      if (!settings.apiKey) {
        return { error: "No API key set. Go to Settings and add your API key." };
      }

      if (screenshots.size === 0) {
        return { error: "No screenshots captured. Please start a new recording." };
      }

      await waitForPendingCaptures();
      return await directUpload(state, stepshotsUrl, settings.apiKey);
    }

    default:
      return null;
  }
}

// Direct upload: build .stepshot bundle in-browser and upload to SaaS
async function directUpload(
  recordingState: RecordingState,
  stepshotsUrl: string,
  apiKey: string,
): Promise<unknown> {
  try {
    broadcastUploadProgress("bundle", "Packaging your recording into a .stepshot bundle…");
    const bundleBytes = buildBundle(recordingState, screenshots, recordingState.viewport);

    const formData = new FormData();
    formData.append("title", recordingState.tutorialTitle || "Untitled");
    if (recordingState.tutorialDescription) {
      formData.append("description", recordingState.tutorialDescription);
    }
    formData.append("bundle", new Blob([bundleBytes], { type: "application/zip" }), "bundle.stepshot");

    broadcastUploadProgress("upload", "Uploading your demo to Stepshots…");
    const res = await fetch(`${stepshotsUrl}/api/demos/upload-bundle`, {
      method: "POST",
      headers: { Authorization: `Bearer ${apiKey}` },
      body: formData,
    });

    if (!res.ok) {
      const text = await res.text();
      if (res.status === 401) {
        return { error: "Invalid API key. Check your key in Settings." };
      }
      return { error: text || `Upload failed (${res.status})` };
    }

    const data = await res.json();
    broadcastUploadProgress("finalize", "Upload complete. Opening your demo in the editor…");
    return {
      ok: true,
      demoId: data.id,
      editorUrl: `${stepshotsUrl}/dashboard/demos/${data.id}/edit`,
    };
  } catch (err) {
    return { error: `Upload failed: ${err}` };
  }
}

// Open side panel when extension icon is clicked
chrome.sidePanel.setPanelBehavior({ openPanelOnActionClick: true });

// Initialize state and settings on service worker start
loadState();
loadSettings();
