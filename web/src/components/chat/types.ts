import type { ChatContext, ChatSource } from "../../lib/api";

export type ChatTurn = {
  /** Stable id so streaming patches hit the right bubble. */
  id?: string;
  role: "user" | "assistant";
  content: string;
  /** Still receiving deltas: an unterminated mermaid fence is expected, not an error. */
  streaming?: boolean;
  mode?: "llm" | "retrieval";
  sources?: ChatSource[];
  contexts?: ChatContext[];
  error?: string;
};
