import { zipSync } from "fflate";
import type { ElementBounds, Highlight, RecordedStep, RecordingState, Viewport } from "../types";

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
      ...(step.meta?.captureOnly ? { name: "Capture screen" } : {}),
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

  return zipSync(files);
}

interface ManifestHighlight {
  bounds: ElementBounds;
  callout?: string;
  position?: string;
  arrow?: boolean;
  color?: string;
  borderWidth?: number;
  isClickTarget?: boolean;
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
    borderWidth: highlight?.showBorder === false ? 0 : 2,
    isClickTarget: true,
  };
}
