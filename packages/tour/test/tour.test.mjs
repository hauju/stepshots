// Test suite for @stepshots/tour — exercises the player's a11y, Escape handling,
// lifecycle events, "lost the trail" recovery, and reload-resume against the
// package's OWN IIFE build (dist/index.global.js), loaded inside jsdom.
//
// RUNTIME: this file must run under `node`, not `bun`. jsdom's `runScripts`
// (which we use to evaluate the IIFE inside the fake window) breaks under Bun's
// VM. Bun remains the package manager for this repo — it installs deps and drives
// `bun run test`, which builds the bundle and then hands this file to `node`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { JSDOM } from "jsdom";

const here = dirname(fileURLToPath(import.meta.url));
// Load the package's own IIFE build — a pure `var StepshotsTour = (() => …)()`,
// so the whole file is the bundle (no vendored copy, no loader to slice off).
const iife = readFileSync(resolve(here, "../dist/index.global.js"), "utf8");

const fixture = `<!doctype html><html><body>
  <button id="btn1">Create project</button>
  <input id="field1" />
  <select id="plan1"><option value="">--</option><option value="pro">Pro</option></select>
</body></html>`;

/**
 * Fresh jsdom "page" with the IIFE evaluated inside it. Each window has its own
 * module scope, so the player's one-tour-at-a-time guard resets per page —
 * exactly like a real full page reload. `seed` pre-fills sessionStorage.
 */
function loadPage(seed) {
  // A real `url` (not the default opaque about:blank origin) so sessionStorage
  // doesn't throw SecurityError on every get/setItem.
  const dom = new JSDOM(fixture, {
    url: "https://example.com/",
    runScripts: "dangerously",
    pretendToBeVisual: true,
  });
  const { window } = dom;
  // jsdom gives every element a 0x0 box; give targets a real box so the player
  // treats them as visible and spotlights them (matches a real browser).
  window.Element.prototype.getBoundingClientRect = function () {
    return { top: 10, left: 10, width: 120, height: 40, bottom: 50, right: 130, x: 10, y: 10 };
  };
  // scrollIntoView is a jsdom no-op stub in some versions; ensure it never throws.
  window.Element.prototype.scrollIntoView = function () {};
  for (const [k, v] of Object.entries(seed || {})) window.sessionStorage.setItem(k, v);
  const s = window.document.createElement("script");
  s.textContent = iife;
  window.document.body.appendChild(s);
  return window;
}

const delay = (ms = 60) => new Promise((r) => setTimeout(r, ms));

// Minimal runner: collect pass/fail across scenarios, report at the end.
let passed = 0;
const failures = [];
let current = "";
function check(cond, msg) {
  try {
    assert.ok(cond, msg);
    passed += 1;
    console.log("  ok   -", msg);
  } catch {
    failures.push(`[${current}] ${msg}`);
    console.error("  FAIL -", msg);
  }
}
async function scenario(name, fn) {
  current = name;
  console.log(`\n# ${name}`);
  await fn();
}

const twoStep = {
  steps: [
    { selector: "#btn1", title: "Step one", body: "Click the button.", advance: { type: "click" } },
    { selector: "#field1", title: "Step two", body: "Type something.", advance: { type: "input" } },
  ],
};

// Push each event as "type" or "type:index" so scenarios can assert exact order.
const eventTag = (e) => e.type + (typeof e.index === "number" ? ":" + e.index : "");

// ---------------------------------------------------------------------------

