export type StepAction =
  | "click"
  | "type"
  | "key"
  | "scroll"
  | "hover"
  | "navigate"
  | "wait"
  | "select";

export interface Highlight {
  showBorder?: boolean;
  callout?: string;
  position?: "top" | "bottom" | "left" | "right";
  arrow?: boolean;
  icon?: string;
  color?: string;
}

export interface ElementBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface StepConfig {
  action: StepAction;
  selector?: string;
  text?: string;
  value?: string;
  key?: string;
  url?: string;
  scrollY?: number;
  scrollX?: number;
  delay?: number;
  highlights: Highlight[];
}

export interface Viewport {
  width: number;
  height: number;
}

export interface TutorialConfig {
  url: string;
  title: string;
  description?: string;
  viewport?: Viewport;
  steps: StepConfig[];
}

export interface ClickthroughConfig {
  $schema?: string;
  baseUrl: string;
  viewport: Viewport;
  defaultDelay?: number;
  tutorials: Record<string, TutorialConfig>;
}

// Extension-specific types

export interface StepMeta {
  elementText?: string;
  ariaLabel?: string;
  placeholder?: string;
  fieldName?: string;
  labelText?: string;
  tagName?: string;
  inputType?: string;
  sensitive?: boolean;
  sensitiveType?: string;
  captureOnly?: boolean;
}

export interface RecordedStep {
  id: string;
  action: StepAction;
  selector?: string;
  text?: string;
  value?: string;
  key?: string;
  url?: string;
  scrollX?: number;
  scrollY?: number;
  highlight?: Highlight;
  targetBounds?: ElementBounds;
  meta?: StepMeta;
  timestamp: number;
}

export interface RecordingState {
  isRecording: boolean;
  isPaused: boolean;
  tutorialName: string;
  tutorialTitle: string;
  tutorialDescription: string;
  baseUrl: string;
  startPath: string;
  steps: RecordedStep[];
  viewport: Viewport;
  recordingTabId?: number;
}

export const DEFAULT_STATE: RecordingState = {
  isRecording: false,
  isPaused: false,
  tutorialName: "",
  tutorialTitle: "",
  tutorialDescription: "",
  baseUrl: "",
  startPath: "",
  steps: [],
  viewport: { width: 1280, height: 800 },
};

export interface Settings {
  stepshotsUrl: string;
  cliServerUrl: string;
  apiKey?: string;
}

export const DEFAULT_SETTINGS: Settings = {
  stepshotsUrl: "https://stepshots.com",
  cliServerUrl: "http://localhost:8124",
};
