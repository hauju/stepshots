export type SelectorQuality = "stable" | "good" | "fragile" | "fallback";

export interface GeneratedSelector {
  selector: string;
  quality: SelectorQuality;
}

/**
 * Generates a robust CSS selector plus a confidence tier for replay/export.
 * Priority chain (first unique match wins):
 * 1. #id / test attributes
 * 2. aria + semantic attributes
 * 3. minimal class combination
 * 4. scoped nth-of-type path from nearest stable ancestor
 * 5. full nth-of-type path fallback
 */
export function generateSelector(el: Element): GeneratedSelector {
  const direct = tryDirectSelector(el);
  if (direct) return direct;

  const tag = el.tagName.toLowerCase();
  const classSel = tryClassSelector(el, tag);
  if (classSel) {
    return { selector: classSel, quality: "good" };
  }

  const scopedFallback = tryScopedFallbackSelector(el);
  if (scopedFallback) {
    return { selector: scopedFallback, quality: "fragile" };
  }

  return { selector: buildNthOfTypePath(el), quality: "fallback" };
}

function tryDirectSelector(el: Element): GeneratedSelector | null {
  if (el.id && !isGeneratedId(el.id)) {
    const selector = `#${CSS.escape(el.id)}`;
    if (isUnique(selector)) return { selector, quality: "stable" };
  }

  for (const attr of ["data-testid", "data-cy", "data-test"]) {
    const value = el.getAttribute(attr);
    if (!value) continue;
    const selector = `[${attr}="${CSS.escape(value)}"]`;
    if (isUnique(selector)) return { selector, quality: "stable" };
  }

  const role = el.getAttribute("role");
  const ariaLabel = el.getAttribute("aria-label");
  if (role && ariaLabel) {
    const selector = `[role="${CSS.escape(role)}"][aria-label="${CSS.escape(ariaLabel)}"]`;
    if (isUnique(selector)) return { selector, quality: "good" };
  }

  const tag = el.tagName.toLowerCase();
  const semanticSelector = trySemanticSelector(el, tag);
  if (semanticSelector) {
    return { selector: semanticSelector, quality: "good" };
  }

  return null;
}

function isGeneratedId(id: string): boolean {
  // Skip IDs that look auto-generated (contain random hex, UUIDs, or are very long)
  if (id.length > 50) return true;
  if (/^[a-f0-9]{8,}$/i.test(id)) return true;
  if (/^:r[0-9a-z]+:$/.test(id)) return true; // React generated IDs
  if (/^[a-z]+-[a-f0-9]{4,}/i.test(id)) return true; // prefix-hash pattern
  return false;
}

function isUnique(selector: string): boolean {
  try {
    return document.querySelectorAll(selector).length === 1;
  } catch {
    return false;
  }
}

function trySemanticSelector(el: Element, tag: string): string | null {
  const attrCandidates: [string, string | null][] = [
    ["name", el.getAttribute("name")],
    ["type", el.getAttribute("type")],
    ["href", el.getAttribute("href")],
    ["placeholder", el.getAttribute("placeholder")],
    ["title", el.getAttribute("title")],
    ["alt", el.getAttribute("alt")],
  ];

  for (const [attr, value] of attrCandidates) {
    if (!value) continue;
    const sel = `${tag}[${attr}="${CSS.escape(value)}"]`;
    if (isUnique(sel)) return sel;
  }

  // button/a with specific type="submit"
  if ((tag === "button" || tag === "input") && el.getAttribute("type") === "submit") {
    const sel = `${tag}[type="submit"]`;
    if (isUnique(sel)) return sel;
  }

  return null;
}

function tryClassSelector(el: Element, tag: string): string | null {
  const classes = Array.from(el.classList).filter(
    (c) => !isGeneratedClassName(c)
  );
  if (classes.length === 0) return null;

  // Try single class
  for (const cls of classes) {
    const sel = `${tag}.${CSS.escape(cls)}`;
    if (isUnique(sel)) return sel;
  }

  // Try pairs
  for (let i = 0; i < classes.length; i++) {
    for (let j = i + 1; j < classes.length; j++) {
      const sel = `${tag}.${CSS.escape(classes[i])}.${CSS.escape(classes[j])}`;
      if (isUnique(sel)) return sel;
    }
  }

  return null;
}

function tryScopedFallbackSelector(el: Element): string | null {
  const anchor = findStableAncestor(el.parentElement);
  if (!anchor) return null;

  const relativePath = buildRelativeNthOfTypePath(anchor.element, el);
  if (!relativePath) return null;

  const selector = `${anchor.selector} > ${relativePath}`;
  return isUnique(selector) ? selector : null;
}

function findStableAncestor(el: Element | null): { element: Element; selector: string } | null {
  let current = el;
  while (current && current !== document.documentElement) {
    const direct = tryDirectSelector(current);
    if (direct && direct.quality !== "fragile" && direct.quality !== "fallback") {
      return { element: current, selector: direct.selector };
    }
    current = current.parentElement;
  }
  return null;
}

function buildRelativeNthOfTypePath(anchor: Element, target: Element): string | null {
  const parts: string[] = [];
  let current: Element | null = target;

  while (current && current !== anchor) {
    const parent = current.parentElement;
    if (!parent) return null;
    parts.unshift(nthOfTypeSegment(current));
    current = parent;
  }

  return current === anchor ? parts.join(" > ") : null;
}

function nthOfTypeSegment(el: Element): string {
  const tag = el.tagName.toLowerCase();
  const parent = el.parentElement;
  if (!parent) return tag;

  const siblings = Array.from(parent.children).filter(
    (child): child is Element => child.tagName === el.tagName
  );

  if (siblings.length <= 1) {
    return tag;
  }

  const index = siblings.indexOf(el) + 1;
  return `${tag}:nth-of-type(${index})`;
}

function isGeneratedClassName(cls: string): boolean {
  // Tailwind-like utilities are fine, but skip CSS module hashes and emotion-style classes
  if (/^css-[a-z0-9]+$/i.test(cls)) return true;
  if (/^[a-z]+__[a-z]+-{2}[a-zA-Z0-9]+$/.test(cls)) return true; // BEM with hash
  if (/^sc-[a-zA-Z]+-[a-zA-Z0-9]+$/.test(cls)) return true; // styled-components
  return false;
}

function buildNthOfTypePath(el: Element): string {
  const parts: string[] = [];
  let current: Element | null = el;

  while (current && current !== document.documentElement) {
    const parent: Element | null = current.parentElement;
    if (!parent) {
      parts.unshift(current.tagName.toLowerCase());
      break;
    }

    parts.unshift(nthOfTypeSegment(current));

    const candidate = parts.join(" > ");
    if (isUnique(candidate)) return candidate;

    current = parent;
  }

  return parts.join(" > ");
}