await scenario("accessibility + Escape dismissal", async () => {
  const window = loadPage();
  check(window.matchMedia === undefined, "matchMedia absent in jsdom (exercises reduceMotion guard)");
  check(typeof window.StepshotsTour?.startTour === "function", "IIFE exposed window.StepshotsTour.startTour");

  let completeReason = null;
  let completeCount = 0;
  window.StepshotsTour.startTour(twoStep, {
    onComplete: (r) => {
      completeReason = r;
      completeCount += 1;
    },
  });

  const host = window.document.querySelector("[data-stepshots-tour]");
  check(!!host, "overlay host mounted in body");
  const shadow = host.shadowRoot;
  check(!!shadow, "host has an open shadow root");
  const card = shadow.querySelector(".card");
  check(!!card, "card exists in shadow root");
  check(card.getAttribute("role") === "dialog", 'card has role="dialog"');
  check(card.getAttribute("aria-label") === "Guided tour", 'card has aria-label="Guided tour"');
  check(card.getAttribute("aria-live") === "polite", 'card has aria-live="polite"');
  check(card.getAttribute("aria-atomic") === "true", 'card has aria-atomic="true"');
  check(!card.hasAttribute("aria-modal"), "card does NOT set aria-modal (page stays interactive)");

  const styleText = shadow.querySelector("style").textContent;
  check(
    /@media \(prefers-reduced-motion:reduce\)\{\.spot,\.card\{transition:none\}\}/.test(styleText),
    "shadow stylesheet has prefers-reduced-motion transition:none block",
  );

  const skip = shadow.querySelector(".skip");
  check(skip && skip.tagName === "BUTTON" && /Skip tour/.test(skip.textContent), "Skip is a labelled <button>");

  await delay();
  check(card.classList.contains("show"), "card shown after step resolved");
  check(shadow.querySelector("h4").textContent === "Step one", "card title announces step 1 text");
  check(shadow.querySelector(".step").textContent === "Step 1 of 2", "counter shows 'Step 1 of 2'");

  // Escape dismisses the tour (same path as Skip).
  window.document.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  check(window.document.querySelector("[data-stepshots-tour]") === null, "Escape removed overlay from the DOM");
  check(completeReason === "skip", 'onComplete fired with reason "skip"');
  check(!host.isConnected, "host element detached");

  // A second Escape must not throw and must not re-fire finish (listener removed).
  let threw = false;
  try {
    window.document.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  } catch {
    threw = true;
  }
  check(!threw, "second Escape does not throw (no leaked keydown listener)");
  check(completeCount === 1, "onComplete fired exactly once across two Escapes");
});

// ---------------------------------------------------------------------------

await scenario("event emission order — full run", async () => {
  const window = loadPage();
  const events = [];
  window.StepshotsTour.startTour(twoStep, { onEvent: (e) => events.push(eventTag(e)) });

  check(events.join(",") === "start,step:0", "start then step:0 emitted on launch");
  window.document.getElementById("btn1").click(); // click-advance step 0
  check(events.at(-1) === "step:1", "clicking the target advances to step:1");

  const field = window.document.getElementById("field1");
  field.value = "hello";
  field.dispatchEvent(new window.Event("input", { bubbles: true })); // input-advance step 1
  check(events.join(",") === "start,step:0,step:1,done", "full run emits start, step:0, step:1, done in order");
});

await scenario("event emission order — skip carries the active index", async () => {
  const window = loadPage();
  const events = [];
  window.StepshotsTour.startTour(twoStep, { onEvent: (e) => events.push(eventTag(e)) });

  window.document.getElementById("btn1").click(); // advance to step 1
  check(events.at(-1) === "step:1", "advanced to step:1 before dismissing");
  window.document.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  check(events.at(-1) === "skip:1", "skip event reports the index of the active step (1)");
  check(events.join(",") === "start,step:0,step:1,skip:1", "skip run emits start, step:0, step:1, skip:1");
});

// ---------------------------------------------------------------------------

await scenario("change-advance: a select step advances on the change event", async () => {
  const window = loadPage();
  const events = [];
  const selectTrack = {
    steps: [
      { selector: "#plan1", title: "Pick a plan", body: "Choose one.", advance: { type: "change" } },
      { selector: "#field1", title: "Name it", body: "Type something.", advance: { type: "input" } },
    ],
  };
  window.StepshotsTour.startTour(selectTrack, { onEvent: (e) => events.push(eventTag(e)) });
  check(events.join(",") === "start,step:0", "start then step:0 emitted on launch");

  // A change on an unrelated element must NOT advance the tour.
  window.document.getElementById("field1").dispatchEvent(new window.Event("change", { bubbles: true }));
  check(events.at(-1) === "step:0", "change on an unrelated element does not advance");

  // A change on the spotlighted <select> advances (no emptiness check).
  const select = window.document.getElementById("plan1");
  select.value = "pro";
  select.dispatchEvent(new window.Event("change", { bubbles: true }));
  check(events.at(-1) === "step:1", "change on the spotlighted select advances to step:1");
});

// ---------------------------------------------------------------------------

