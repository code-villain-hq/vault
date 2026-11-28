import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useMemo, useState, type MouseEvent, type ReactNode } from "react";
import { copyAndSay } from "../lib/clipboard";
import { authoredGroups, countOf, reviewCounts, taskOfPr } from "../lib/derive";
import { useEpics } from "../lib/epics";
import { githubPrRepo, githubRepoLabel } from "../lib/github-source";
import { goTo } from "../lib/goto";
import {
  byEpic,
  groupBy,
  sortPrs,
  SORT_LABEL,
  ticketKeyIn,
  type EpicOf,
  type Group,
  type Sort,
} from "../lib/grouping";
import { readOneOf, write } from "../lib/persist";
import { createReviewPrReaders } from "../lib/review-pr-readers";
import type { AuthoredPr, ReviewRequest, TeamReviews } from "../lib/types";
import { useStore } from "../store";
import { ChevronIcon } from "./icons";
import { ContextMenu, type MenuItem } from "./menu";
import { AuthoredCard, identity, ReviewCard } from "./ReviewCards";
import { BusyLabel } from "./ui";

/** `@fe` when that is all we know; the team's own name when GitHub sent one. */
function teamLabel(team: TeamReviews): string {
  const slugPart = team.slug.split("/").pop() || team.slug;
  if (team.name && team.name.toLowerCase() !== slugPart.toLowerCase()) return team.name;
  return `@${slugPart}`;
}

const TONE = { good: "add", bad: "del", wait: "warn", draft: "" } as const;

/** Your own queue, by what it asks of you (REV-9): a second look first. */
const YOUR_MOVES = [
  { id: "you:again", heading: "Review again", again: true },
  { id: "you:new", heading: "Review", again: false },
];

/** A group's heading (REV-8): its standing, as a title. */
function groupHeading(label: string): string {
  return label === "draft" ? "Drafts" : label[0].toUpperCase() + label.slice(1);
}

const SIDES = ["review", "authored"] as const;
type Side = (typeof SIDES)[number];

/**
 * How each side can be grouped and sorted (REV-11). The first grouping is
 * the one it always had: who it was asked of, and whose move it is (REV-8).
 * Your own PRs have no size to sort by.
 */
const GROUPS = {
  review: [
    ["who", "Who asked"],
    ["epic", "Epic"],
    ["repo", "Repository"],
  ],
  authored: [
    ["move", "Whose move"],
    ["epic", "Epic"],
    ["repo", "Repository"],
  ],
} as const;
const SORTS_OF: Record<Side, readonly Sort[]> = {
  review: ["newest", "oldest", "smallest"],
  authored: ["newest", "oldest"],
};

/** The ticket a pull request is for: its task's, else a key in its branch or title. */
function ticketOf(pr: { head: string; title: string }, taskKey?: string | null): string | null {
  return taskKey ?? ticketKeyIn(pr.head) ?? ticketKeyIn(pr.title);
}

/** Grouped by epic or repository: the grouping that is not about whose move it is. */
function regroup<T extends { source?: string; url: string; repo: string }>(
  prs: T[],
  by: string,
  epicOf: (pr: T) => EpicOf | null | undefined,
): Group<T>[] {
  return by === "epic"
    ? byEpic(prs, epicOf)
    : groupBy(
        prs,
        (pr) => ({ id: githubPrRepo(pr), label: githubRepoLabel(pr) || "pull request" }),
        "No repository",
      );
}

function ReviewList({
  heading,
  epicKey,
  tone,
  title,
  count,
  open,
  onToggle,
  children,
}: {
  heading: string;
  /** An epic's Jira key, beside its title. */
  epicKey?: string;
  tone?: string;
  title?: string;
  count?: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <section className="review-list" title={title}>
      <button type="button" className="review-list-head" onClick={onToggle}>
        <span className={`chev${open ? " open" : ""}`}>
          <ChevronIcon />
        </span>
        <span className={`title${tone ? ` ${tone}` : ""}`}>{heading}</span>
        {epicKey && <span className="count key">{epicKey}</span>}
        {count !== undefined && <span className="count">{count}</span>}
      </button>
      {open && <div className="review-list-body">{children}</div>}
    </section>
  );
}

