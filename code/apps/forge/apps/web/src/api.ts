import type { ApiError } from "@forge/types";
import { apiUrl, authHeaders, credentials, rememberToken } from "./server";

/** A failed API call, with the server's error code and message. */
export class RequestError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "RequestError";
  }

  get offline() {
    return this.status === 0;
  }
}

let onSessionEnded = () => {};

/** What to do when the server says the session has ended (outside of signing in). */
export function setSessionEndedHandler(handler: () => void) {
  onSessionEnded = handler;
}

export async function api<T>(method: string, path: string, body?: unknown): Promise<T> {
  let response: Response;
  try {
    response = await fetch(apiUrl(path), {
      method,
      credentials,
      headers: {
        ...authHeaders(),
        ...(body === undefined ? {} : { "content-type": "application/json" }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new RequestError(0, "offline", "Can't reach Forge. Check your connection.");
  }
  await rememberToken(response);

  if (response.status === 401 && !path.startsWith("/api/auth/")) onSessionEnded();
  if (response.status === 204) return undefined as T;
  const data = await response.json().catch(() => null);
  if (!response.ok) {
    const error = data as ApiError | null;
    throw new RequestError(
      response.status,
      error?.code ?? "error",
      error?.message ?? response.statusText,
    );
  }
  return data as T;
}

/** A message to show for a failed request. */
export function messageOf(error: unknown): string {
  const message = error instanceof Error ? error.message : "Something went wrong.";
  return message.charAt(0).toUpperCase() + message.slice(1);
}

/** An uploaded file, and the Markdown that shows or links to it. */
export async function uploadAttachment(workspaceId: string, file: File): Promise<string> {
  const path = `/api/w/${workspaceId}/attachments?name=${encodeURIComponent(file.name || "file")}`;
  let response: Response;
  try {
    response = await fetch(apiUrl(path), {
      method: "POST",
      credentials,
      headers: { ...authHeaders(), "content-type": file.type || "application/octet-stream" },
      body: file,
    });
  } catch {
    throw new RequestError(0, "offline", "Can't reach Forge. Attaching files needs a connection.");
  }
  if (response.status === 413) {
    throw new RequestError(413, "too_large", `${file.name} is larger than 10 MB.`);
  }
  if (!response.ok) {
    const error = (await response.json().catch(() => null)) as ApiError | null;
    throw new RequestError(
      response.status,
      error?.code ?? "error",
      error?.message ?? "Upload failed.",
    );
  }
  const uploaded = (await response.json()) as { name: string; url: string; inline: boolean };
  // Characters that would end or escape the link text.
  const label = uploaded.name.replace(/[[\]\\]/g, "");
  return uploaded.inline ? `![${label}](${uploaded.url})` : `[${label}](${uploaded.url})`;
}
