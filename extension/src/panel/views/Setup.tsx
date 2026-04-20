import { useEffect, useState } from "preact/hooks";
import { recordingState, sendMessage, setupError, slugify, tabInfo, viewOverride } from "../store";

export function Setup() {
  const [title, setTitle] = useState("");
  const [desc, setDesc] = useState("");
  const [guidanceOpen, setGuidanceOpen] = useState(false);

  useEffect(() => {
    chrome.storage.local.get("setupGuidanceSeen").then((r) => {
      if (!r.setupGuidanceSeen) setGuidanceOpen(true);
    });
    chrome.tabs.query({ active: true, currentWindow: true }).then(([tab]) => {
      if (!tab) return;
      tabInfo.value = {
        favicon: tab.favIconUrl,
        domain: tab.url ? new URL(tab.url).hostname : "Unknown page",
        title: tab.title ? truncate(tab.title, 40) : "",
      };
    });
  }, []);

  const start = async () => {
    setupError.value = null;
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    const finalTitle = title.trim() || tab?.title?.trim() || "Untitled";

    if (tab?.url && !tab.url.startsWith("chrome://") && !tab.url.startsWith("chrome-extension://")) {
      const origin = new URL(tab.url).origin + "/*";
      const granted = await chrome.permissions.request({ origins: [origin] });
      if (!granted) {
        setupError.value = "Permission denied. The extension needs access to this site to record.";
        return;
      }
    }

    chrome.storage.local.set({ setupGuidanceSeen: true });

    const state = await sendMessage({
      type: "START_RECORDING",
      tutorialName: slugify(finalTitle),
      tutorialTitle: finalTitle,
      tutorialDescription: desc.trim(),
    });

    if (state && !("error" in state)) {
      recordingState.value = state;
    } else if (state?.error) {
      setupError.value = state.error;
    }
  };

  const ti = tabInfo.value;

  return (
    <div>
      <h1>Stepshots Recorder</h1>
      <p class="panel-intro">Record a flow here, then upload it to polish and share in the app.</p>
      {ti && (
        <div class="tab-info">
          {ti.favicon ? (
            <img src={ti.favicon} class="tab-favicon" alt="" />
          ) : (
            <span class="tab-favicon-placeholder">&#x1F310;</span>
          )}
          <div class="tab-info-text">
            <span class="tab-domain">{ti.domain}</span>
            {ti.title && <span class="tab-title">{ti.title}</span>}
          </div>
        </div>
      )}
      <label for="tutorial-title">Title (optional)</label>
      <input
        id="tutorial-title"
        type="text"
        value={title}
        onInput={(e) => setTitle((e.target as HTMLInputElement).value)}
        placeholder="Defaults to the page title — you can rename later"
      />
      <label for="tutorial-desc">Description (optional)</label>
      <input
        id="tutorial-desc"
        type="text"
        value={desc}
        onInput={(e) => setDesc((e.target as HTMLInputElement).value)}
        placeholder="e.g. How to create an account"
      />
      {setupError.value && <p class="record-status error">{setupError.value}</p>}
      <button class="btn btn-primary" onClick={start}>
        Start Recording
      </button>
      <details class="setup-guidance" open={guidanceOpen}>
        <summary>Recording tips</summary>
        <div class="setup-guidance-body">
          <p class="guidance-heading">What gets recorded</p>
          <ul>
            <li>Clicks, typing, and captured screens</li>
            <li>Keyboard shortcuts and dropdown selections</li>
          </ul>
          <p class="guidance-heading">Privacy</p>
          <ul>
            <li>Password fields and sensitive inputs are never recorded</li>
          </ul>
          <p class="guidance-heading">Tips</p>
          <ul>
            <li>Navigate to the starting page before recording</li>
            <li>Each interaction becomes a step you can annotate</li>
            <li>
              Use <strong>Capture Screen</strong> or <strong>Alt+Shift+S</strong> for scene-only steps
            </li>
            <li>Use the in-page HUD to pause or stop recording</li>
          </ul>
        </div>
      </details>
      <button class="btn btn-link" onClick={() => (viewOverride.value = "settings")}>
        Settings
      </button>
    </div>
  );
}

function truncate(str: string, max: number): string {
  return str.length > max ? str.slice(0, max) + "..." : str;
}
