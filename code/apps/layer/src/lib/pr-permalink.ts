import { externalHttpUrl } from "./external-url";

/** Match the REST permalink by origin, repository and PR, not query or fragment. */
export function prPermalinkIdentity(raw: string): string | null {
  const url = externalHttpUrl(raw);
  if (!url) return null;
  const { origin, pathname } = new URL(url);
  const path = /^\/([A-Za-z0-9_.-]+)\/([A-Za-z0-9_.-]+)\/pull\/([1-9][0-9]*)$/.exec(pathname);
  if (!path) return null;
  const [, owner, repo, number] = path;
  return `${origin}/${owner.toLowerCase()}/${repo.toLowerCase()}/pull/${number}`;
}
