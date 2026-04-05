import type { Message } from "../background/messages";
import { activate, deactivate, pause, resume } from "./recorder";
import { showHud, hideHud, updateHud, setHudVisible } from "./hud";
import { setToastVisible } from "./popover";

// Listen for messages from the background service worker
chrome.runtime.onMessage.addListener((message: Message, _sender, sendResponse) => {
  switch (message.type) {
    case "ACTIVATE_CONTENT_SCRIPT":
      activate();
      showHud();
      // Return the actual viewport dimensions to the service worker
      sendResponse({
        viewport: {
          width: window.innerWidth,
          height: window.innerHeight,
          deviceScaleFactor: window.devicePixelRatio || 1,
        },
      });
      return true;
    case "DEACTIVATE_CONTENT_SCRIPT":
      deactivate();
      hideHud();
      break;
    case "PAUSE_CONTENT_SCRIPT":
      pause();
      break;
    case "RESUME_CONTENT_SCRIPT":
      resume();
      break;
    case "HUD_UPDATE":
      updateHud({
        stepCount: message.stepCount,
        lastAction: message.lastAction,
        isPaused: message.isPaused,
      });
      break;
    case "HIDE_OVERLAYS":
      setHudVisible(false);
      setToastVisible(false);
      // Wait for browser repaint before confirming
      requestAnimationFrame(() => {
        requestAnimationFrame(() => sendResponse(true));
      });
      return true; // keep message channel open for async response
    case "SHOW_OVERLAYS":
      setHudVisible(true);
      setToastVisible(true);
      break;
  }
});

// On load, check if we should already be recording
chrome.runtime
  .sendMessage({ type: "GET_STATE" })
  .then((state) => {
    if (state?.isRecording) {
      activate();
      showHud();
      updateHud({
        stepCount: state.steps?.length ?? 0,
        isPaused: state.isPaused ?? false,
      });
      if (state.isPaused) {
        pause();
      }
    }
  })
  .catch(() => {
    // Service worker may not be ready yet
  });
