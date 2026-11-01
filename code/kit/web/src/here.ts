// Claude Code on this Mac answers first, wherever the workspace is
// (`studio/design/behaviour.md`, "Claude"). On a server's workspace the
// desktop app asks with `here`: the server hands the question over, Claude
// Code here answers it with the server's tools (the desktop app's
// `answer_here`, with `code/kit/claude`), and this tells the server what
// came of it at `/api/answered`. The workspaces on this Mac answer here
// anyway; their server asks Claude Code itself.

import { apiUrl, appConfig, isDesktop, type Place, publicUrl } from "./places";
import { messageOf } from "./problems";

/** What a server hands to Claude Code on this Mac (`code/kit/claude`'s `HandedQuestion`). */
export type HandedQuestion = {
  token: string;
  system: string;
  prompt: string;
  photos: { media_type: string; data: string }[];
  web: boolean;
  tools: string[];
  /** Seconds. */
  timeout?: number;
  max_turns?: number;
};

/** What Claude Code here made of it (`Answered`). */
export type Answered = {
  text?: string;
  read?: string[];
  stop?: string;
  not_answered?: "not_installed" | "signed_out" | "trouble";
  said?: string;
};

/** Whether Claude Code can answer here (`ClaudeCodeInfo`). */
export type ClaudeCodeInfo = {
  state: "ready" | "not_installed" | "signed_out" | "trouble";
  version?: string;
  /** How to install it, as a Terminal command. */
  install: string;
  problem?: string;
};

const settingKey = () => `${appConfig().key}:answer-here`;

/** Settings → Claude's "Claude Code on this Mac": on unless turned off, on this Mac. */
export function answersHere(): boolean {
  try {
    return localStorage.getItem(settingKey()) !== "off";
  } catch {
    return true;
  }
}

export function setAnswersHere(on: boolean) {
  try {
    if (on) localStorage.removeItem(settingKey());
    else localStorage.setItem(settingKey(), "off");
  } catch {}
}

/**
 * Whether what's asked in `place` goes to Claude Code here: in the desktop
 * app, on a server's workspace, unless turned off.
 */
export function asksHere(place: Place): boolean {
  return isDesktop && place.kind !== "mac" && answersHere();
}

/** The server's own address, which Claude Code reaches its tools at. */
const origin = (place: Place) => publicUrl("", place);

/** Whether Claude Code here can answer for the place's server. */
export function claudeHere(place: Place): Promise<ClaudeCodeInfo> {
  return appConfig().invoke<ClaudeCodeInfo>("claude_here", { origin: origin(place) });
}

/**
 * Answers what the server handed over with Claude Code here, then says
 * what came of it. The server goes on from there: it writes the answer, or
 * its key answers instead.
 */
export async function answerHere(place: Place, question: HandedQuestion): Promise<void> {
  let answered: Answered;
  try {
    answered = await appConfig().invoke<Answered>("answer_here", {
      origin: origin(place),
      question,
    });
  } catch (e) {
    answered = { not_answered: "trouble", said: messageOf(e) };
  }
  // Nothing to do when this fails: the server stops waiting on its own,
  // and says so.
  await fetch(apiUrl("/api/answered", place), {
    method: "POST",
    credentials: "omit",
    headers: { authorization: `Bearer ${question.token}`, "content-type": "application/json" },
    body: JSON.stringify(answered),
  }).catch(() => undefined);
}
