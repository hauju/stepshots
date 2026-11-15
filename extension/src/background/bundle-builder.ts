import { zipSync } from "fflate";
import { buildConfig } from "../utils/config-builder";
import { generateStepSummary } from "../utils/step-summary";
import type {
  ClickthroughConfig,
  ElementBounds,
  Highlight,
  RecordedStep,
  RecordingState,
  Viewport,
} from "../types";

/** Convert a data URL (image/webp or image/png) to a Uint8Array. */
function dataUrlToBytes(dataUrl: string): Uint8Array {
  const base64 = dataUrl.split(",")[1];
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

/**
 * Build a .stepshot bundle zip from recorded state and screenshots.
 *
 * Screenshots map: stepId -> data URL for each recorded step.
 *
 * Each recorded step maps 1:1 to a demo step and carries its own screenshot
 * plus any automatically captured target highlight. Click steps use a pre-action
 * screenshot so the highlight matches the scene where the click happens.
 */
export function buildBundle(
  state: RecordingState,
  screenshots: Map<string, string>,
  viewport: Viewport,
): Uint8Array {
  const files: Record<string, Uint8Array> = {};
  const manifestSteps: ManifestStep[] = [];
  // The recorded steps that made it into the bundle, in bundle order.
  const bundledSteps: RecordedStep[] = [];

  // Each recorded step produces one screenshot for its own scene.
  for (let i = 0; i < state.steps.length; i++) {
    const step = state.steps[i];
    const dataUrl = screenshots.get(step.id);
    if (!dataUrl) {
      console.warn(`Missing screenshot for step ${i} (${step.action}), skipping`);
      continue;
    }

    const fileIndex = manifestSteps.length;
    const fileName = `steps/${fileIndex}.webp`;
    files[fileName] = dataUrlToBytes(dataUrl);

    const stepHighlight = buildManifestHighlight(step.targetBounds, step.highlight);

    const manifestStep: ManifestStep = {
      file: fileName,
      // Step name feeds the hosted guided-tour projection's step titles —
      // without it, tours built from extension bundles get blank steps.
      name: generateStepSummary(step),
      action: step.action,
      url: step.url,
      current_path: step.currentPath,
      target_url: step.targetUrl,
      selector: step.selector,
      selector_quality: step.selectorQuality,
      ...driftAnchors(step),
      text: step.text,
      key: step.key,
      scroll_x: step.scrollX,
      scroll_y: step.scrollY,
      scene_scroll_x: step.sceneScrollX,
      scene_scroll_y: step.sceneScrollY,
      value: step.value,
    };

    if (stepHighlight) {
      manifestStep.highlights = [stepHighlight];
    }

    manifestSteps.push(manifestStep);
    bundledSteps.push(step);
  }

  const manifest = {
    version: 1,
    viewport: {
      width: viewport.width,
      height: viewport.height,
      ...(viewport.deviceScaleFactor != null
        ? { deviceScaleFactor: viewport.deviceScaleFactor }
        : {}),
    },
    base_url: state.baseUrl,
    start_path: state.startPath,
    steps: manifestSteps,
  };

  const encoder = new TextEncoder();
  files["manifest.json"] = encoder.encode(JSON.stringify(manifest, null, 2));
  files["recipe.json"] = encoder.encode(
    JSON.stringify(buildRecipe({ ...state, steps: bundledSteps }, viewport), null, 2),
  );

  return zipSync(files);
}

/**
 * The bundle's `recipe.json`: the recording as a one-tutorial config whose
 * step `i` produced manifest step `i`, so the dashboard can re-record it.
 * Highlights drop their captured bounds and re-resolve against the step's
 * selector; sensitive steps carry no input and are marked `secret`.
 */
function buildRecipe(state: RecordingState, viewport: Viewport): ClickthroughConfig {
  const config = buildConfig({ ...state, viewport });
  const steps = Object.values(config.tutorials)[0]?.steps ?? [];
  steps.forEach((step, i) => {
    if (state.steps[i]?.meta?.sensitive) {
      delete step.text;
      delete step.value;
      step.secret = true;
    }
    if (step.selector) {
      for (const h of step.highlights) {
        delete h.bounds;
      }
    }
  });
  return config;
}

interface ManifestHighlight {
  bounds: ElementBounds;
  callout?: string;
  position?: string;
  arrow?: boolean;
  color?: string;
  // Wire names are snake_case: the manifest crate's HighlightEntry has no
  // serde rename, so camelCase keys are silently dropped on upload.
  border_width?: number;
  is_click_target?: boolean;
}

interface ManifestStep {
  file: string;
  name?: string;
  action?: string;
  url?: string;
  current_path?: string;
  target_url?: string;
  selector?: string;
  selector_quality?: string;
  /** Trimmed text of the target at record time — guided-tour drift anchor. */
  target_text?: string;
  /** aria-label of the target at record time — guided-tour drift anchor. */
  target_aria?: string;
  text?: string;
  key?: string;
  scroll_x?: number;
  scroll_y?: number;
  scene_scroll_x?: number;
  scene_scroll_y?: number;
  value?: string;
  highlights?: ManifestHighlight[];
}

/**
 * Drift anchors for the guided-tour player: the target's recorded text and
 * aria-label let a tour step keep working when its CSS selector drifts.
 * Mirrors the CLI recorder's capture (whitespace-collapsed, capped at 120).
 * Skipped for steps the recorder flagged sensitive, so field contents never
 * resurface in public tour JSON.
 */
function driftAnchors(step: RecordedStep): Pick<ManifestStep, "target_text" | "target_aria"> {
  if (!step.selector || step.meta?.sensitive) {
    return {};
  }
  const text = step.meta?.elementText?.replace(/\s+/g, " ").trim().slice(0, 120);
  const aria = step.meta?.ariaLabel;
  return {
    ...(text ? { target_text: text } : {}),
    ...(aria ? { target_aria: aria } : {}),
  };
}

function buildManifestHighlight(
  bounds?: ElementBounds,
  highlight?: Highlight,
): ManifestHighlight | undefined {
  if (!bounds) {
    return undefined;
  }

  return {
    bounds,
    ...(highlight?.callout ? { callout: highlight.callout } : {}),
    position: highlight?.position ?? "bottom",
    arrow: highlight?.arrow ?? false,
    color: highlight?.color ?? "#3b82f6",
    border_width: highlight?.showBorder === false ? 0 : 2,
    is_click_target: true,
  };
}
