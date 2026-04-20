import { useEffect, useState } from "preact/hooks";
import { generateStepSummary } from "../../utils/step-summary";
import {
  recordingState,
  sendMessage,
  uploadResult,
  uploadStatus,
  viewOverride,
} from "../store";

let cachedJson = "";

export function Export() {
  const state = recordingState.value!;
  const [title, setTitle] = useState(state.tutorialTitle || "");
  const [desc, setDesc] = useState(state.tutorialDescription || "");
  const [vpWidth, setVpWidth] = useState(String(state.viewport.width));
  const [vpHeight, setVpHeight] = useState(String(state.viewport.height));
  const [uploading, setUploading] = useState(false);
  const [copyLabel, setCopyLabel] = useState("Copy config");

  // Reset upload feedback when steps or viewport change (new state arrives).
  useEffect(() => {
    uploadStatus.value = null;
    uploadResult.value = null;
    cachedJson = "";
  }, [state.steps.length]);

  // Push title/desc edits to background, debounced — keystrokes don't get clobbered.
  useEffect(() => {
    const timer = setTimeout(() => {
      sendMessage({
        type: "UPDATE_TUTORIAL_META",
        tutorialTitle: title.trim() || "Untitled",
        tutorialDescription: desc.trim(),
      });
    }, 300);
    return () => clearTimeout(timer);
  }, [title, desc]);

  const viewportPayload = () => ({
    width: parseInt(vpWidth) || 1280,
    height: parseInt(vpHeight) || 800,
  });

  const exportConfig = async () => {
    const result = await sendMessage({ type: "EXPORT_CONFIG", viewport: viewportPayload() });
    if (result?.json) {
      cachedJson = result.json;
      downloadJson(cachedJson, "stepshots.config.json");
    }
  };

  const copyConfig = async () => {
    if (!cachedJson) {
      const result = await sendMessage({ type: "EXPORT_CONFIG", viewport: viewportPayload() });
      if (result?.json) cachedJson = result.json;
    }
    if (cachedJson) {
      await navigator.clipboard.writeText(cachedJson);
      setCopyLabel("Copied!");
      setTimeout(() => setCopyLabel("Copy config"), 1500);
    }
  };

  const upload = async () => {
    const settingsResult = await sendMessage({ type: "GET_SETTINGS" });
    if (!settingsResult?.apiKey) {
      uploadStatus.value = {
        message: "Add an API key in Settings to upload directly. You can still download the config below.",
        tone: "error",
        needsApiKey: true,
      };
      return;
    }

    uploadResult.value = null;
    uploadStatus.value = { message: "Preparing your demo upload…", tone: "progress" };
    setUploading(true);
    try {
      const result = await sendMessage({
        type: "UPLOAD_TO_STEPSHOTS",
        viewport: viewportPayload(),
      });
      if (result?.ok && result.editorUrl) {
        uploadStatus.value = {
          message: "Upload complete. Opening your demo in the editor…",
          tone: "success",
        };
        uploadResult.value = { editorUrl: result.editorUrl };
      } else {
        uploadStatus.value = { message: result?.error || "Upload failed.", tone: "error" };
      }
    } catch {
      uploadStatus.value = {
        message: "Upload failed. Check your connection and try again.",
        tone: "error",
      };
    } finally {
      setUploading(false);
    }
  };

  const startNew = () => {
    recordingState.value = null;
    cachedJson = "";
    uploadStatus.value = null;
    uploadResult.value = null;
  };

  const status = uploadStatus.value;
  const result = uploadResult.value;

  return (
    <div>
      <h1>Finalize</h1>
      <label for="export-title">Title</label>
      <input
        id="export-title"
        type="text"
        value={title}
        placeholder="Untitled"
        onInput={(e) => setTitle((e.target as HTMLInputElement).value)}
      />
      <label for="export-desc">Description (optional)</label>
      <input
        id="export-desc"
        type="text"
        value={desc}
        placeholder="Add a short description"
        onInput={(e) => setDesc((e.target as HTMLInputElement).value)}
      />
      <p class="export-steps-count">
        {state.steps.length} action{state.steps.length !== 1 ? "s" : ""} recorded
      </p>
      <div class="export-step-preview">
        {state.steps.length === 0 ? (
          <div class="export-step-preview-empty">No steps recorded.</div>
        ) : (
          state.steps.map((step, i) => {
            const summary = generateStepSummary(step);
            return (
              <div class="export-step-preview-item" key={step.id}>
                <span class="step-number">{i + 1}</span>
                <span class="step-text" title={summary}>
                  {summary}
                </span>
              </div>
            );
          })
        )}
      </div>
      <p class="panel-intro panel-intro-compact">
        Recommended: upload to Stepshots, then review callouts and sharing in the editor.
      </p>
      <button class="btn btn-primary btn-lg" onClick={upload} disabled={uploading}>
        Upload to Stepshots
      </button>
      {result && (
        <div class="upload-result">
          <p class="upload-success-text">Upload complete.</p>
          <a
            class="upload-editor-link"
            href={result.editorUrl}
            onClick={(e) => {
              e.preventDefault();
              chrome.tabs.create({ url: result.editorUrl });
            }}
          >
            Open in editor &rarr;
          </a>
        </div>
      )}
      {status && (
        <p class={`record-status ${status.tone}`}>
          {status.needsApiKey ? (
            <>
              Add an API key in{" "}
              <a
                href="#"
                class="status-link"
                onClick={(e) => {
                  e.preventDefault();
                  viewOverride.value = "settings";
                }}
              >
                Settings
              </a>{" "}
              to upload directly. You can still download the config below.
            </>
          ) : (
            status.message
          )}
        </p>
      )}
      <details class="advanced-section">
        <summary>Advanced options</summary>
        <div class="advanced-body">
          <div class="viewport-row">
            <div>
              <label for="vp-width">Width</label>
              <input
                id="vp-width"
                type="number"
                value={vpWidth}
                min={1}
                onInput={(e) => setVpWidth((e.target as HTMLInputElement).value)}
              />
            </div>
            <div>
              <label for="vp-height">Height</label>
              <input
                id="vp-height"
                type="number"
                value={vpHeight}
                min={1}
                onInput={(e) => setVpHeight((e.target as HTMLInputElement).value)}
              />
            </div>
          </div>
          <p class="meta">Viewport size for screenshot rendering. Defaults to captured dimensions.</p>
          <div class="export-actions">
            <button class="btn" onClick={exportConfig}>
              Download config
            </button>
            <button class="btn" onClick={copyConfig}>
              {copyLabel}
            </button>
          </div>
        </div>
      </details>
      <button class="btn" onClick={startNew}>
        Start new recording
      </button>
    </div>
  );
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
