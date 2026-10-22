import { RequestError } from "@code-villain/web";
import type { ApiError } from "@forge/types";
import { apiUrl, authHeaders, credentialsFor, placeFor } from "./server";

export {
  api,
  messageOf,
  RequestError,
  setSessionEndedHandler,
  useApi,
} from "@code-villain/web";

/** An uploaded file, and the Markdown that shows or links to it. */
export async function uploadAttachment(workspaceId: string, file: File): Promise<string> {
  const path = `/api/w/${workspaceId}/attachments?name=${encodeURIComponent(file.name || "file")}`;
  const place = placeFor(path);
  let response: Response;
  try {
    response = await fetch(apiUrl(path, place), {
      method: "POST",
      credentials: credentialsFor(place),
      headers: { ...authHeaders(place), "content-type": file.type || "application/octet-stream" },
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
