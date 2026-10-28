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
import { apiUrl, authHeaders, credentialsFor } from "./server";
import type { TopicSearch } from "./topic";
import { useWorkspace } from "./workspace";

/** An answer on its way, until the talk's own records have it. */
export interface Asking {
  talkId: string | null;
  question: string;
  /** How many questions the talk had before this one. */
  asked: number;
  text: string;
  answerId: string | null;
  problem: string | null;
}

interface Talks {
  /** The answer on its way in each topic, by topic. */
  asking: Record<string, Asking>;
  /** The talk open in each topic (`new` for a new one), by topic. */
  open: Record<string, string>;
  setOpen: (topicId: string, talk: string | undefined) => void;
  ask: (topicId: string, question: string, talkId: string | null) => Promise<void>;
}

const TalksContext = createContext<Talks | null>(null);

export function TalksProvider({ children }: { children: ReactNode }) {
  const { workspace, place, sync } = useWorkspace();
  const router = useRouter();
  const [asking, setAsking] = useState<Record<string, Asking>>({});
  const [open, setOpenTalks] = useState<Record<string, string>>({});
  // The number of each topic's latest question: an earlier answer still
  // arriving there changes nothing.
  const count = useRef(0);
  const latest = useRef(new Map<string, number>());

  const setOpen = useCallback((topicId: string, talk: string | undefined) => {
    setOpenTalks(({ [topicId]: _, ...rest }) =>
      talk === undefined ? rest : { ...rest, [topicId]: talk },
    );
  }, []);

  const ask = useCallback(
    async (topicId: string, question: string, talkId: string | null) => {
      const text = question.trim();
      if (!text) return;
      const asked = talkId
        ? sync.query("Message").filter((m) => m.talk_id === talkId && m.role === "you").length
        : 0;
      const number = ++count.current;
      latest.current.set(topicId, number);
      const update = (change: Partial<Asking>) =>
        setAsking((all) => {
          const a = all[topicId];
          if (!a || latest.current.get(topicId) !== number) return all;
          return { ...all, [topicId]: { ...a, ...change } };
        });
      setAsking((all) => ({
        ...all,
        [topicId]: { talkId, question: text, asked, text: "", answerId: null, problem: null },
      }));
      const path = `/api/w/${workspace.id}/topics/${topicId}/ask`;
      let response: Response;
      try {
        response = await fetch(apiUrl(path, place), {
          method: "POST",
          credentials: credentialsFor(place),
          headers: { ...authHeaders(place), "content-type": "application/json" },
          body: JSON.stringify(talkId ? { talk_id: talkId, text } : { text }),
        });
      } catch {
        update({ problem: "Can't reach Almanac. Check your connection, then ask again." });
        return;
      }
      if (!response.ok || !response.body) {
        const error = (await response.json().catch(() => null)) as { message?: string } | null;
        update({ problem: messageOf(new Error(error?.message ?? UNKNOWN_PROBLEM)) });
        return;
      }
      const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
      let buffer = "";
      let said = "";
      for (;;) {
        const { value, done } = await reader.read().catch(() => ({ value: "", done: true }));
        if (done) break;
        buffer += value;
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
          const body = JSON.parse(data) as { talk_id?: string; text?: string; message_id?: string };
          if (event === "talk" && body.talk_id) {
            const talk = body.talk_id;
            update({ talkId: talk });
            // A new talk now has its id: the topic shows it, and so does the
            // page if it's still on this topic's new talk.
            setOpenTalks((all) => (all[topicId] === "new" ? { ...all, [topicId]: talk } : all));
            const { pathname, search } = router.state.location;
            if (pathname.endsWith(`/topics/${topicId}`) && (search as TopicSearch).talk === "new") {
              void router.navigate({ to: ".", search: { talk }, replace: true });
            }
          } else if (event === "text") {
            said += body.text ?? "";
            update({ text: said });
          } else if (event === "done") {
            update({ answerId: body.message_id ?? null });
          } else if (event === "problem") {
            update({ problem: body.text ?? UNKNOWN_PROBLEM });
          }
        }
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
      (question: string, talkId: string | null) => ask(topicId, question, talkId),
      [ask, topicId],
    ),
  };
}
