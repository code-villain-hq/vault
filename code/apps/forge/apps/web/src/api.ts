import { api, RequestError } from "@code-villain/web";
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
export async function uploadAttachment(
  workspaceId: string,
  file: File,
  scope: { issueId?: string; teamId?: string } = {},
): Promise<string> {
  const params = new URLSearchParams({ name: file.name || "file" });
  if (scope.issueId) params.set("issue_id", scope.issueId);
  else if (scope.teamId) params.set("team_id", scope.teamId);
  const path = `/api/w/${workspaceId}/attachments?${params}`;
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

/**
 * A link to an attachment that works once, within a minute, in a browser
 * without a session: for the desktop app to open, since its own session is
 * a token the browser doesn't have.
 */
export async function attachmentLink(path: string): Promise<string> {
  const place = placeFor(path);
  const link = await api<{ url: string }>("POST", `${path}/link`, undefined, place);
  return apiUrl(link.url, place);
}
