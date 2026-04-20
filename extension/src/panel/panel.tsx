import { render } from "preact";
import type { Message } from "../background/messages";
import { recordingState, sendMessage, settings, uploadStatus, view } from "./store";
import { Export } from "./views/Export";
import { Recording } from "./views/Recording";
import { Settings as SettingsView } from "./views/Settings";
import { Setup } from "./views/Setup";

function Panel() {
  switch (view.value) {
    case "recording":
      return <Recording />;
    case "export":
      return <Export />;
    case "settings":
      return <SettingsView />;
    case "setup":
    default:
      return <Setup />;
  }
}

chrome.runtime.onMessage.addListener((message: Message) => {
  if (message.type === "STATE_UPDATE") {
    recordingState.value = message.state;
  } else if (message.type === "UPLOAD_PROGRESS") {
    uploadStatus.value = {
      message: message.message,
      tone: message.stage === "finalize" ? "success" : "progress",
    };
  }
});

sendMessage({ type: "GET_STATE" })
  .then((state) => {
    if (state) recordingState.value = state;
  })
  .catch(() => {});

sendMessage({ type: "GET_SETTINGS" })
  .then((s) => {
    if (s) settings.value = s;
  })
  .catch(() => {});

render(<Panel />, document.getElementById("root")!);
