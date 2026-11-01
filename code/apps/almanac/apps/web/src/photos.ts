// Photos a question comes with. A photo is made smaller on the way in, to
// the size Claude reads best, so it's quick to send; a sent one is fetched
// once and kept for as long as the app runs.

import type { PhotoUpload } from "@almanac/types";
import { useEffect, useState } from "react";
import { apiUrl, authHeaders, credentialsFor, type Place } from "./server";

/** A photo picked for a question: what goes out, and how it looks here. */
export interface Picked extends PhotoUpload {
  /** An object URL for the smaller photo. */
  url: string;
}

/** The most photos one question takes, as the server has it. */
export const MOST_PHOTOS = 6;

/** Claude reads a photo best at up to this many pixels on its long side. */
const LONGEST = 1568;

/** Whether a dragged or pasted thing holds a photo. */
export function holdsPhoto(data: DataTransfer | null): boolean {
  return !!data && Array.from(data.items).some((i) => i.kind === "file" && isPhoto(i.type));
}

/** The photos among dropped or pasted files. */
export function photosIn(files: FileList | File[] | null | undefined): File[] {
  return Array.from(files ?? []).filter((f) => isPhoto(f.type));
}

function isPhoto(type: string): boolean {
  return type.startsWith("image/");
}

/** The photo at most `LONGEST` pixels on its long side, as a JPEG. */
export async function pick(file: Blob): Promise<Picked> {
  const bitmap = await createImageBitmap(file);
  const scale = Math.min(1, LONGEST / Math.max(bitmap.width, bitmap.height));
  const width = Math.max(1, Math.round(bitmap.width * scale));
  const height = Math.max(1, Math.round(bitmap.height * scale));
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("This photo can't be read.");
  // A JPEG has no transparency: a screenshot's clear parts go white.
  context.fillStyle = "white";
  context.fillRect(0, 0, width, height);
  context.drawImage(bitmap, 0, 0, width, height);
  bitmap.close();
  const blob = await new Promise<Blob | null>((done) => canvas.toBlob(done, "image/jpeg", 0.88));
  if (!blob) throw new Error("This photo can't be read.");
  return {
    media_type: "image/jpeg",
    data: await base64(blob),
    width,
    height,
    url: URL.createObjectURL(blob),
  };
}

async function base64(blob: Blob): Promise<string> {
  const url = await new Promise<string>((done, fail) => {
    const reader = new FileReader();
    // A data: URL, as readAsDataURL always gives.
    reader.onload = () => done(reader.result as string);
    reader.onerror = () => fail(reader.error);
    reader.readAsDataURL(blob);
  });
  return url.slice(url.indexOf(",") + 1);
}

/** Sent photos by id, fetched once. */
const fetched = new Map<string, Promise<string>>();

/** An object URL for a sent photo, once it's here. */
export function usePhoto(place: Place, workspaceId: string, id: string): string | null {
  // The photo fetched last, by id, so another id never shows the one before.
  const [got, setGot] = useState<{ id: string; url: string } | null>(null);
  useEffect(() => {
    // A sample's photo is the app's own file, at its address already.
    if (ownFile(id)) return;
    let current = true;
    let photo = fetched.get(id);
    if (!photo) {
      photo = fetch(apiUrl(`/api/w/${workspaceId}/photos/${id}`, place), {
        credentials: credentialsFor(place),
        headers: authHeaders(place),
      }).then(async (response) => {
        if (!response.ok) throw new Error(`photo ${response.status}`);
        return URL.createObjectURL(await response.blob());
      });
      // A photo that didn't come is asked for again next time.
      photo.catch(() => fetched.delete(id));
      fetched.set(id, photo);
    }
    photo.then((url) => current && setGot({ id, url })).catch(() => {});
    return () => {
      current = false;
    };
  }, [place, workspaceId, id]);
  if (ownFile(id)) return id;
  return got?.id === id ? got.url : null;
}

/** Whether a photo's id is the address of one of the app's own files. */
function ownFile(id: string): boolean {
  return id.startsWith("/") || id.startsWith("data:");
}
