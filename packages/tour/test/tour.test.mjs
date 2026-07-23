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
  window.StepshotsTour.startTour(twoStep, {
    inputSettleMs: 30,
    onEvent: (e) => events.push(eventTag(e)),
  });

  check(events.join(",") === "start,step:0", "start then step:0 emitted on launch");
  window.document.getElementById("btn1").click(); // click-advance step 0
  check(events.at(-1) === "step:1", "clicking the target advances to step:1");

  const field = window.document.getElementById("field1");
  field.value = "hello";
  field.dispatchEvent(new window.Event("input", { bubbles: true })); // input step: settle-advance
  check(events.at(-1) === "step:1", "typing does NOT advance immediately (settle pending)");
  await delay(80); // past the 30ms settle window
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
      inputSettleMs: 30,
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
  await delay(80); // past the settle window
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
  runResume(window, { inputSettleMs: 30 });
  await delay();
  const field = window.document.getElementById("field1"); // resumed step (idx 1) is an input step
  field.value = "hello";
  field.dispatchEvent(new window.Event("input", { bubbles: true }));
  await delay(80); // past the settle window
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
// Input-step settle/commit behavior and walk-back validation.

// Step 0 and 2 reuse #btn1 — the fixture only has one button, and reuse also
// proves advance detection binds to the ACTIVE step, not to the element.
const threeStep = {
  steps: [
    { selector: "#btn1", title: "Open the form", body: "Click.", advance: { type: "click" } },
    { selector: "#field1", title: "Name it", body: "Type a name.", advance: { type: "input" } },
    { selector: "#btn1", title: "Create it", body: "Hit create.", advance: { type: "click" } },
  ],
};

await scenario("input settle: clearing the value cancels the pending advance", async () => {
  const window = loadPage();
  const events = [];
  window.StepshotsTour.startTour(twoStep, { inputSettleMs: 40, onEvent: (e) => events.push(eventTag(e)) });
  window.document.getElementById("btn1").click(); // → input step
  const field = window.document.getElementById("field1");
  field.value = "x";
  field.dispatchEvent(new window.Event("input", { bubbles: true }));
  field.value = "";
  field.dispatchEvent(new window.Event("input", { bubbles: true })); // emptied before settle
  await delay(120);
  check(events.at(-1) === "step:1", "emptying the field before the settle window cancels the advance");
  field.value = "ok";
  field.dispatchEvent(new window.Event("input", { bubbles: true }));
  await delay(120);
  check(events.at(-1) === "done", "typing again after the cancel still settles and completes");
});

await scenario("Enter commits an input step immediately (no settle wait)", async () => {
  const window = loadPage();
  const events = [];
  // Default 1200ms settle — an immediate `done` proves Enter committed, not the timer.
  window.StepshotsTour.startTour(twoStep, { onEvent: (e) => events.push(eventTag(e)) });
  window.document.getElementById("btn1").click();
  const field = window.document.getElementById("field1");
  field.value = "hello";
  field.dispatchEvent(new window.Event("input", { bubbles: true }));
  field.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  check(events.at(-1) === "done", "Enter on the active field advances without waiting for settle");
});

await scenario("leaving the field (focusout) commits an input step immediately", async () => {
  const window = loadPage();
  const events = [];
  window.StepshotsTour.startTour(twoStep, { onEvent: (e) => events.push(eventTag(e)) });
  window.document.getElementById("btn1").click();
  const field = window.document.getElementById("field1");
  field.value = "hello";
  field.dispatchEvent(new window.Event("input", { bubbles: true }));
  field.dispatchEvent(new window.Event("focusout", { bubbles: true })); // e.g. mousedown on Create
  check(events.at(-1) === "done", "blurring the active field advances without waiting for settle");

  // An EMPTY field must not commit on blur.
  const window2 = loadPage();
  const events2 = [];
  window2.StepshotsTour.startTour(twoStep, { onEvent: (e) => events2.push(eventTag(e)) });
  window2.document.getElementById("btn1").click();
  window2.document.getElementById("field1").dispatchEvent(new window2.Event("focusout", { bubbles: true }));
  check(events2.at(-1) === "step:1", "blurring an empty field does not advance");
});

