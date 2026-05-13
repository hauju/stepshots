// Persistent screenshot store backed by IndexedDB.
//
// Screenshots cannot live only in a Map on the service worker, because MV3
// workers are evicted after ~30s of inactivity. When the worker restarts the
// recording state is rehydrated from chrome.storage.session, but an in-memory
// Map would be empty — leaving steps with no matching image.
//
// Data URLs can be large; chrome.storage.session is capped at 10 MB and is
// wiped on browser restart anyway. IndexedDB has neither limitation.

const DB_NAME = "stepshots";
const STORE_NAME = "screenshots";
const DB_VERSION = 1;

let dbPromise: Promise<IDBDatabase> | null = null;

function openDb(): Promise<IDBDatabase> {
  if (dbPromise) return dbPromise;
  dbPromise = new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME);
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
  return dbPromise;
}

function tx(mode: IDBTransactionMode): Promise<IDBObjectStore> {
  return openDb().then((db) => db.transaction(STORE_NAME, mode).objectStore(STORE_NAME));
}

function awaitRequest<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

export async function setScreenshot(stepId: string, dataUrl: string): Promise<void> {
  const store = await tx("readwrite");
  await awaitRequest(store.put(dataUrl, stepId));
}

export async function getScreenshot(stepId: string): Promise<string | undefined> {
  const store = await tx("readonly");
  return (await awaitRequest(store.get(stepId))) as string | undefined;
}

export async function hasScreenshot(stepId: string): Promise<boolean> {
  const store = await tx("readonly");
  const count = await awaitRequest(store.count(stepId));
  return count > 0;
}

export async function deleteScreenshot(stepId: string): Promise<void> {
  const store = await tx("readwrite");
  await awaitRequest(store.delete(stepId));
}

export async function clearScreenshots(): Promise<void> {
  const store = await tx("readwrite");
  await awaitRequest(store.clear());
}

export async function screenshotCount(): Promise<number> {
  const store = await tx("readonly");
  return await awaitRequest(store.count());
}

// Snapshot the entire store into a Map. Used at bundle-build time so the
// existing synchronous buildBundle API keeps working unchanged.
export async function loadAllScreenshots(): Promise<Map<string, string>> {
  const store = await tx("readonly");
  const keys = (await awaitRequest(store.getAllKeys())) as IDBValidKey[];
  const values = (await awaitRequest(store.getAll())) as string[];
  const map = new Map<string, string>();
  for (let i = 0; i < keys.length; i++) {
    map.set(String(keys[i]), values[i]);
  }
  return map;
}
