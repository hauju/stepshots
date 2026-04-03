import type { RecordedStep } from "../types";

export function generateStepSummary(step: RecordedStep): string {
  const meta = step.meta;

  if (meta?.sensitive) {
    return `Sensitive input (${meta.sensitiveType ?? "password"}) — value hidden`;
  }
  if (meta?.captureOnly) {
    return "Capture screen";
  }

  switch (step.action) {
    case "click": {
      const label = meta?.ariaLabel
        ?? meta?.elementText
        ?? meta?.placeholder
        ?? meta?.tagName
        ?? "element";
      const tagHint = meta?.tagName === "button" ? " button"
        : meta?.tagName === "a" ? " link"
        : "";
      return `Click '${truncate(label, 25)}'${tagHint}`;
    }
    case "type": {
      const field = meta?.labelText
        ?? meta?.placeholder
        ?? meta?.ariaLabel
        ?? meta?.fieldName
        ?? "field";
      const preview = step.text ? truncate(step.text, 15) : "...";
      return `Type '${preview}' into ${field}`;
    }
    case "navigate":
      return `Navigate to ${step.url ?? "/"}`;
    case "scroll": {
      const dir = (step.scrollY ?? 0) > 0 ? "down" : "up";
      return `Scroll ${dir} ${Math.abs(step.scrollY ?? 0)}px`;
    }
    case "key":
      return `Press ${step.key ?? "key"}`;
    case "select": {
      const field = meta?.labelText ?? meta?.fieldName ?? "dropdown";
      return `Select '${step.value ?? "option"}' in ${field}`;
    }
    case "hover":
      return `Hover over ${meta?.ariaLabel ?? meta?.elementText ?? meta?.tagName ?? "element"}`;
    case "wait":
      return "Wait";
    default:
      return `${step.action} step`;
  }
}

function truncate(str: string, max: number): string {
  return str.length > max ? str.slice(0, max) + "..." : str;
}