await scenario("resume walks back past an input step whose field is empty again", async () => {
  const window = loadPage({ [RESUME_KEY]: "2" }); // stored: the final "Create it" step
  const events = [];
  window.StepshotsTour.startTour(threeStep, { resumeKey: RESUME_KEY, onEvent: (e) => events.push(eventTag(e)) });
  await delay();
  check(events[0] === "step:1", "resume re-enters at the unmet input step, not the stored one");
  const stepLabel = window.document.querySelector("[data-stepshots-tour]").shadowRoot.querySelector(".step");
  check(stepLabel.textContent === "Step 2 of 3", "overlay shows 'Step 2 of 3' after walking back");
  check(window.sessionStorage.getItem(RESUME_KEY) === "1", "walk-back rewrites the stored index");
});

await scenario("resume keeps the stored step when earlier input steps still hold", async () => {
  const window = loadPage({ [RESUME_KEY]: "2" });
  window.document.getElementById("field1").value = "kept"; // earlier requirement still met
  const events = [];
  window.StepshotsTour.startTour(threeStep, { resumeKey: RESUME_KEY, onEvent: (e) => events.push(eventTag(e)) });
  await delay();
  check(events[0] === "step:2", "resume stays on the stored step when the earlier field has a value");
});

await scenario("clearing an earlier step's field walks the tour back live", async () => {
  const window = loadPage();
  const events = [];
  window.StepshotsTour.startTour(threeStep, { inputSettleMs: 30, onEvent: (e) => events.push(eventTag(e)) });
  window.document.getElementById("btn1").click(); // → input step
  const field = window.document.getElementById("field1");
  field.value = "My project";
  field.dispatchEvent(new window.Event("input", { bubbles: true }));
  field.dispatchEvent(new window.Event("focusout", { bubbles: true })); // commit → "Create it" step
  check(events.at(-1) === "step:2", "committed name advances to the final step");
  field.value = "";
  field.dispatchEvent(new window.Event("input", { bubbles: true })); // user clears the name again
  check(events.at(-1) === "step:1", "emptying the earlier field walks the tour back to the input step");
});

await scenario("SPA nav away and back re-validates earlier input steps", async () => {
  const window = loadPage();
  const events = [];
  window.StepshotsTour.startTour(threeStep, { inputSettleMs: 30, onEvent: (e) => events.push(eventTag(e)) });
  const btn = window.document.getElementById("btn1");
  btn.click(); // → input step
  const field = window.document.getElementById("field1");
  field.value = "My project";
  field.dispatchEvent(new window.Event("input", { bubbles: true }));
  field.dispatchEvent(new window.Event("focusout", { bubbles: true })); // commit → "Create it" step
  check(events.at(-1) === "step:2", "on the final step before navigating away");

  btn.remove(); // "navigate away": the target unmounts…
  field.value = ""; // …and the form state is gone (no input event — remount, not typing)
  await delay(120); // let the rAF loop lose the target
  const fresh = window.document.createElement("button");
  fresh.id = "btn1";
  fresh.textContent = "Create project";
  window.document.body.appendChild(fresh); // "navigate back": target remounts, field empty
  await delay(120); // let the rAF loop re-acquire
  check(events.at(-1) === "step:1", "re-acquiring the target with an empty earlier field walks back");
});

// ---------------------------------------------------------------------------
// firstRun intro — the consent card autoBoot offers before auto-starting a tour.

