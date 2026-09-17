import type { ChatContext, ChatSource } from "../../lib/api";

export type ChatTurn = {
  /** Stable id so streaming patches hit the right bubble. */
  id?: string;
  role: "user" | "assistant";
  content: string;
  mode?: "llm" | "retrieval";
  sources?: ChatSource[];
  contexts?: ChatContext[];
  error?: string;
};
