import type { ChatContext, ChatSource } from "../../lib/api";

export type ChatTurn = {
  role: "user" | "assistant";
  content: string;
  mode?: "llm" | "retrieval";
  sources?: ChatSource[];
  contexts?: ChatContext[];
};
