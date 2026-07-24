import { useEffect, useState } from "preact/hooks";
import { generateStepSummary } from "../../utils/step-summary";
import type { RecordingState } from "../../types";
import {
  recordingState,
  sendMessage,
  settings as settingsSignal,
  uploadResult,
  uploadStatus,
  viewOverride,
} from "../store";

let cachedJson = "";

const TOUR_SCHEMA_URL =
  "https://raw.githubusercontent.com/hauju/stepshots/main/schema/tour.schema.json";

/** Recorded action → guided-tour advance kind. Anything else is a setup step. */
const TOUR_ADVANCE: Record<string, string> = { click: "click", type: "input", select: "change" };

/**
 * Project the recording into a guided-tour source file (*.tour.json) — the
 * same convention as `stepshots tour init --from`: a step joins the tour only
 * when it's interactive (click/type/select), has a selector, and carries a
 * callout. Drift anchors come from the captured element identity, skipped for
 * sensitive fields.
 */
function buildTourFile(
  state: RecordingState,
  title: string,
): { json: string; included: number; missingCallouts: number } {
  const steps = [];
  let missingCallouts = 0;
  for (const step of state.steps) {
    const advance = TOUR_ADVANCE[step.action];
    if (!advance || !step.selector) continue;
    const body = step.highlight?.callout?.trim();
    if (!body) {
      missingCallouts++;
      continue;
    }
    const fallback: { text?: string; aria?: string } = {};
    if (!step.meta?.sensitive) {
      const text = step.meta?.elementText?.replace(/\s+/g, " ").trim().slice(0, 120);
      if (text) fallback.text = text;
      if (step.meta?.ariaLabel) fallback.aria = step.meta.ariaLabel;
    }
    steps.push({
      selector: step.selector,
      title: generateStepSummary(step),
      body,
      advance: { type: advance },
      ...(fallback.text || fallback.aria ? { fallback } : {}),
    });
  }
  const key = slugify(title);
  const file = {
    $schema: TOUR_SCHEMA_URL,
    schema: "1",
    key,
    title: title || "Untitled",
    steps,
  };
  return { json: JSON.stringify(file, null, 2) + "\n", included: steps.length, missingCallouts };
}

function slugify(title: string): string {
  const slug = title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60);
  return slug || "tour";
}

export function Export() {
  const state = recordingState.value!;
  const [title, setTitle] = useState(state.tutorialTitle || "");
  const [desc, setDesc] = useState(state.tutorialDescription || "");
  const [vpWidth, setVpWidth] = useState(String(state.viewport.width));
  const [vpHeight, setVpHeight] = useState(String(state.viewport.height));
  const [uploading, setUploading] = useState(false);
  const [copyLabel, setCopyLabel] = useState("Copy config");
  const [tourNote, setTourNote] = useState("");

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

  const exportTour = () => {
    const { json, included, missingCallouts } = buildTourFile(state, title.trim() || "Untitled");
    if (included === 0) {
      setTourNote(
        missingCallouts > 0
          ? "No tour-worthy steps yet — add callout text to your click/type steps in the step list; callouts become the tour's instructions."
          : "No tour-worthy steps — a tour needs click, type, or select actions.",
      );
      return;
    }
    const skipped =
      missingCallouts > 0
        ? ` ${missingCallouts} interactive step${missingCallouts !== 1 ? "s" : ""} without callout text ${missingCallouts !== 1 ? "were" : "was"} left out.`
        : "";
    setTourNote(
      `Tour file with ${included} step${included !== 1 ? "s" : ""} downloaded — keep it in tours/ in your repo, then run \`stepshots tour check\` and \`stepshots tour push\`.${skipped}`,
    );
    downloadJson(json, `${slugify(title.trim() || "Untitled")}.tour.json`);
  };

  const upload = async () => {
    // Read settings synchronously from the signal (pre-loaded at panel boot).
    // Doing this without awaits is required so chrome.permissions.request below
    // still has the click's user-gesture token.
    const currentSettings = settingsSignal.value;
    if (!currentSettings?.apiKey) {
      uploadStatus.value = {
        message: "Add an API key in Settings to upload directly. You can still download the config below.",
        tone: "error",
        needsApiKey: true,
      };
      return;
    }

    // If the configured Stepshots URL isn't one of the manifest-declared hosts,
    // ensure we have runtime host permission for it. Without this, the service
    // worker's fetch falls back to CORS and fails with "Failed to fetch".
    const stepshotsUrl = currentSettings.stepshotsUrl || "https://stepshots.com";
    if (!/^https:\/\/(.*\.)?stepshots\.com\/?$/i.test(stepshotsUrl)) {
      let originPattern: string;
      try {
        originPattern = new URL(stepshotsUrl).origin + "/*";
      } catch {
        uploadStatus.value = { message: `Invalid Stepshots URL: ${stepshotsUrl}`, tone: "error" };
        return;
      }
      const granted = await chrome.permissions.request({ origins: [originPattern] });
      if (!granted) {
        uploadStatus.value = {
          message: `Permission required for ${new URL(stepshotsUrl).origin}. Click Upload again and accept the Chrome prompt.`,
          tone: "error",
        };
        return;
      }
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
  // Tour-first finalize: show live eligibility so missing callouts are visible
  // before downloading (steps are only editable back in the Recording view).
  const tourMode = !!state.tourMode;
  const tourEligibility = tourMode ? buildTourFile(state, title.trim() || "Untitled") : null;

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
      {tourMode && tourEligibility ? (
        <>
          <p class="panel-intro panel-intro-compact">
            Recommended: download the tour file and keep it in <code>tours/</code> in your repo,
            then run <code>stepshots tour push</code>.
          </p>
          <p class="meta">
            {tourEligibility.included} step{tourEligibility.included !== 1 ? "s" : ""} make the tour
            {tourEligibility.missingCallouts > 0
              ? ` · ${tourEligibility.missingCallouts} interactive step${tourEligibility.missingCallouts !== 1 ? "s" : ""} missing callout text will be left out`
              : ""}
          </p>
          <button class="btn btn-primary btn-lg" onClick={exportTour}>
            Download tour file
          </button>
          {tourNote && <p class="meta">{tourNote}</p>}
          <button class="btn" onClick={upload} disabled={uploading || !!result}>
            Upload demo to Stepshots
          </button>
        </>
      ) : (
        <>
          <p class="panel-intro panel-intro-compact">
            Recommended: upload to Stepshots, then review callouts and sharing in the editor.
          </p>
          <button class="btn btn-primary btn-lg" onClick={upload} disabled={uploading || !!result}>
            Upload to Stepshots
          </button>
        </>
      )}
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
            {!tourMode && (
              <button class="btn" onClick={exportTour}>
                Download tour file
              </button>
            )}
          </div>
          {!tourMode && tourNote && <p class="meta">{tourNote}</p>}
          {!tourMode && (
            <p class="meta">
              The tour file (.tour.json) runs this flow as guided onboarding on your own app —
              steps with callout text become the tour's instructions.
            </p>
          )}
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
