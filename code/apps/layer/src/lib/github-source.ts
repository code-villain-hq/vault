interface SourcePr {
  source?: string;
  repo?: string;
  url: string;
}

/** PRs are scoped by source even when owner, repo and number all match. */
export function githubPrSource(pr: SourcePr): string {
  if (pr.source) return pr.source;
  try {
    return new URL(pr.url).hostname.toLowerCase().replace(/\.+$/, "");
  } catch {
    return "";
  }
}

export function githubPrRepo(pr: SourcePr): string {
  const source = githubPrSource(pr);
  return source && pr.repo ? `${source}/${pr.repo}`.toLowerCase() : "";
}

export function githubPrKey(pr: SourcePr & { number: number }): string {
  return `${githubPrRepo(pr) || pr.repo || ""}#${pr.number}`;
}

export function githubRepoLabel(pr: SourcePr): string {
  const source = githubPrSource(pr);
  return source && pr.repo ? `${pr.repo} · ${source}` : (pr.repo ?? "");
}

export function repoHost(remote?: string | null): string | null {
  if (!remote || remote.startsWith("/")) return null;
  const [host, owner, repo, ...rest] = remote.split("/");
  return host && owner && repo && rest.length === 0 ? host : null;
}
