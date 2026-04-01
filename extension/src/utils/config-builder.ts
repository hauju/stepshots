import type { ClickthroughConfig, RecordingState, StepConfig } from "../types";

export function buildConfig(state: RecordingState): ClickthroughConfig {
  const steps: StepConfig[] = state.steps.map((s) => {
    const step: StepConfig = { action: s.action };

    if (s.selector) step.selector = s.selector;
    if (s.text) step.text = s.text;
    if (s.value) step.value = s.value;
    if (s.key) step.key = s.key;
    if (s.url) step.url = s.url;
    if (s.scrollX) step.scrollX = s.scrollX;
    if (s.scrollY) step.scrollY = s.scrollY;

    if (s.highlight) {
      const h: Record<string, unknown> = {};
      if (s.highlight.callout) h.callout = s.highlight.callout;
      if (s.highlight.showBorder != null) h.showBorder = s.highlight.showBorder;
      if (s.highlight.position) h.position = s.highlight.position;
      if (s.highlight.arrow != null) h.arrow = s.highlight.arrow;
      if (s.highlight.icon) h.icon = s.highlight.icon;
      if (s.highlight.color) h.color = s.highlight.color;
      if (Object.keys(h).length > 0) {
        step.highlights = [h as StepConfig["highlights"][0]];
      }
    }

    return step;
  });

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
