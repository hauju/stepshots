import { computed, signal } from "@preact/signals";
import type { Message } from "../background/messages";
import type { RecordedStep, RecordingState, Settings } from "../types";

export type ViewName = "setup" | "recording" | "export" | "settings";

// Cross-view shared state lives in signals at the module top level.
// View-local state stays in component-level useState/useSignal.

export const recordingState = signal<RecordingState | null>(null);
export const settings = signal<Settings | null>(null);
export const tabInfo = signal<{ favicon?: string; domain: string; title: string } | null>(null);

// "Settings" is a transient overlay — viewOverride lets us show it without
// disturbing the recording-derived flow. Set to null to follow recordingState.
export const viewOverride = signal<ViewName | null>(null);

export const view = computed<ViewName>(() => {
  if (viewOverride.value) return viewOverride.value;
  const s = recordingState.value;
  if (s?.isRecording) return "recording";
  if (s && s.steps.length > 0) return "export";
  return "setup";
});

export const expandedStepId = signal<string | null>(null);
export const undoEntry = signal<{ step: RecordedStep; index: number } | null>(null);
export const setupError = signal<string | null>(null);

export type UploadTone = "error" | "progress" | "success";
export const uploadStatus = signal<{ message: string; tone: UploadTone; needsApiKey?: boolean } | null>(null);
export const uploadResult = signal<{ editorUrl: string } | null>(null);

export async function sendMessage(message: Message | Record<string, unknown>): Promise<any> {
  return chrome.runtime.sendMessage(message);
}

export function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}
