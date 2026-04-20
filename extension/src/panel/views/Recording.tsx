import { useEffect, useRef, useState } from "preact/hooks";
import type { Highlight, RecordedStep, StepAction } from "../../types";
import { generateStepSummary } from "../../utils/step-summary";
import { expandedStepId, recordingState, sendMessage, undoEntry } from "../store";

const DEFAULT_ACTIONS: StepAction[] = ["click", "type", "key", "wait", "select"];

export function Recording() {
  const state = recordingState.value!;
  const stepListRef = useRef<HTMLDivElement>(null);
  const undoTimerRef = useRef<number | null>(null);
  const dragFromRef = useRef<number | null>(null);
  const lastStepCountRef = useRef(0);

  // Auto-expand newest step when one is added.
  useEffect(() => {
    if (state.steps.length > lastStepCountRef.current && state.steps.length > 0) {
      expandedStepId.value = state.steps[state.steps.length - 1].id;
    }
    if (state.steps.length !== lastStepCountRef.current) hideUndo();
    lastStepCountRef.current = state.steps.length;
    if (stepListRef.current) stepListRef.current.scrollTop = stepListRef.current.scrollHeight;
  }, [state.steps.length]);

  const togglePause = async () => {
    const msgType = state.isPaused ? "RESUME_RECORDING" : "PAUSE_RECORDING";
    const newState = await sendMessage({ type: msgType });
    if (newState) recordingState.value = newState;
  };

  const stop = async () => {
    hideUndo();
    const newState = await sendMessage({ type: "STOP_RECORDING" });
    if (newState) recordingState.value = newState;
  };

  const capture = async () => {
    await chrome.runtime.sendMessage({ type: "CAPTURE_SCREEN" });
  };

  const deleteStep = async (step: RecordedStep, index: number) => {
    const newState = await sendMessage({ type: "DELETE_STEP", stepId: step.id });
    if (newState) recordingState.value = newState;
    showUndo(step, index);
  };

  function showUndo(step: RecordedStep, index: number) {
    if (undoTimerRef.current) clearTimeout(undoTimerRef.current);
    undoEntry.value = { step, index };
    undoTimerRef.current = window.setTimeout(hideUndo, 5000);
  }

  function hideUndo() {
    undoEntry.value = null;
    if (undoTimerRef.current) {
      clearTimeout(undoTimerRef.current);
      undoTimerRef.current = null;
    }
  }

  const restoreStep = async () => {
    if (!undoEntry.value) return;
    const newState = await sendMessage({
      type: "INSERT_STEP",
      step: undoEntry.value.step,
      index: undoEntry.value.index,
    });
    hideUndo();
    if (newState) recordingState.value = newState;
  };

  const handleDragStart = (e: DragEvent, index: number) => {
    dragFromRef.current = index;
    (e.target as HTMLElement).closest(".step-item")?.classList.add("dragging");
    e.dataTransfer!.effectAllowed = "move";
  };

  const handleDragOver = (e: DragEvent) => {
    e.preventDefault();
    const item = (e.target as HTMLElement).closest(".step-item") as HTMLElement | null;
    if (!item || !stepListRef.current) return;
    stepListRef.current.querySelectorAll(".drop-above, .drop-below").forEach((el) => {
      el.classList.remove("drop-above", "drop-below");
    });
    const rect = item.getBoundingClientRect();
    item.classList.add(e.clientY < rect.top + rect.height / 2 ? "drop-above" : "drop-below");
  };

  const handleDrop = async (e: DragEvent) => {
    e.preventDefault();
    if (dragFromRef.current === null || !stepListRef.current) return;
    const item = (e.target as HTMLElement).closest(".step-item") as HTMLElement | null;
    if (!item) return;
    const allItems = Array.from(stepListRef.current.querySelectorAll(".step-item"));
    let toIndex = allItems.indexOf(item);
    const rect = item.getBoundingClientRect();
    if (e.clientY >= rect.top + rect.height / 2) toIndex++;
    if (dragFromRef.current < toIndex) toIndex--;
    const maxIndex = state.steps.length - 1;
    toIndex = Math.max(0, Math.min(toIndex, maxIndex));
    const fromIndex = dragFromRef.current;
    dragFromRef.current = null;
    if (fromIndex !== toIndex) {
      const newState = await sendMessage({ type: "REORDER_STEPS", fromIndex, toIndex });
      if (newState) recordingState.value = newState;
    }
    stepListRef.current
      .querySelectorAll(".dragging, .drop-above, .drop-below")
      .forEach((el) => el.classList.remove("dragging", "drop-above", "drop-below"));
  };

  return (
    <div>
      <div class="header-row">
        <div class="recording-indicator">
          <span class={`recording-dot ${state.isPaused ? "recording-dot-paused" : ""}`}></span>
          <span>{state.isPaused ? "Paused" : "Recording"}</span>
        </div>
        <span class="badge">
          {state.steps.length} action{state.steps.length !== 1 ? "s" : ""}
        </span>
      </div>
      <p class="meta">{state.baseUrl + state.startPath}</p>
      <p class="recording-hint">
        Capture a scene without clicking: <strong>Alt+Shift+S</strong>
      </p>
      <div ref={stepListRef} class="step-list" onDragOver={handleDragOver} onDrop={handleDrop}>
        {state.steps.map((step, i) => (
          <StepItem
            key={step.id}
            step={step}
            index={i}
            onDragStart={handleDragStart}
            onDelete={deleteStep}
          />
        ))}
      </div>
      {undoEntry.value && (
        <div class="undo-bar">
          <span>Step deleted</span>
          <button class="undo-btn" onClick={restoreStep}>
            Undo
          </button>
        </div>
      )}
      <div class="recording-controls">
        <button
          class="btn"
          title="Capture screen without an action (Alt+Shift+S)"
          onClick={capture}
        >
          Capture Screen
        </button>
        <button class="btn" onClick={togglePause}>
          {state.isPaused ? "Resume" : "Pause"}
        </button>
        <button class="btn btn-danger" onClick={stop}>
          Stop Recording
        </button>
      </div>
    </div>
  );
}