await scenario("firstRun intro: consent card gates the auto-started tour", async () => {
  const window = loadPage();
  const events = [];
  window.StepshotsTour.autoBoot({
    tracks: { welcome: twoStep },
    firstRun: {
      key: "welcome",
      marker: "#btn1",
      intro: { title: "Welcome 👋", body: "Take a quick tour?", startLabel: "Let's go", dismissLabel: "Not now" },
    },
    onEvent: (e) => events.push(eventTag(e)),
  });
  await delay(); // autoBoot defers to DOMContentLoaded while jsdom is "loading"
  const introHost = window.document.querySelector("[data-stepshots-intro]");
  check(!!introHost, "intro card mounted when the marker is present");
  const shadow = introHost.shadowRoot;
  check(shadow.querySelector("h4").textContent === "Welcome 👋", "intro shows the provided title");
  check(shadow.querySelector("p").textContent === "Take a quick tour?", "intro shows the provided body");
  check(shadow.querySelector(".start").textContent === "Let's go", "start button uses the provided label");
  check(shadow.querySelector(".dismiss").textContent === "Not now", "dismiss button uses the provided label");
  check(shadow.querySelector(".card").getAttribute("role") === "dialog", 'intro card has role="dialog"');
  check(window.document.querySelector("[data-stepshots-tour]") === null, "tour does NOT start while the intro is open");
  check(window.localStorage.getItem("stepshots_tour_seen:welcome") === "1", "the offer itself marks the tour seen");
  check(events.length === 0, "no tour events while the intro is open");

  shadow.querySelector(".start").click();
  check(window.document.querySelector("[data-stepshots-intro]") === null, "accepting removes the intro card");
  check(events.join(",") === "start,step:0", "accepting starts the tour at step 0");
  check(window.sessionStorage.getItem("stepshots_active_tour") === "welcome", "accepting stashes the active tour key");
});

await scenario("firstRun intro: declining never starts or re-offers the tour", async () => {
  const window = loadPage();
  const events = [];
  const boot = () =>
    window.StepshotsTour.autoBoot({
      tracks: { welcome: twoStep },
      firstRun: { key: "welcome", marker: "#btn1", intro: { title: "Welcome", body: "Tour?" } },
      onEvent: (e) => events.push(eventTag(e)),
    });
  boot();
  await delay();
  const shadow = window.document.querySelector("[data-stepshots-intro]").shadowRoot;
  check(shadow.querySelector(".start").textContent === "Show me around", "start label has a sensible default");
  check(shadow.querySelector(".dismiss").textContent === "I'll explore on my own", "dismiss label has a sensible default");
  shadow.querySelector(".dismiss").click();
  check(window.document.querySelector("[data-stepshots-intro]") === null, "declining removes the intro card");
  check(window.document.querySelector("[data-stepshots-tour]") === null, "declining does not start the tour");
  check(events.length === 0, "declining emits no tour events");
  boot();
  await delay();
  check(window.document.querySelector("[data-stepshots-intro]") === null, "a later autoBoot does not re-offer (seen)");
});

await scenario("firstRun intro: Escape declines", async () => {
  const window = loadPage();
  window.StepshotsTour.autoBoot({
    tracks: { welcome: twoStep },
    firstRun: { key: "welcome", marker: "#btn1", intro: { title: "Welcome", body: "Tour?" } },
  });
  await delay();
  check(!!window.document.querySelector("[data-stepshots-intro]"), "intro card is open");
  window.document.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  check(window.document.querySelector("[data-stepshots-intro]") === null, "Escape removes the intro card");
  check(window.document.querySelector("[data-stepshots-tour]") === null, "Escape does not start the tour");
});

await scenario("firstRun without intro still auto-starts immediately", async () => {
  const window = loadPage();
  const events = [];
  window.StepshotsTour.autoBoot({
    tracks: { welcome: twoStep },
    firstRun: { key: "welcome", marker: "#btn1" },
    onEvent: (e) => events.push(eventTag(e)),
  });
  await delay();
  check(events.join(",") === "start,step:0", "marker starts the tour outright when no intro is configured");
  check(!!window.document.querySelector("[data-stepshots-tour]"), "tour overlay mounted");
});

// ---------------------------------------------------------------------------

await scenario("attribution badge — present with the badge option, absent without it", async () => {
  // With `badge`: a muted attribution anchor is rendered in the callout card.
  const withBadge = loadPage();
  withBadge.StepshotsTour.startTour(twoStep, {
    badge: { label: "Powered by Stepshots", href: "https://stepshots.com/?ref=tour-badge" },
  });
  const cardWith = withBadge.document.querySelector("[data-stepshots-tour]").shadowRoot.querySelector(".card");
  const a = cardWith.querySelector("a");
  check(!!a && a.tagName === "A", "an anchor is rendered in the card when badge is set");
  check(a.textContent === "Powered by Stepshots", "badge anchor uses the provided label");
  check(a.getAttribute("href") === "https://stepshots.com/?ref=tour-badge", "badge anchor uses the provided href");
  check(a.getAttribute("target") === "_blank", 'badge anchor sets target="_blank"');
  check(a.getAttribute("rel") === "noopener noreferrer", 'badge anchor sets rel="noopener noreferrer"');

  // Without `badge`: no anchor exists in the card at all.
  const noBadge = loadPage();
  noBadge.StepshotsTour.startTour(twoStep, {});
  const cardWithout = noBadge.document.querySelector("[data-stepshots-tour]").shadowRoot.querySelector(".card");
  check(cardWithout.querySelector("a") === null, "no anchor is rendered in the card when badge is omitted");
});

