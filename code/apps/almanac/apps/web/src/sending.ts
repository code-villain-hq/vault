// Photos added here: a cover, a row's photo, a picture's. The app names
// each one itself and keeps it on this device until the server has it
// (`PUT /api/w/{id}/photos/{photo}`), so a photo is added offline too and
// shows at once; its id goes on the page through sync, as any change does.

import type { Photo, PhotoUpload } from "@almanac/types";
import { type SyncClient, uuidv7 } from "@code-villain/sync-client";
import { api, triedLater } from "./api";
import type { Picked } from "./photos";
import type { Place } from "./server";
import type { Schema } from "./sync";

/** A photo added here, still to send. */
interface Waiting {
  id: string;
  workspaceId: string;
  upload: PhotoUpload;
}

const DATABASE = "almanac-photos";
const STORE = "waiting";

let database: Promise<IDBDatabase> | undefined;

function opened(): Promise<IDBDatabase> {
  if (!database) {
    database = new Promise<IDBDatabase>((done, fail) => {
      const request = indexedDB.open(DATABASE, 1);
      request.onupgradeneeded = () => request.result.createObjectStore(STORE, { keyPath: "id" });
      request.onsuccess = () => done(request.result);
      request.onerror = () => fail(request.error);
    });
    // One that didn't open is tried again next time.
    database.catch(() => (database = undefined));
  }
  return database;
}

/** Does `use` in the store, and gives what it asked for once it's written. */
async function inStore<T>(
  mode: IDBTransactionMode,
  use: (store: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  const db = await opened();
  return new Promise<T>((done, fail) => {
    const transaction = db.transaction(STORE, mode);
    const request = use(transaction.objectStore(STORE));
    transaction.oncomplete = () => done(request.result);
    transaction.onerror = () => fail(transaction.error);
    transaction.onabort = () => fail(transaction.error);
  });
}

/** Photos still to send, by workspace, as this device keeps them. */
const lists = new Map<string, Promise<Waiting[]>>();

function waitingIn(workspaceId: string): Promise<Waiting[]> {
  let list = lists.get(workspaceId);
  if (!list) {
    list = inStore<Waiting[]>("readonly", (store) => store.getAll())
      .then((all) => all.filter((photo) => photo.workspaceId === workspaceId))
      .catch(() => []);
    lists.set(workspaceId, list);
  }
  return list;
}

/** Object URLs of photos added here, by id. */
const shown = new Map<string, string>();

/** Who sends a workspace's photos, to wake when one is added. */
const senders = new Map<string, () => void>();

/** Keeps a photo picked here to send, shows it at once, and gives its id. */
export async function keepHere(workspaceId: string, picked: Picked): Promise<string> {
  const { url, ...upload } = picked;
  const photo: Waiting = { id: uuidv7(), workspaceId, upload };
  shown.set(photo.id, url);
  (await waitingIn(workspaceId)).push(photo);
  try {
    await inStore("readwrite", (store) => store.put(photo));
  } catch {
    // Kept for as long as the app is open.
  }
  senders.get(workspaceId)?.();
  return photo.id;
}

/** An object URL for a photo added here, if it is one: sent or not. */
export async function addedHere(id: string): Promise<string | null> {
  const url = shown.get(id);
  if (url) return url;
  // Added before the app last started, and not sent yet.
  const photo = await inStore<Waiting | undefined>("readonly", (store) => store.get(id)).catch(
    () => undefined,
  );
  if (!photo) return null;
  const bytes = Uint8Array.from(atob(photo.upload.data), (c) => c.charCodeAt(0));
  const added = URL.createObjectURL(new Blob([bytes], { type: photo.upload.media_type }));
  shown.set(id, added);
  return added;
}

/**
 * Sends the photos added here once the server answers, each once; returns
 * what stops it. A server that can't be reached is asked again later; a
 * photo it refuses is dropped, since sending it again won't help.
 */
export function followPhotos(
  sync: SyncClient<Schema>,
  place: Place,
  workspaceId: string,
): () => void {
  let busy = false;
  let again = false;
  let stopped = false;
  let later: ReturnType<typeof setTimeout> | undefined;
  const run = async (): Promise<void> => {
    if (stopped || sync.status().connection !== "online") return;
    if (busy) {
      again = true;
      return;
    }
    busy = true;
    again = false;
    try {
      const list = await waitingIn(workspaceId);
      // Oldest first; one added meanwhile goes after.
      for (let photo = list[0]; photo; photo = list[0]) {
        if (stopped) return;
        try {
          await api<Photo>("PUT", `/api/w/${workspaceId}/photos/${photo.id}`, photo.upload, place);
        } catch (error) {
          if (triedLater(error)) {
            clearTimeout(later);
            later = setTimeout(() => void run(), 30_000);
            return;
          }
          console.warn("Almanac refused a photo added here", error);
        }
        list.shift();
        const sent = photo.id;
        await inStore("readwrite", (store) => store.delete(sent)).catch(() => {});
      }
    } finally {
      busy = false;
    }
    if (again) await run();
  };
  const wake = () => void run();
  senders.set(workspaceId, wake);
  const unsubscribe = sync.subscribe(wake);
  wake();
  return () => {
    stopped = true;
    clearTimeout(later);
    unsubscribe();
    if (senders.get(workspaceId) === wake) senders.delete(workspaceId);
  };
}
