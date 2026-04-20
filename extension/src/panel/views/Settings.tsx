import { useEffect, useState } from "preact/hooks";
import type { Settings as SettingsT } from "../../types";
import { sendMessage, settings as settingsSignal, viewOverride } from "../store";

export function Settings() {
  const [stepshotsUrl, setStepshotsUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [saveLabel, setSaveLabel] = useState("Save Settings");

  useEffect(() => {
    sendMessage({ type: "GET_SETTINGS" }).then((s: SettingsT | null) => {
      if (s) {
        setStepshotsUrl(s.stepshotsUrl);
        setApiKey(s.apiKey || "");
      }
    });
  }, []);

  const save = async () => {
    const current = (await sendMessage({ type: "GET_SETTINGS" })) as SettingsT | null;
    const updated: SettingsT = {
      stepshotsUrl: stepshotsUrl.trim() || "https://stepshots.com",
      apiKey: apiKey.trim() || undefined,
      cliServerUrl: current?.cliServerUrl || "http://localhost:8124",
    };
    await sendMessage({ type: "SAVE_SETTINGS", settings: updated });
    settingsSignal.value = updated;
    setSaveLabel("Saved!");
    setTimeout(() => setSaveLabel("Save Settings"), 1500);
  };

  const back = () => {
    viewOverride.value = null;
  };

  const openApiKeys = (e: Event) => {
    e.preventDefault();
    const base = (stepshotsUrl.trim() || "https://stepshots.com").replace(/\/+$/, "");
    chrome.tabs.create({ url: `${base}/settings` });
  };

  return (
    <div>
      <h1>Settings</h1>
      <label for="setting-stepshots-url">Stepshots URL</label>
      <input
        id="setting-stepshots-url"
        type="text"
        value={stepshotsUrl}
        placeholder="https://stepshots.com"
        onInput={(e) => setStepshotsUrl((e.target as HTMLInputElement).value)}
      />
      <p class="meta">
        Default: <code>https://stepshots.com</code>. Override only if you self-host.
      </p>
      <label for="setting-api-key">API Key</label>
      <input
        id="setting-api-key"
        type="password"
        value={apiKey}
        placeholder="Paste your Stepshots API key"
        onInput={(e) => setApiKey((e.target as HTMLInputElement).value)}
      />
      <p class="meta">
        Add an API key for direct upload. You can still download the config without one.{" "}
        <a href="#" class="status-link" onClick={openApiKeys}>
          Get an API key &rarr;
        </a>
      </p>
      <button class="btn btn-primary" onClick={save}>
        {saveLabel}
      </button>
      <button class="btn btn-link" onClick={back}>
        Back
      </button>
    </div>
  );
}