await scenario("show-me trigger starts a registry track on click", async () => {
  const window = loadPage();
  const { document } = window;
  // An FAQ-style launcher plus a decoy — only the marked element may start a tour.
  const faq = document.createElement("button");
  faq.setAttribute("data-stepshots-tour-trigger", "faq-demo");
  faq.textContent = "Show me";
  document.body.appendChild(faq);
  const decoy = document.createElement("button");
  decoy.textContent = "Unrelated";
  document.body.appendChild(decoy);

  window.__STEPSHOTS_TOURS = { "faq-demo": twoStep };
  window.StepshotsTour.autoBoot();

  decoy.dispatchEvent(new window.MouseEvent("click", { bubbles: true }));
  await delay();
  check(
    !document.querySelector("[data-stepshots-tour]"),
    "clicks outside a trigger start nothing",
  );

  faq.dispatchEvent(new window.MouseEvent("click", { bubbles: true }));
  await delay();
  const host = document.querySelector("[data-stepshots-tour]");
  check(!!host, "clicking the trigger mounts the tour overlay");
  const title = host?.shadowRoot?.querySelector("h4")?.textContent;
  check(title === "Step one", "the triggered tour starts at step 0 with the track's copy");
  check(
    window.sessionStorage.getItem("stepshots_active_tour") === "faq-demo",
    "the trigger stashes the active key so SPA navigations resume it",
  );

  // A trigger naming an unknown key is ignored (no crash, no overlay churn).
  const bad = document.createElement("button");
  bad.setAttribute("data-stepshots-tour-trigger", "nope");
  document.body.appendChild(bad);
  bad.dispatchEvent(new window.MouseEvent("click", { bubbles: true }));
  await delay();
  check(
    document.querySelectorAll("[data-stepshots-tour]").length === 1,
    "an unknown trigger key changes nothing",
  );
});

await scenario("jump trigger stashes the key, navigates, and starts on arrival", async () => {
  // Page A: the FAQ page. The flow starts elsewhere, so the trigger carries a URL.
  const pageA = loadPage();
  const trigger = pageA.document.createElement("button");
  trigger.setAttribute("data-stepshots-tour-trigger", "faq-demo");
  trigger.setAttribute("data-stepshots-tour-url", "/settings");
  pageA.document.body.appendChild(trigger);
  // Page A doesn't even need the track — the destination owns it.
  pageA.StepshotsTour.autoBoot();
  // autoBoot defers binding to DOMContentLoaded when the document is still
  // loading (jsdom reports "loading" here) — yield once before clicking.
  await delay();
  trigger.dispatchEvent(new pageA.MouseEvent("click", { bubbles: true }));
  await delay();
  check(
    !pageA.document.querySelector("[data-stepshots-tour]"),
    "no tour starts on the page hosting a jump trigger",
  );
  check(
    pageA.sessionStorage.getItem("stepshots_active_tour") === "faq-demo",
    "the jump stashes the tour key for the destination page",
  );
  check(
    pageA.sessionStorage.getItem("stepshots_active_tour:step") === null,
    "the jump clears any stale step index — arrival starts at step 0",
  );

  // Page B: the destination after navigation (fresh page, same sessionStorage).
  const pageB = loadPage({ stepshots_active_tour: "faq-demo" });
  pageB.__STEPSHOTS_TOURS = { "faq-demo": twoStep };
  pageB.StepshotsTour.autoBoot();
  await delay();
  const host = pageB.document.querySelector("[data-stepshots-tour]");
  check(!!host, "the destination page starts the stashed tour on boot");
  check(
    host?.shadowRoot?.querySelector("h4")?.textContent === "Step one",
    "the jumped tour begins at step 0",
  );
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
