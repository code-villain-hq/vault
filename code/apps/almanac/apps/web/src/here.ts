// Claude Code on this Mac answers first, wherever the workspace is
// (`studio/design/behaviour.md`, "Claude"). On a server's workspace the
// desktop app asks with `here`: the server hands the question over, Claude
// Code here answers it with the server's tools (the desktop app's
// `answer_here`), and this tells the server what came of it.

import type { Answered, ClaudeCodeInfo, HandedQuestion } from "@almanac/types";
import { messageOf } from "@code-villain/web";
import { isDesktop } from "./platform";
import { apiUrl, type Place, publicUrl } from "./server";

const settingKey = "almanac:answer-here";

/** Settings → Claude's "Answer with Claude Code on this Mac": on unless turned off, on this Mac. */
export function answersHere(): boolean {
  try {
    return localStorage.getItem(settingKey) !== "off";
  } catch {
    return true;
  }
}

export function setAnswersHere(on: boolean) {
  try {
    if (on) localStorage.removeItem(settingKey);
    else localStorage.setItem(settingKey, "off");
  } catch {}
}

/**
 * Whether a question in `place` goes to Claude Code here: in the desktop
 * app, unless turned off. The workspaces on this Mac answer here anyway.
 */
export function asksHere(place: Place): boolean {
  return isDesktop && place.kind !== "mac" && answersHere();
}

/** The server's own address, which Claude Code reaches its tools at. */
const origin = (place: Place) => publicUrl("", place);

/** Whether Claude Code here can answer for the place's server. */
export async function claudeHere(place: Place): Promise<ClaudeCodeInfo> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ClaudeCodeInfo>("claude_here", { origin: origin(place) });
}

/**
 * Answers a question the server handed over with Claude Code here, each
 * piece to `piece` as it's written, then says what came of it. The server
 * goes on from there: it writes the answer, or its key answers instead.
 */
export async function answerHere(
  place: Place,
  question: HandedQuestion,
  piece?: (text: string) => void,
): Promise<void> {
  let answered: Answered;
  try {
    const { Channel, invoke } = await import("@tauri-apps/api/core");
    const pieces = new Channel<string>();
    if (piece) pieces.onmessage = piece;
    answered = await invoke<Answered>("answer_here", {
      origin: origin(place),
      question,
      piece: pieces,
    });
  } catch (e) {
    answered = { not_answered: "trouble", said: messageOf(e) };
  }
  // Nothing to do when this fails: the server stops waiting on its own,
  // and says so in the talk.
  await fetch(apiUrl("/api/answered", place), {
    method: "POST",
    credentials: "omit",
    headers: { authorization: `Bearer ${question.token}`, "content-type": "application/json" },
    body: JSON.stringify(answered),
  }).catch(() => undefined);
}