await scenario('"lost the trail" recovery when a target never appears', async () => {
  const window = loadPage();
  const events = [];
  const lostTrack = {
    steps: [{ selector: "#nope-never-exists", title: "Ghost step", body: "…", advance: { type: "click" } }],
  };
  window.StepshotsTour.startTour(lostTrack, {
    waitTimeoutMs: 50, // tiny so the recovery path fires fast
    onEvent: (e) => events.push(eventTag(e)),
  });

  check(events.join(",") === "start,step:0", "start + step:0 emitted even though the target is missing");
  await delay(90); // wait past the 50ms recovery timeout
  check(events.includes("lost:0"), "lost:0 emitted after the wait timeout");

  const shadow = window.document.querySelector("[data-stepshots-tour]").shadowRoot;
  check(shadow.querySelector("h4").textContent === "Hmm, we lost the trail", "recovery card shows the lost-the-trail heading");
  check(shadow.querySelector(".card").classList.contains("show"), "recovery card is visible");
  check(!shadow.querySelector(".spot").classList.contains("show"), "no spotlight shown in the recovery state");
});

// ---------------------------------------------------------------------------

await scenario("a throwing onEvent callback does not break the tour", async () => {
  const window = loadPage();
  let completeReason = null;
  let threw = false;
  try {
    window.StepshotsTour.startTour(twoStep, {
      onEvent: () => {
        throw new Error("consumer analytics blew up");
      },
      onComplete: (r) => {
        completeReason = r;
      },
    });
  } catch {
    threw = true;
  }
  check(!threw, "startTour does not propagate a throwing onEvent");

  await delay();
  const shadow = window.document.querySelector("[data-stepshots-tour]").shadowRoot;
  check(shadow.querySelector("h4").textContent === "Step one", "tour still rendered step 1 despite the throwing callback");

  window.document.getElementById("btn1").click();
  const field = window.document.getElementById("field1");
  field.value = "hi";
  field.dispatchEvent(new window.Event("input", { bubbles: true }));
  check(completeReason === "done", "tour still completes (onComplete: done) with a throwing onEvent");
});

// ---------------------------------------------------------------------------
// Resume across full page reloads (resumeKey persisted in sessionStorage).

const RESUME_KEY = "stepshots_tour_step:test";
const runResume = (window, opts = {}) => {
  const events = [];
  window.StepshotsTour.startTour(twoStep, {
    resumeKey: RESUME_KEY,
    onEvent: (e) => events.push(eventTag(e)),
    ...opts,
  });
  return events;
};

await scenario("resume (1) fresh run persists the step index", async () => {
  const window = loadPage();
  const events = runResume(window);
  await delay();
  check(events[0] === "start", "fresh run emits start first");
  check(window.sessionStorage.getItem(RESUME_KEY) === "0", "fresh run persists idx 0 at step 0");
  window.document.getElementById("btn1").click();
  await delay();
  check(window.sessionStorage.getItem(RESUME_KEY) === "1", "advancing to step 2 writes idx 1");
});

await scenario("resume (2) reload at stored index resumes without re-emitting start", async () => {
  const window = loadPage({ [RESUME_KEY]: "1" });
  const events = runResume(window);
  await delay();
  check(!events.includes("start"), "resume does NOT emit start");
  check(events[0] === "step:1", "resume's first event is step:1 (the resumed step)");
  const stepLabel = window.document.querySelector("[data-stepshots-tour]").shadowRoot.querySelector(".step");
  check(stepLabel.textContent === "Step 2 of 2", "resumed overlay shows 'Step 2 of 2'");
});

await scenario("resume (3) finishing clears the stored key", async () => {
  const window = loadPage({ [RESUME_KEY]: "1" });
  runResume(window);
  await delay();
  const field = window.document.getElementById("field1"); // resumed step (idx 1) is an input step
  field.value = "hello";
  field.dispatchEvent(new window.Event("input", { bubbles: true }));
  await delay();
  check(window.sessionStorage.getItem(RESUME_KEY) === null, "finishing (done) clears the resume key");
});

await scenario("resume (4) out-of-range stored index starts fresh", async () => {
  const window = loadPage({ [RESUME_KEY]: "5" }); // 5 >= steps.length (2)
  const events = runResume(window);
  await delay();
  check(events[0] === "start", "out-of-range index starts fresh (emits start)");
  check(events[1] === "step:0", "out-of-range starts at step 0");
  check(window.sessionStorage.getItem(RESUME_KEY) === "0", "out-of-range overwrites stale index with 0");
});

// ---------------------------------------------------------------------------

console.log(`\n${"-".repeat(48)}`);
if (failures.length) {
  console.error(`FAILED: ${failures.length} assertion(s), ${passed} passed`);
  for (const f of failures) console.error("  •", f);
  process.exit(1);
}
console.log(`ALL PASSED: ${passed} assertions across all scenarios`);
process.exit(0);
