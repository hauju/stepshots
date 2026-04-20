import type { Highlight, RecordedStep, RecordingState, Settings, StepAction, Viewport } from "../types";

// Panel -> Background
export interface StartRecordingMessage {
  type: "START_RECORDING";
  tutorialName: string;
  tutorialTitle: string;
  tutorialDescription: string;
}

export interface StopRecordingMessage {
  type: "STOP_RECORDING";
}

export interface PauseRecordingMessage {
  type: "PAUSE_RECORDING";
}

export interface ResumeRecordingMessage {
  type: "RESUME_RECORDING";
}

export interface CaptureScreenMessage {
  type: "CAPTURE_SCREEN";
}

export interface GetStateMessage {
  type: "GET_STATE";
}

export interface UpdateStepMessage {
  type: "UPDATE_STEP";
  stepId: string;
  highlight?: Highlight;
  action?: StepAction;
  selector?: string;
  text?: string;
  value?: string;
  url?: string;
  key?: string;
}

export interface DeleteStepMessage {
  type: "DELETE_STEP";
  stepId: string;
}

export interface InsertStepMessage {
  type: "INSERT_STEP";
  step: RecordedStep;
  index: number;
}

export interface ReorderStepsMessage {
  type: "REORDER_STEPS";
  fromIndex: number;
  toIndex: number;
}

export interface UpdateTutorialMetaMessage {
  type: "UPDATE_TUTORIAL_META";
  tutorialTitle?: string;
  tutorialDescription?: string;
}

export interface ExportConfigMessage {
  type: "EXPORT_CONFIG";
  viewport: Viewport;
}

export interface GetSettingsMessage {
  type: "GET_SETTINGS";
}

export interface SaveSettingsMessage {
  type: "SAVE_SETTINGS";
  settings: Settings;
}

export interface UploadToStepshotsMessage {
  type: "UPLOAD_TO_STEPSHOTS";
  viewport: Viewport;
}

// Content Script -> Background
export interface StepRecordedMessage {
  type: "STEP_RECORDED";
  step: RecordedStep;
}

export interface CaptureStepScreenshotMessage {
  type: "CAPTURE_STEP_SCREENSHOT";
  stepId: string;
}

// Background -> Content Script
export interface ActivateContentScriptMessage {
  type: "ACTIVATE_CONTENT_SCRIPT";
}

export interface DeactivateContentScriptMessage {
  type: "DEACTIVATE_CONTENT_SCRIPT";
}

export interface PauseContentScriptMessage {
  type: "PAUSE_CONTENT_SCRIPT";
}

export interface ResumeContentScriptMessage {
  type: "RESUME_CONTENT_SCRIPT";
}

export interface HudUpdateMessage {
  type: "HUD_UPDATE";
  stepCount: number;
  lastAction?: string;
  isPaused: boolean;
}

export interface HideOverlaysMessage {
  type: "HIDE_OVERLAYS";
}

export interface ShowOverlaysMessage {
  type: "SHOW_OVERLAYS";
}

// Background -> Panel
export interface StateUpdateMessage {
  type: "STATE_UPDATE";
  state: RecordingState;
}

export interface ExportResultMessage {
  type: "EXPORT_RESULT";
  json: string;
}

export interface UploadProgressMessage {
  type: "UPLOAD_PROGRESS";
  stage: "bundle" | "upload" | "finalize";
  message: string;
}

export type Message =
  | StartRecordingMessage
  | StopRecordingMessage
  | PauseRecordingMessage
  | ResumeRecordingMessage
  | CaptureScreenMessage
  | GetStateMessage
  | UpdateStepMessage
  | DeleteStepMessage
  | InsertStepMessage
  | ReorderStepsMessage
  | UpdateTutorialMetaMessage
  | ExportConfigMessage
  | GetSettingsMessage
  | SaveSettingsMessage
  | UploadToStepshotsMessage
  | StepRecordedMessage
  | CaptureStepScreenshotMessage
  | ActivateContentScriptMessage
  | DeactivateContentScriptMessage
  | PauseContentScriptMessage
  | ResumeContentScriptMessage
  | HudUpdateMessage
  | HideOverlaysMessage
  | ShowOverlaysMessage
  | StateUpdateMessage
  | ExportResultMessage
  | UploadProgressMessage;