export function ReviewsView() {
  const settings = useStore((s) => s.settings);
  const queue = useStore((s) => s.reviewQueue);
  const loading = useStore((s) => s.reviewQueueLoading);
  const error = useStore((s) => s.reviewQueueError);
  const refresh = useStore((s) => s.refreshReviewQueue);
  const githubConnectionRevision = useStore((s) => s.githubConnectionRevision);
  const connection = JSON.stringify([
    settings?.github_connected,
    (settings?.github_sources ?? []).map((g) => [g.key, g.api_url, g.web_url, g.revision]),
    githubConnectionRevision,
  ]);
  const readers = useMemo(() => createReviewPrReaders(connection), [connection]);
  useEffect(() => () => readers.prune([]), [readers]);
  useEffect(() => {
    readers.prune(
      (queue?.sources ?? []).flatMap((source) => [
        ...(source.mine_error ? [] : source.mine),
        ...(source.team?.error ? [] : (source.team?.prs ?? [])),
        ...(source.authored?.error ? [] : (source.authored?.prs ?? [])),
      ]),
    );
  }, [queue, readers]);
  const toggleSettings = useStore((s) => s.toggleSettings);
  const fail = useStore((s) => s.fail);
  // Both start open. Folding is for getting one queue out of the way, not the
  // default: the personal list is why the tab exists.
  const [shut, setShut] = useState<Record<string, boolean>>({});
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const toggle = (id: string) => setShut((c) => ({ ...c, [id]: !c[id] }));
  const focusReview = useStore((s) => s.focusReview);
  const clearFocusReview = useStore((s) => s.clearFocusReview);
  const tasks = useStore((s) => s.tasks);
  const projects = useStore((s) => s.projects);
  const taskPrs = useStore((s) => s.prs);
  // Two sides of review, one at a time: what waits on you, what you wait on.
  const [side, setSideState] = useState<Side>(() => readOneOf("reviewsSide", SIDES, "review"));
  const setSide = (next: Side) => {
    setSideState(next);
    write("reviewsSide", next);
  };
  const [grouping, setGrouping] = useState<Record<Side, string>>(() => ({
    review: readOneOf(
      "reviewsGroup.review",
      GROUPS.review.map((g) => g[0]),
      "who",
    ),
    authored: readOneOf(
      "reviewsGroup.authored",
      GROUPS.authored.map((g) => g[0]),
      "move",
    ),
  }));
  const [sorting, setSorting] = useState<Record<Side, Sort>>(() => ({
    review: readOneOf("reviewsSort.review", SORTS_OF.review, "newest"),
    authored: readOneOf("reviewsSort.authored", SORTS_OF.authored, "newest"),
  }));
  const trackerInUse = useStore((s) => s.settings?.tracker != null);
  // Without Jira there is no epic to ask for; the choice is kept for when it is back.
  const by = grouping[side] === "epic" && !trackerInUse ? GROUPS[side][0][0] : grouping[side];
  const sort = sorting[side];
  const focused =
    focusReview && focusReview.split("/").length < 3
      ? settings?.github_legacy_host
        ? `${settings.github_legacy_host}/${focusReview.toLowerCase()}`
        : null
      : focusReview;

  // A banner or a message about one pull request: open its list, bring it
  // into view and light it up briefly. It waits for the queue to arrive; one
  // no longer waiting on anybody simply is not there to find.
  useEffect(() => {
    if (!focusReview || !queue) return;
    // Banners are only ever about requests of you or your team.
    setSideState("review");
    if (focused) setShut({});
    const scroll = window.setTimeout(() => {
      if (!focused) return;
      document
        .querySelector(`[data-review="${CSS.escape(focused)}"]`)
        ?.scrollIntoView({ block: "center", behavior: "smooth" });
    }, 80);
    const clear = window.setTimeout(() => clearFocusReview(), 2200);
    return () => {
      window.clearTimeout(scroll);
      window.clearTimeout(clear);
    };
  }, [focusReview, focused, queue, clearFocusReview]);

  function prMenu(pr: ReviewRequest | AuthoredPr, taskId?: string): MenuItem[] {
    const key = identity(pr);
    return [
      { label: "Open in GitHub", onSelect: () => void openUrl(pr.url).catch(fail) },
      ...(taskId ? [{ label: "Open task", onSelect: () => goTo(`pr:${taskId}`) }] : []),
      { label: "Copy link", onSelect: () => copyAndSay(pr.url, pr.url) },
      { label: `Copy ${key}`, onSelect: () => copyAndSay(key, key) },
      { label: "Copy title", onSelect: () => copyAndSay(pr.title, key) },
    ];
  }

  function openMenu(e: MouseEvent, pr: ReviewRequest | AuthoredPr, taskId?: string) {
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, items: prMenu(pr, taskId) });
  }

  /** The task a pull request of yours belongs to, if one of the app's made it. */
  function taskOf(pr: AuthoredPr): { id: string; name: string } | null {
    const id = taskOfPr(pr, taskPrs, tasks, projects);
    const task = id ? tasks.find((t) => t.id === id) : undefined;
    return task ? { id: task.id, name: task.name } : null;
  }

  const sources = queue?.sources ?? [];
  const mine = sources.flatMap((source) => (source.mine_error ? [] : source.mine));
  const mineMore = sources.some((source) => !source.mine_error && source.mine_more);
  const teams = sources.flatMap(({ source, team }) => (team ? [{ source, team }] : []));
  const requested = [
    ...mine.map((pr) => ({ pr, asked: "you" })),
    ...teams.flatMap(({ team }) =>
      team.error ? [] : team.prs.map((pr) => ({ pr, asked: teamLabel(team) })),
    ),
  ];
  const readAuthored = sources.filter((source) => source.authored && !source.authored.error);
  const ownPrs = readAuthored.flatMap((source) => source.authored?.prs ?? []);
  const keyOf = (pr: AuthoredPr) =>
    ticketOf(pr, tasks.find((t) => t.id === taskOfPr(pr, taskPrs, tasks, projects))?.issue_key);
  const epics = useEpics(
    side === "review"
      ? requested.flatMap(({ pr }) => ticketOf(pr) ?? [])
      : ownPrs.flatMap((pr) => keyOf(pr) ?? []),
    by === "epic",
  );

  if (!settings) return null;

  if (!settings.github_connected) {
    return (
      <div className="empty">
        <h2>GitHub not connected</h2>
        <p>Connect GitHub to list pull requests waiting on your review.</p>
        <button
          type="button"
          className="ui-button ui-button-primary"
          onClick={() => toggleSettings(true, "github")}
        >
          Connect GitHub
        </button>
      </div>
    );
  }

  const { toReview, yours } = reviewCounts(queue);

  return (
    <div className="wide">
      <div className="wide-head">
        <h2>Reviews</h2>
        <span className="sub">Group</span>
        <span className="ui-segmented" role="group" aria-label="Group by">
          {GROUPS[side]
            .filter(([g]) => g !== "epic" || trackerInUse)
            .map(([g, label]) => (
              <button
                key={g}
                type="button"
                className="ui-segment"
                aria-pressed={by === g}
                onClick={() => {
                  setGrouping((c) => ({ ...c, [side]: g }));
                  write(`reviewsGroup.${side}`, g);
                }}
              >
                {label}
              </button>
            ))}
        </span>
        <span className="sub">Sort</span>
        <button
          type="button"
          className="ui-button"
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            setMenu({
              x: r.left,
              y: r.bottom + 4,
              items: SORTS_OF[side].map((o) => ({
                label: `${o === sort ? "✓ " : ""}${SORT_LABEL[o]}`,
                onSelect: () => {
                  setSorting((c) => ({ ...c, [side]: o }));
                  write(`reviewsSort.${side}`, o);
                },
              })),
            });
          }}
        >
          {SORT_LABEL[sort]} ▾
        </button>
        <div className="spacer" />
        <button
          type="button"
          className="ui-button"
          onClick={() => {
            readers.refresh(side === "review" ? requested.map(({ pr }) => pr) : ownPrs);
            void refresh();
          }}
        >
          <BusyLabel busy={loading} idle="Refresh" working="Refreshing…" />
        </button>
      </div>

      <div className="ui-tabs sub-tabs" role="tablist">
        <button
          type="button"
          className="ui-tab"
          role="tab"
          aria-selected={side === "review"}
          onClick={() => setSide("review")}
        >
          To review{toReview !== undefined && <span className="ui-tab-count">{toReview}</span>}
        </button>
        <button
          type="button"
          className="ui-tab"
          role="tab"
          aria-selected={side === "authored"}
          onClick={() => setSide("authored")}
        >
          Opened by you{yours !== undefined && <span className="ui-tab-count">{yours}</span>}
        </button>
      </div>

      {error && <p className="review-note warn">{error}</p>}
      {sources.flatMap((source) => {
        const failures =
          side === "review"
            ? [source.mine_error, by !== "who" ? source.team?.error : null]
            : [source.authored?.error];
        return [...new Set(failures.filter(Boolean))].map((message) => (
          <p key={`${source.source}:${message}`} className="review-note warn">
            {source.source}: {message}
          </p>
        ));
      })}

      {side === "authored" && (
        <div>
          {readAuthored.length > 0 && ownPrs.length === 0 ? (
            <p className="review-note">You have no open pull requests.</p>
          ) : by !== "move" ? (
            regroup(sortPrs(ownPrs, sort), by, (pr) => {
              const key = keyOf(pr);
              return key ? epics[key] : null;
            }).map((g) => (
              <ReviewList
                key={g.id}
                heading={g.label}
                epicKey={g.key}
                count={String(g.items.length)}
                open={!shut[`authored:${by}:${g.id}`]}
                onToggle={() => toggle(`authored:${by}:${g.id}`)}
              >
                {g.items.map((pr) => {
                  const task = taskOf(pr);
                  return (
                    <AuthoredCard
                      key={identity(pr)}
                      pr={pr}
                      task={task}
                      readers={readers}
                      onContextMenu={(e) => openMenu(e, pr, task?.id)}
                    />
                  );
                })}
              </ReviewList>
            ))
          ) : (
            authoredGroups(sortPrs(ownPrs, sort)).map(({ standing, prs }) => (
              <ReviewList
                key={standing.label}
                heading={groupHeading(standing.label)}
                tone={TONE[standing.tone]}
                count={String(prs.length)}
                open={!shut[`authored:${standing.label}`]}
                onToggle={() => toggle(`authored:${standing.label}`)}
              >
                {prs.map((pr) => {
                  const task = taskOf(pr);
                  return (
                    <AuthoredCard
                      key={identity(pr)}
                      pr={pr}
                      task={task}
                      readers={readers}
                      onContextMenu={(e) => openMenu(e, pr, task?.id)}
                    />
                  );
                })}
              </ReviewList>
            ))
          )}
          {readAuthored
            .filter((source) => source.authored?.more)
            .map(({ source }) => (
              <p key={source} className="review-note">
                {source}: showing the 50 most recently updated.
              </p>
            ))}
        </div>
      )}

      {side === "review" && by !== "who" && (
        <>
          {sources.some((source) => !source.mine_error) && requested.length === 0 && (
            <p className="review-note">Nothing is waiting on you or your team.</p>
          )}
          {regroup(
            sortPrs(
              requested.map((r) => r.pr),
              sort,
            ),
            by,
            (pr) => {
              const key = ticketOf(pr);
              return key ? epics[key] : null;
            },
          ).map((g) => (
            <ReviewList
              key={g.id}
              heading={g.label}
              epicKey={g.key}
              count={String(g.items.length)}
              open={!shut[`review:${by}:${g.id}`]}
              onToggle={() => toggle(`review:${by}:${g.id}`)}
            >
              {g.items.map((pr) => (
                <ReviewCard
                  key={identity(pr)}
                  pr={pr}
                  asked={requested.find((r) => r.pr === pr)?.asked}
                  focused={focused === identity(pr)}
                  readers={readers}
                  onContextMenu={(e) => openMenu(e, pr)}
                />
              ))}
            </ReviewList>
          ))}
        </>
      )}

      {side === "review" && by === "who" && (
        <>
          {mine.length === 0 ? (
            <ReviewList
              heading="You"
              count={sources.some((source) => !source.mine_error) ? "0" : undefined}
              open={!shut.you}
              onToggle={() => toggle("you")}
            >
              {sources.some((source) => !source.mine_error) && (
                <p className="review-note">Nothing is waiting on you.</p>
              )}
            </ReviewList>
          ) : (
            // Asked again after you reviewed it first: its author is waiting
            // on a second look, usually a short one (REV-9).
            YOUR_MOVES.map(({ id, heading, again }) => {
              const prs = mine.filter((pr) => (pr.my_review !== "none") === again);
              return (
                prs.length > 0 && (
                  <ReviewList
                    key={id}
                    heading={heading}
                    count={countOf(prs.length, mineMore && !again)}
                    open={!shut[id]}
                    onToggle={() => toggle(id)}
                  >
                    {sortPrs(prs, sort).map((pr) => (
                      <ReviewCard
                        key={identity(pr)}
                        pr={pr}
                        focused={focused === identity(pr)}
                        readers={readers}
                        onContextMenu={(e) => openMenu(e, pr)}
                      />
                    ))}
                  </ReviewList>
                )
              );
            })
          )}

          {teams.length ? (
            teams.map(({ source, team }) => (
              <ReviewList
                key={source}
                heading={`${teamLabel(team)} · ${source}`}
                title={team.slug}
                count={team.error ? undefined : countOf(team.prs.length, team.more)}
                open={!shut[source]}
                onToggle={() => toggle(source)}
              >
                {team.error ? (
                  <p className="review-note warn">{team.error}</p>
                ) : team.prs.length === 0 ? (
                  <p className="review-note">Nothing is waiting on {teamLabel(team)}.</p>
                ) : (
                  sortPrs(team.prs, sort).map((pr) => (
                    <ReviewCard
                      key={identity(pr)}
                      pr={pr}
                      focused={focused === identity(pr)}
                      readers={readers}
                      onContextMenu={(e) => openMenu(e, pr)}
                    />
                  ))
                )}
              </ReviewList>
            ))
          ) : (
            <ReviewList heading="Team" open={!shut.team} onToggle={() => toggle("team")}>
              <p className="review-note">
                Set a review team in Settings → Code — for example @fe — to list pull requests
                requested of that team.
              </p>
              <button
                type="button"
                className="ui-button"
                onClick={() => toggleSettings(true, "github")}
              >
                Settings
              </button>
            </ReviewList>
          )}
        </>
      )}
      {side === "review" &&
        sources
          .filter((source) => source.mine_more || source.team?.more)
          .map(({ source }) => (
            <p key={source} className="review-note">
              {source}: showing up to 100 of each review list.
            </p>
          ))}

      {menu && (
        <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />
      )}
    </div>
  );
}
