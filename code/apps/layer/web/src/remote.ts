/**
 * `github.com/acme/billing`, however it was cloned: `remote_key` in the
 * server's `repository.rs`, line for line, so the page can say at once
 * when the workspace has it.
 */
export function remoteKey(url: string): string {
  let rest = url.trim().replace(/\/+$/, "");
  if (rest.endsWith(".git")) rest = rest.slice(0, -4);
  let host: string;
  let path: string;
  const scheme = rest.indexOf("://");
  if (scheme >= 0) {
    rest = rest.slice(scheme + 3);
    const slash = rest.indexOf("/");
    [host, path] = slash < 0 ? [rest, ""] : [rest.slice(0, slash), rest.slice(slash + 1)];
  } else {
    const colon = rest.indexOf(":");
    if (colon < 0 || rest.slice(0, colon).includes("/")) return rest;
    [host, path] = [rest.slice(0, colon), rest.slice(colon + 1)];
  }
  host = host.slice(host.lastIndexOf("@") + 1).split(":")[0] ?? "";
  return `${host.toLowerCase()}/${path.replace(/^\/+/, "")}`;
}
