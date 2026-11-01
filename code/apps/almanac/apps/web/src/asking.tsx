// Asking, kept above the pages: an answer goes on arriving while you look
// at another page, and each topic keeps the talk open in it, so coming
// back finds both where you left them.

import { messageOf, UNKNOWN_PROBLEM } from "@code-villain/web";
import { useRouter } from "@tanstack/react-router";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
} from "react";
import type { Picked } from "./photos";
import { apiUrl, authHeaders, credentialsFor } from "./server";
import type { TopicSearch } from "./topic";
import { useWorkspace } from "./workspace";

/** An answer on its way, until the talk's own records have it. */
export interface Asking {
  talkId: string | null;
  question: string;
  /** The photos it came with. */
  photos: Picked[];
  /** How many questions the talk had before this one. */
  asked: number;
  text: string;
  answerId: string | null;
  problem: string | null;
  /** Until it's done or has a problem: the topic takes no other question meanwhile. */
  arriving: boolean;
}

interface Talks {
  /** The answer on its way in each topic, by topic. */
  asking: Record<string, Asking>;
  /** The talk open in each topic (`new` for a new one), by topic. */
  open: Record<string, string>;
  setOpen: (topicId: string, talk: string | undefined) => void;
  ask: (
    topicId: string,
    question: string,
    talkId: string | null,
    photos?: Picked[],
    about?: About,
  ) => Promise<void>;
}

/** What's open in front of them as they ask: a section, or a row's page. */
export interface About {
  section: string;
  row?: string;
}

const TalksContext = createContext<Talks | null>(null);

export function TalksProvider({ children }: { children: ReactNode }) {
  const { workspace, place, sync } = useWorkspace();
  const router = useRouter();
  const [asking, setAsking] = useState<Record<string, Asking>>({});
  const [open, setOpenTalks] = useState<Record<string, string>>({});
  // The topics with an answer on its way, which take no other question.
  const arriving = useRef(new Set<string>());

  const setOpen = useCallback((topicId: string, talk: string | undefined) => {
    setOpenTalks(({ [topicId]: _, ...rest }) =>
      talk === undefined ? rest : { ...rest, [topicId]: talk },
    );
  }, []);

  const ask = useCallback(
    async (
      topicId: string,
      question: string,
      talkId: string | null,
      photos: Picked[] = [],
      about?: About,
    ) => {
      const text = question.trim();
      if ((!text && photos.length === 0) || arriving.current.has(topicId)) return;
      arriving.current.add(topicId);
      const asked = talkId
        ? sync.query("Message").filter((m) => m.talk_id === talkId && m.role === "you").length
        : 0;
      const update = (change: Partial<Asking>) =>
        setAsking((all) => {
          const a = all[topicId];
          return a ? { ...all, [topicId]: { ...a, ...change } } : all;
        });
      // Done, or with a problem: the topic may be asked again.
      const finish = (change: Partial<Asking>) => {
        arriving.current.delete(topicId);
        update({ ...change, arriving: false });
      };
      setAsking((all) => ({
        ...all,
        [topicId]: {
          talkId,
          question: text,
          photos,
          asked,
          text: "",
          answerId: null,
          problem: null,
          arriving: true,
        },
      }));
      const path = `/api/w/${workspace.id}/topics/${topicId}/ask`;
      let response: Response;
      try {
        response = await fetch(apiUrl(path, place), {
          method: "POST",
          credentials: credentialsFor(place),
          headers: { ...authHeaders(place), "content-type": "application/json" },
          body: JSON.stringify({
            ...(talkId ? { talk_id: talkId } : {}),
            text,
            ...(about ? { about: about.section } : {}),
            ...(about?.row ? { row: about.row } : {}),
            ...(photos.length > 0
              ? {
                  photos: photos.map(({ media_type, data, width, height }) => ({
                    media_type,
                    data,
                    width,
                    height,
                  })),
                }
              : {}),
          }),
        });
      } catch {
        finish({ problem: "Can't reach Almanac. Check your connection, then ask again." });
        return;
      }
      if (!response.ok || !response.body) {
        const error = (await response.json().catch(() => null)) as { message?: string } | null;
        finish({ problem: messageOf(new Error(error?.message ?? UNKNOWN_PROBLEM)) });
        return;
      }
      const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
      let buffer = "";
      let said = "";
      let ended = false;
      while (!ended) {
        const read = await reader.read().catch(() => null);
        if (!read || read.done) break;
        buffer += read.value;
        let end = buffer.indexOf("\n\n");
        while (end >= 0) {
          const block = buffer.slice(0, end);
          buffer = buffer.slice(end + 2);
          end = buffer.indexOf("\n\n");
          const event = /^event: (.*)$/m.exec(block)?.[1];
          const data = block
            .split("\n")
            .filter((line) => line.startsWith("data: "))
            .map((line) => line.slice(6))
            .join("\n");
          if (!event || !data) continue;
          let body: { talk_id?: string; text?: string; message_id?: string };
          try {
            body = JSON.parse(data);
          } catch {
            continue;
          }
          if (event === "talk" && body.talk_id) {
            const talk = body.talk_id;
            update({ talkId: talk });
            // A new talk now has its id: the topic shows it, and so does the
            // page if it's still on this topic's new talk.
            setOpenTalks((all) => (all[topicId] === "new" ? { ...all, [topicId]: talk } : all));
            const { pathname, search } = router.state.location;
            if (pathname.endsWith(`/topics/${topicId}`) && (search as TopicSearch).talk === "new") {
              // Still on the section or row it was asked from; the draft is sent.
              void router.navigate({
                to: ".",
                search: ({ ask: _, ...rest }: TopicSearch) => ({ ...rest, talk }),
                replace: true,
              });
            }
          } else if (event === "text") {
            said += body.text ?? "";
            update({ text: said });
          } else if (event === "done") {
            finish({ answerId: body.message_id ?? null });
            ended = true;
          } else if (event === "problem") {
            finish({ problem: body.text ?? UNKNOWN_PROBLEM });
            ended = true;
          }
        }
      }
      // The connection dropped before the answer was done.
      if (!ended) {
        finish({ problem: "The answer stopped arriving. Check your connection, then ask again." });
      }
    },
    [workspace.id, place, sync, router],
  );

  const value = useMemo(() => ({ asking, open, setOpen, ask }), [asking, open, setOpen, ask]);
  return <TalksContext.Provider value={value}>{children}</TalksContext.Provider>;
}

/** A topic's answer on its way, the talk open in it, and asking there. */
export function useTopicTalks(topicId: string) {
  const talks = useContext(TalksContext);
  if (!talks) throw new Error("useTopicTalks needs a TalksProvider");
  const { asking, open, setOpen, ask } = talks;
  return {
    asking: asking[topicId] ?? null,
    open: open[topicId],
    setOpen: useCallback((talk: string | undefined) => setOpen(topicId, talk), [setOpen, topicId]),
    ask: useCallback(
      (question: string, talkId: string | null, photos?: Picked[], about?: About) =>
        ask(topicId, question, talkId, photos, about),
      [ask, topicId],
    ),
  };
}
