<script>
  import { marked } from "marked";
  import DOMPurify from "dompurify";
  import { agent, send, stop, newChat } from "./agent.svelte.js";

  // True until the first-run interview has written data/profile.yaml.
  let { needsSetup = false } = $props();

  let draft = $state("");
  let log = $state();

  // Agent text can echo web content, so sanitize before rendering.
  const md = (text) => DOMPurify.sanitize(marked.parse(text));

  $effect(() => {
    agent.messages.length;
    log?.scrollTo({ top: log.scrollHeight });
  });

  function submit() {
    if (!draft.trim()) return;
    send(draft);
    draft = "";
  }

  function onKey(e) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  }
</script>

<section class="chat">
  <header class="row">
    <h2>Agent</h2>
    <span class="grow"></span>
    {#if agent.running}<span class="chip warn">working…</span>{/if}
    <button class="ghost" onclick={newChat} disabled={agent.running} title="Forget this conversation">New chat</button>
  </header>

  <div class="log" bind:this={log}>
    {#if agent.messages.length === 0}
      <div class="empty">
        {#if needsSetup}
          First, the agent gets to know your household: who you cook for, diet, routine, budget and pantry. It takes about five minutes.
          <br />
          <button class="primary" onclick={() => send("Hi! We'd like to set up Madplan for our household.")}>Get started</button>
        {:else}
          Ask the agent to plan the coming week, or use the buttons in the plan view.
          <br />
          <button class="primary" onclick={() => send("Let's plan next week.")}>Plan next week</button>
        {/if}
      </div>
    {/if}
    {#each agent.messages as m, i (i)}
      {#if m.role === "user"}
        <!-- one line: the bubble is white-space: pre-wrap -->
        <div class="bubble user" class:pending={m.pending} title={m.pending ? "Delivered after the agent's current step" : ""}>{m.text}{#if m.pending}<span class="queued">queued</span>{/if}</div>
      {:else if m.role === "assistant"}
        <div class="bubble assistant">{@html md(m.text)}</div>
      {:else if m.role === "tool"}
        <div class="tool" class:failed={m.error} title={m.error ?? ""}>
          <span class="name">{m.name}</span> <span class="muted">{m.summary}</span>
        </div>
      {:else if m.role === "auto"}
        <details class="auto">
          <summary>Validation errors sent back to the agent</summary>
          <pre>{m.text}</pre>
        </details>
      {:else}
        <div class="bubble error">{m.text}</div>
      {/if}
    {/each}
  </div>

  <form class="composer" onsubmit={(e) => { e.preventDefault(); submit(); }}>
    <textarea
      bind:value={draft}
      onkeydown={onKey}
      rows="3"
      placeholder={agent.running ? "Steer the agent: it reads this after its current step" : "Message the agent (Enter to send, Shift+Enter for newline)"}
    ></textarea>
    <div class="actions">
      <button class="primary" type="submit" disabled={!draft.trim()}>{agent.running ? "Steer" : "Send"}</button>
      {#if agent.running}<button type="button" onclick={stop}>Stop</button>{/if}
    </div>
  </form>
</section>

<style>
  .chat { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--panel); border-left: 1px solid var(--border); }
  header { padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .grow { flex: 1; }
  .log { flex: 1; overflow-y: auto; padding: 12px 14px; display: flex; flex-direction: column; gap: 8px; }
  /* Without this, flex squeezes messages to fit instead of letting the log scroll. */
  .log > :global(*) { flex-shrink: 0; }
  .bubble { padding: 8px 12px; border-radius: var(--radius); max-width: 100%; overflow-x: auto; }
  .bubble.user { background: var(--user-bubble); align-self: flex-end; white-space: pre-wrap; }
  .bubble.assistant { background: var(--panel-2); }
  .bubble.assistant :global(p) { margin: 0.3em 0; }
  .bubble.assistant :global(table) { font-size: 13px; margin: 6px 0; }
  .bubble.assistant :global(th), .bubble.assistant :global(td) { padding: 3px 6px; }
  .bubble.error { color: var(--bad); border: 1px solid var(--bad); white-space: pre-wrap; }
  .tool { font: 12px var(--mono); color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tool .name { color: var(--text); }
  .tool.failed .name { color: var(--bad); }
  .auto { font-size: 12px; color: var(--warn); }
  .auto pre { white-space: pre-wrap; font: 12px var(--mono); color: var(--muted); }
  .composer { display: flex; gap: 8px; padding: 10px 14px; border-top: 1px solid var(--border); align-items: flex-end; }
  .composer textarea { flex: 1; resize: none; }
  .actions { display: flex; flex-direction: column; gap: 6px; }
  .bubble.pending { opacity: 0.7; border: 1px dashed var(--muted); }
  .queued { display: block; font-size: 11px; color: var(--muted); margin-top: 2px; }
</style>