interface StepItemProps {
  step: RecordedStep;
  index: number;
  onDragStart: (e: DragEvent, index: number) => void;
  onDelete: (step: RecordedStep, index: number) => void;
}

function StepItem({ step, index, onDragStart, onDelete }: StepItemProps) {
  const isExpanded = expandedStepId.value === step.id;
  const [draft, setDraft] = useState(() => makeDraft(step));

  // Reset draft when this slot displays a different step.
  useEffect(() => {
    setDraft(makeDraft(step));
  }, [step.id]);

  const summary = generateStepSummary(step);
  const isSensitive = !!step.meta?.sensitive;
  const captureOnly = !!step.meta?.captureOnly;
  const hasHighlight = !!step.highlight?.callout;
  const availableActions = DEFAULT_ACTIONS.includes(step.action)
    ? DEFAULT_ACTIONS
    : [...DEFAULT_ACTIONS, step.action];

  const showSelector = step.selector !== undefined || ["click", "type", "hover", "select"].includes(step.action);
  const showText = step.text !== undefined || step.action === "type";
  const showValue = step.value !== undefined || step.action === "select";
  const showUrl = step.url !== undefined || step.action === "navigate";
  const showKey = step.key !== undefined || step.action === "key";

  const save = async () => {
    const highlight: Highlight = {
      callout: draft.callout.trim() || undefined,
      position: draft.position,
      arrow: draft.arrow,
    };
    const msg: Record<string, unknown> = {
      type: "UPDATE_STEP",
      stepId: step.id,
      highlight,
      action: draft.action,
    };
    if (showSelector) msg.selector = draft.selector;
    if (showText) msg.text = draft.text;
    if (showValue) msg.value = draft.value;
    if (showUrl) msg.url = draft.url;
    if (showKey) msg.key = draft.key;
    const newState = await sendMessage(msg);
    if (newState) recordingState.value = newState;
  };

  return (
    <div
      class={`step-item ${isExpanded ? "expanded" : ""}`}
      data-step-id={step.id}
      data-index={index}
    >
      <div
        class="step-summary"
        onClick={() => (expandedStepId.value = isExpanded ? null : step.id)}
      >
        <span
          class="drag-handle"
          title="Drag to reorder"
          draggable
          onDragStart={(e) => onDragStart(e as DragEvent, index)}
        >
          &#x2630;
        </span>
        <span class="step-number">{index + 1}</span>
        <span class={`step-action ${isSensitive ? "step-action-sensitive" : ""}`}>
          {isSensitive ? "sensitive" : step.action}
        </span>
        <span class="step-summary-text" title={summary}>
          {summary}
        </span>
        {hasHighlight && (
          <span class="step-highlight-icon" title="Has highlight">
            &#9998;
          </span>
        )}
        <button
          class="step-delete"
          title="Delete step"
          onClick={(e) => {
            e.stopPropagation();
            onDelete(step, index);
          }}
        >
          &times;
        </button>
      </div>
      {isExpanded && (
        <div class="step-detail">
          <label>Action</label>
          <select
            value={draft.action}
            onChange={(e) =>
              setDraft({ ...draft, action: (e.target as HTMLSelectElement).value as StepAction })
            }
          >
            {availableActions.map((a) => (
              <option value={a} key={a}>
                {a}
              </option>
            ))}
          </select>
          {showSelector && (
            <>
              <label>Selector</label>
              <input
                type="text"
                value={draft.selector}
                placeholder="CSS selector"
                onInput={(e) =>
                  setDraft({ ...draft, selector: (e.target as HTMLInputElement).value })
                }
              />
            </>
          )}
          {showText && (
            <>
              <label>Text</label>
              <input
                type="text"
                value={draft.text}
                placeholder="Typed text"
                onInput={(e) =>
                  setDraft({ ...draft, text: (e.target as HTMLInputElement).value })
                }
              />
            </>
          )}
          {showValue && (
            <>
              <label>Value</label>
              <input
                type="text"
                value={draft.value}
                placeholder="Selected value"
                onInput={(e) =>
                  setDraft({ ...draft, value: (e.target as HTMLInputElement).value })
                }
              />
            </>
          )}
          {showUrl && (
            <>
              <label>URL</label>
              <input
                type="text"
                value={draft.url}
                placeholder="/path"
                onInput={(e) =>
                  setDraft({ ...draft, url: (e.target as HTMLInputElement).value })
                }
              />
            </>
          )}
          {showKey && (
            <>
              <label>Key</label>
              <input
                type="text"
                value={draft.key}
                placeholder="e.g. Enter, cmd+k"
                onInput={(e) =>
                  setDraft({ ...draft, key: (e.target as HTMLInputElement).value })
                }
              />
            </>
          )}
          {captureOnly && (
            <div class="step-meta-row">
              <span class="step-meta-chip">Scene capture</span>
              <span class="step-meta-chip shortcut">Alt+Shift+S</span>
            </div>
          )}
          <hr class="detail-divider" />
          <label>Callout text</label>
          <input
            type="text"
            value={draft.callout}
            placeholder="e.g. Click the submit button"
            onInput={(e) =>
              setDraft({ ...draft, callout: (e.target as HTMLInputElement).value })
            }
          />
          <label>Position</label>
          <div class="position-picker">
            {(["top", "bottom", "left", "right"] as const).map((p) => (
              <button
                key={p}
                class={`position-btn ${draft.position === p ? "active" : ""}`}
                onClick={() => setDraft({ ...draft, position: p })}
              >
                {p[0].toUpperCase() + p.slice(1)}
              </button>
            ))}
          </div>
          <div class="arrow-toggle">
            <input
              type="checkbox"
              checked={draft.arrow}
              onChange={(e) =>
                setDraft({ ...draft, arrow: (e.target as HTMLInputElement).checked })
              }
            />
            <span>Show arrow</span>
          </div>
          <button class="btn-save-step" onClick={save}>
            Save
          </button>
        </div>
      )}
    </div>
  );
}

function makeDraft(step: RecordedStep) {
  return {
    action: step.action,
    selector: step.selector ?? "",
    text: step.text ?? "",
    value: step.value ?? "",
    url: step.url ?? "",
    key: step.key ?? "",
    callout: step.highlight?.callout ?? "",
    position: (step.highlight?.position ?? "bottom") as Highlight["position"],
    arrow: step.highlight?.arrow ?? false,
  };
}
