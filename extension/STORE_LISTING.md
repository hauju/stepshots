# Chrome Web Store Listing

Staging copy for the Stepshots Recorder listing. Paste into the
[Developer Dashboard](https://chrome.google.com/webstore/devconsole/).
Nothing here is read from the zip — the dashboard takes every field manually.

## Short description (≤132 chars)

> Record clicks and screens on any site to create interactive product demos you can embed, share, or ship with your docs.

## Detailed description

> Stepshots Recorder turns a walkthrough on any website into a polished, interactive product demo — no editing timeline, no video file.
>
> Click Start on the side panel, do the flow you want to demo, and the extension captures each click, keystroke, and scene as a step with an annotated screenshot. Stop when you're done, upload to Stepshots, and you'll land in the editor with everything already staged.
>
> **What it does**
> - Records clicks, typing, keyboard shortcuts, dropdown selections, and captured scenes
> - Auto-generates stable CSS selectors for every step
> - Skips password fields and sensitive inputs
> - Uploads the finished bundle to your Stepshots workspace with one click
>
> **Privacy**
> Recording only runs on the tab you start it on, and only after you grant permission for that origin. Nothing is sent to Stepshots servers unless you upload. Your API key stays in your browser.
>
> Free to use with a Stepshots account.

## Single-purpose statement

> Record a user's on-screen interactions on a single website they explicitly choose, and produce an uploadable product-demo bundle.

## Permission justifications

- **`activeTab`** — inject the in-page HUD and capture the DOM of the tab the user started recording on.
- **`scripting`** — programmatically inject the recorder into the chosen tab, so injection only happens when recording (no broad static content script).
- **`tabs`** — read the URL, title, and favicon of the tab the user has chosen to record when they open the side panel, so we can display it and request a host permission for that origin. We do not enumerate other tabs and do not read tab state outside the recording flow.
- **`storage`** — persist recording state across service-worker restarts and store the user's API key + server URL preferences.
- **`sidePanel`** — render the Stepshots recorder UI in the side panel.
- **`optional_host_permissions: <all_urls>`** — requested at runtime, per-origin, the first time the user records on a site. Without it, we can't capture DOM selectors or screenshots of that origin.

## Data-usage disclosures (check these)

- Website content — captured during user-initiated recordings, uploaded only on explicit action.
- Authentication information — the user's Stepshots API key stored locally on-device.
- User activity — clicks, keystrokes, and navigations the user performs during a recording.
- Not sold, not used for creditworthiness, ads, or unrelated purposes.

## Privacy policy

Link: https://stepshots.com/legal/privacy

### Addendum to add to the hosted policy

Add a "Browser Extension" section covering the three gaps in the current page
(no mention of the extension, API-key storage, or typed-input capture):

> **Browser Extension.** The Stepshots Recorder extension captures screenshots of the active tab and records the interactions you perform (clicks, keystrokes, form selections, navigations) only while you have explicitly started a recording. Password fields and inputs marked sensitive by the site are excluded. Captured data is held locally in the browser's session storage until you upload it; nothing is sent to Stepshots servers unless you press Upload.
>
> **API key.** Your Stepshots API key is stored in the browser's local storage on your device. It is sent only to the Stepshots API (the server URL you configured) in the `Authorization` header to upload recordings. We do not transmit it to any third party.
>
> **Host permissions.** The extension requests access to a site only when you start recording on it, and only for that origin. It does not read or inject into other tabs.

## Assets checklist

- [ ] Icon 128×128 (already in `icons/icon-128.png`)
- [ ] At least one screenshot at 1280×800 or 640×400
- [ ] Small promo tile 440×280
- [ ] Optional: marquee 1400×560

## Pre-submit checklist

- [ ] Privacy-policy addendum published at https://stepshots.com/legal/privacy
- [ ] `manifest.json` version bumped (consider `1.0.0` for first public release)
- [ ] `bun run build` produces fresh `dist/` files
- [ ] Zip contains `manifest.json`, `icons/`, `src/panel/panel.html`, `src/panel/panel.css`, `dist/*.js` (exclude TS sources, `node_modules/`, `bun.lock`)
- [ ] Tested install from unpacked zip on a clean Chrome profile
