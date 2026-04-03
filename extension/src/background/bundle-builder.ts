import { zipSync } from "fflate";
import type { ElementBounds, Highlight, RecordingState, Viewport } from "../types";

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
    const fileName = `steps/${fileIndex}.jpg`;
    files[fileName] = dataUrlToBytes(dataUrl);

    const stepHighlight = buildManifestHighlight(step.targetBounds, step.highlight);

    const manifestStep: ManifestStep = {
      file: fileName,
      ...(step.meta?.captureOnly ? { name: "Capture screen" } : {}),
      action: step.action,
      url: step.url,
      selector: step.selector,
      text: step.text,
      key: step.key,
      scrollX: step.scrollX,
      scrollY: step.scrollY,
      value: step.value,
    };

    if (stepHighlight) {
      manifestStep.highlights = [stepHighlight];
    }

    manifestSteps.push(manifestStep);
  }

  const manifest = {
    version: 1,
    viewport: { width: viewport.width, height: viewport.height },
    baseUrl: state.baseUrl,
    startPath: state.startPath,
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
  icon?: string;
  isClickTarget?: boolean;
}

interface ManifestStep {
  file: string;
  name?: string;
  action?: string;
  url?: string;
  selector?: string;
  text?: string;
  key?: string;
  scrollX?: number;
  scrollY?: number;
  value?: string;
  highlights?: ManifestHighlight[];
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
    ...(highlight?.icon ? { icon: highlight.icon } : {}),
    isClickTarget: true,
  };
}
