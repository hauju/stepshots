import { zipSync } from "fflate";
import type { RecordingState, Viewport } from "../types";

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
 * Screenshots map: stepId -> data URL. The special key "__initial__" holds
 * the screenshot taken before any action (step 0).
 *
 * Highlights are placed on the PRE-action screenshot:
 * - steps/0.jpg (initial) gets state.steps[0]'s highlight
 * - steps/N.jpg (after step N-1's action) gets state.steps[N]'s highlight
 */
export function buildBundle(
  state: RecordingState,
  screenshots: Map<string, string>,
  viewport: Viewport,
): Uint8Array {
  const files: Record<string, Uint8Array> = {};
  const manifestSteps: ManifestStep[] = [];

  // Step 0: initial screenshot with first step's highlight
  const initialDataUrl = screenshots.get("__initial__");
  if (initialDataUrl) {
    files["steps/0.jpg"] = dataUrlToBytes(initialDataUrl);

    const firstStep = state.steps[0];
    const highlight = firstStep?.highlight ?? undefined;

    manifestSteps.push({
      file: "steps/0.jpg",
      ...(highlight?.callout ? {
        highlights: [{
          callout: highlight.callout,
          position: highlight.position ?? "bottom",
          arrow: highlight.arrow,
        }],
      } : {}),
    });
  }

  // Each recorded step produces a screenshot taken AFTER its action.
  // The annotation for the NEXT step goes on this screenshot.
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

    // Next step's highlight goes on this screenshot
    const nextStep = state.steps[i + 1];
    const nextHighlight = nextStep?.highlight ?? undefined;

    const manifestStep: ManifestStep = {
      file: fileName,
      action: step.action,
      url: step.url,
      selector: step.selector,
      text: step.text,
      key: step.key,
      scrollX: step.scrollX,
      scrollY: step.scrollY,
      value: step.value,
    };

    if (nextHighlight?.callout) {
      manifestStep.highlights = [{
        callout: nextHighlight.callout,
        position: nextHighlight.position ?? "bottom",
        arrow: nextHighlight.arrow,
      }];
    }

    manifestSteps.push(manifestStep);
  }

  const manifest = {
    version: 1,
    viewport: { width: viewport.width, height: viewport.height },
    steps: manifestSteps,
  };

  const encoder = new TextEncoder();
  files["manifest.json"] = encoder.encode(JSON.stringify(manifest, null, 2));

  return zipSync(files);
}

interface ManifestHighlight {
  callout?: string;
  position?: string;
  arrow?: boolean;
}

interface ManifestStep {
  file: string;
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
