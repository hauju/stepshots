import type { ClickthroughConfig, RecordingState, StepConfig } from "../types";

export function buildConfig(state: RecordingState): ClickthroughConfig {
  const steps: StepConfig[] = [];
  let previousPath = state.startPath;

  for (const recordedStep of state.steps) {
    const pathChanged = !!recordedStep.currentPath && recordedStep.currentPath !== previousPath;
    steps.push(normalizeRecordedStep(state.baseUrl, recordedStep, pathChanged));
    if (recordedStep.currentPath) {
      previousPath = recordedStep.currentPath;
    }
  }

  const config: ClickthroughConfig = {
    baseUrl: state.baseUrl,
    viewport: state.viewport,
    tutorials: {
      [state.tutorialName]: {
        url: state.startPath,
        title: state.tutorialTitle,
        steps,
      },
    },
  };

  if (state.tutorialDescription) {
    config.tutorials[state.tutorialName].description = state.tutorialDescription;
  }

  return config;
}

function normalizeRecordedStep(baseUrl: string, step: RecordingState["steps"][number], pathChanged: boolean): StepConfig {
  const normalizedAction = step.action === "click"
    ? normalizeLinkClick(baseUrl, step.targetUrl, step.selector) ?? { action: "click", selector: step.selector, url: step.url }
    : { action: step.action, selector: step.selector, url: step.url };

  const configStep: StepConfig = { action: normalizedAction.action, highlights: [] };

  if (normalizedAction.selector) configStep.selector = normalizedAction.selector;
  if (step.selectorQuality) configStep.selectorQuality = step.selectorQuality;
  if (step.text) configStep.text = step.text;
  if (step.value) configStep.value = step.value;
  if (step.key) configStep.key = step.key;
  if (normalizedAction.url) configStep.url = normalizedAction.url;
  if (normalizedAction.action === "navigate" && pathChanged) {
    configStep.delay = 1200;
  }
  if (step.scrollX) configStep.scrollX = step.scrollX;
  if (step.scrollY) configStep.scrollY = step.scrollY;
  if (step.sceneScrollX != null) configStep.sceneScrollX = step.sceneScrollX;
  if (step.sceneScrollY != null) configStep.sceneScrollY = step.sceneScrollY;

  if (step.highlight || step.targetBounds) {
    const highlight = step.highlight;
    const h: Record<string, unknown> = {};
    if (step.targetBounds) h.bounds = step.targetBounds;
    if (highlight?.callout) h.callout = highlight.callout;
    if (highlight?.showBorder != null) h.showBorder = highlight.showBorder;
    if (highlight?.position) h.position = highlight.position;
    if (highlight?.arrow != null) h.arrow = highlight.arrow;
    if (highlight?.icon) h.icon = highlight.icon;
    if (highlight?.color) h.color = highlight.color;
    if (Object.keys(h).length > 0) {
      configStep.highlights = [h as StepConfig["highlights"][0]];
    }
  }

  return configStep;
}

function normalizeLinkClick(baseUrl: string, targetUrl?: string, selector?: string): { action: "navigate"; url: string; selector?: string } | null {
  if (!targetUrl || targetUrl.startsWith("#")) {
    return null;
  }
  if (targetUrl.startsWith("/")) {
    return { action: "navigate", url: targetUrl, selector };
  }

  const normalizedBase = baseUrl.replace(/\/$/, "");
  if (targetUrl.startsWith(normalizedBase)) {
    const path = targetUrl.slice(normalizedBase.length) || "/";
    return { action: "navigate", url: path.startsWith("/") ? path : `/${path}`, selector };
  }

  return null;
}
