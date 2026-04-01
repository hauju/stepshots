/**
 * Generates a robust, unique CSS selector for a DOM element.
 * Priority chain (first unique match wins):
 * 1. #id (skip generated-looking IDs)
 * 2. [data-testid] / [data-cy] / [data-test]
 * 3. [role][aria-label]
 * 4. Semantic tag + unique attribute (input[name], button[type="submit"])
 * 5. Minimal class combination
 * 6. nth-child path from nearest identifiable ancestor
 */
export function generateSelector(el: Element): string {
  // 1. ID-based selector
  if (el.id && !isGeneratedId(el.id)) {
    const sel = `#${CSS.escape(el.id)}`;
    if (isUnique(sel)) return sel;
  }

  // 2. Test attributes
  for (const attr of ["data-testid", "data-cy", "data-test"]) {
    const value = el.getAttribute(attr);
    if (value) {
      const sel = `[${attr}="${CSS.escape(value)}"]`;
      if (isUnique(sel)) return sel;
    }
  }

  // 3. ARIA role + label
  const role = el.getAttribute("role");
  const ariaLabel = el.getAttribute("aria-label");
  if (role && ariaLabel) {
    const sel = `[role="${CSS.escape(role)}"][aria-label="${CSS.escape(ariaLabel)}"]`;
    if (isUnique(sel)) return sel;
  }

  // 4. Semantic tag + unique attribute
  const tag = el.tagName.toLowerCase();
  const semanticSel = trySemanticSelector(el, tag);
  if (semanticSel) return semanticSel;

  // 5. Minimal class combination
  const classSel = tryClassSelector(el, tag);
  if (classSel) return classSel;

  // 6. nth-child path from nearest identifiable ancestor
  return buildNthChildPath(el);
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

function isGeneratedClassName(cls: string): boolean {
  // Tailwind-like utilities are fine, but skip CSS module hashes and emotion-style classes
  if (/^css-[a-z0-9]+$/i.test(cls)) return true;
  if (/^[a-z]+__[a-z]+-{2}[a-zA-Z0-9]+$/.test(cls)) return true; // BEM with hash
  if (/^sc-[a-zA-Z]+-[a-zA-Z0-9]+$/.test(cls)) return true; // styled-components
  return false;
}

function buildNthChildPath(el: Element): string {
  const parts: string[] = [];
  let current: Element | null = el;

  while (current && current !== document.documentElement) {
    const tag = current.tagName.toLowerCase();

    // Check if this ancestor has a usable ID
    if (current.id && !isGeneratedId(current.id)) {
      parts.unshift(`#${CSS.escape(current.id)}`);
      const sel = parts.join(" > ");
      if (isUnique(sel)) return sel;
    }

    const parent: Element | null = current.parentElement;
    if (!parent) {
      parts.unshift(tag);
      break;
    }

    const currentTag = current.tagName;
    const siblings = Array.from(parent.children).filter(
      (c: Element) => c.tagName === currentTag
    );

    if (siblings.length === 1) {
      parts.unshift(tag);
    } else {
      const index = siblings.indexOf(current) + 1;
      parts.unshift(`${tag}:nth-child(${index})`);
    }

    // Check if current path is already unique
    const candidate = parts.join(" > ");
    if (isUnique(candidate)) return candidate;

    current = parent;
  }

  return parts.join(" > ");
}
