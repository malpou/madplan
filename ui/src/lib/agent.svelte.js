// Shared chat state, fed by the backend's `agent-event` stream (claude stream-json + local events).
// Sending while the agent works steers it: the message shows as pending until claude echoes it back.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { logError } from "./telemetry.js";

export const agent = $state({ messages: [], running: false });

export async function send(text) {
  text = text.trim();
  if (!text) return;
  try {
    await invoke("agent_send", { message: text });
  } catch (e) {
    logError("agent_send_failed", { error: String(e) });
    agent.messages.push({ role: "error", text: String(e) });
  }
}

export const stop = () => invoke("agent_stop");

export async function newChat() {
  try {
    await invoke("agent_new_session");
    agent.messages = [];
  } catch (e) {
    logError("new_chat_failed", { error: String(e) });
    agent.messages.push({ role: "error", text: String(e) });
  }
}

function toolSummary(name, input = {}) {
  const short = (p) => p.split("/").slice(-2).join("/");
  if (input.file_path) return short(input.file_path);
  if (input.command) return input.command;
  if (input.query) return input.query;
  if (input.url) return input.url;
  if (input.pattern) return input.pattern;
  const first = Object.values(input).find((v) => typeof v === "string");
  return first ?? "";
}

function prettyTool(name) {
  const m = name.match(/^mcp__(.+?)__(.+)$/);
  return m ? `${m[1]} · ${m[2]}` : name;
}

function handle(ev) {
  if (ev.type === "local") {
    if (ev.kind === "prompt") agent.messages.push({ role: "user", text: ev.text });
    if (ev.kind === "autofix") agent.messages.push({ role: "auto", text: ev.text });
    if (ev.kind === "steer") agent.messages.push({ role: "user", text: ev.text, pending: true });
    if (ev.kind === "prompt" || ev.kind === "autofix" || ev.kind === "busy") agent.running = true;
    if (ev.kind === "done") {
      agent.running = false;
      if (!ev.success) agent.messages.push({ role: "error", text: ev.stderr || "Agent stopped." });
    }
    return;
  }
  if (ev.type === "assistant") {
    for (const block of ev.message?.content ?? []) {
      if (block.type === "text" && block.text.trim()) agent.messages.push({ role: "assistant", text: block.text });
      if (block.type === "tool_use")
        agent.messages.push({ role: "tool", id: block.id, name: prettyTool(block.name), summary: toolSummary(block.name, block.input) });
    }
    return;
  }
  if (ev.type === "user") {
    const content = ev.message?.content;
    // Replayed user message: a steering message was delivered to the agent.
    const text = typeof content === "string" ? content : content?.filter((b) => b.type === "text").map((b) => b.text).join("");
    if (text) {
      const pending = agent.messages.find((m) => m.pending && m.text === text);
      if (pending) pending.pending = false;
      return;
    }
    for (const block of content ?? []) {
      if (block.type !== "tool_result" || !block.is_error) continue;
      const tool = agent.messages.findLast((m) => m.role === "tool" && m.id === block.tool_use_id);
      if (tool) tool.error = typeof block.content === "string" ? block.content : "failed";
    }
    return;
  }
  if (ev.type === "result" && ev.is_error) {
    agent.messages.push({ role: "error", text: ev.result || ev.subtype || "Agent error" });
  }
}

listen("agent-event", (e) => handle(e.payload));
