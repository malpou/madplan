<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Chat from "./lib/Chat.svelte";
  import PlanView from "./lib/PlanView.svelte";
  import ShoppingView from "./lib/ShoppingView.svelte";
  import PantryView from "./lib/PantryView.svelte";
  import { agent, send } from "./lib/agent.svelte.js";
  import { logError } from "./lib/telemetry.js";

  let snap = $state(null);
  let planFile = $state(null);
  let plan = $state(null);
  let tab = $state("plan");
  let validation = $state({}); // path -> errors
  let showIssues = $state(false);

  const issues = $derived(Object.entries(validation).filter(([, errs]) => errs.length));

  async function refresh() {
    snap = await invoke("snapshot");
    validation = Object.fromEntries(snap.validation.map((r) => [r.path, r.errors]));
    agent.running ||= snap.agent_running;
    if (!snap.plans.some((p) => p.file === planFile)) planFile = snap.plans[0]?.file ?? null;
    await loadPlan();
  }

  async function loadPlan() {
    plan = planFile
      ? await invoke("read_plan", { file: planFile }).catch((e) => {
          logError("read_plan_failed", { file: planFile, error: String(e) });
          return null;
        })
      : null;
  }

  $effect(() => {
    refresh();
    const unlisten = listen("files-changed", async () => {
      const known = new Set(snap?.plans.map((p) => p.file));
      // A new plan file appeared: jump to it, since that's what the agent is working on.
      await refresh();
      const added = snap.plans.find((p) => !known.has(p.file));
      if (added) {
        planFile = added.file;
        await loadPlan();
      }
    });
    return () => unlisten.then((f) => f());
  });

  // Open links (agent replies, recipe sources) in the browser instead of navigating the app away.
  function onClick(e) {
    const a = e.target.closest?.("a[href]");
    if (!a || !/^https?:/.test(a.href)) return;
    e.preventDefault();
    openUrl(a.href);
  }

  const current = $derived(snap?.plans.find((p) => p.file === planFile));
  const needsSetup = $derived(snap && (!snap.profile || !snap.pantry));
</script>

<svelte:window onclick={onClick} />

<div class="app">
  <main>
    <nav class="row">
      <h1>Madplan</h1>
      {#if snap?.plans.length}
        <select bind:value={planFile} onchange={loadPlan}>
          {#each snap.plans as p}<option value={p.file}>{p.week ?? p.file} · {p.status ?? "?"}</option>{/each}
        </select>
      {/if}
      <div class="tabs">
        {#each [["plan", "Plan"], ["shopping", "Shopping"], ["pantry", "Pantry"]] as [id, name]}
          <button class:active={tab === id} onclick={() => (tab = id)}>{name}</button>
        {/each}
      </div>
      <span class="grow"></span>
      <button class="ghost status" onclick={() => (showIssues = !showIssues)} title="Background schema validation">
        {#if issues.length}<span class="bad">✗ {issues.length} file{issues.length > 1 ? "s" : ""} invalid</span>
        {:else}<span class="good">✓ data valid</span>{/if}
      </button>
    </nav>

    {#if showIssues && issues.length}
      <div class="card issues">
        {#each issues as [path, errs]}
          <div><b>{path}</b><ul>{#each errs as e}<li>{e}</li>{/each}</ul></div>
        {/each}
        <button disabled={agent.running} onclick={() => send("Fix the validation errors in: " + issues.map(([p]) => p).join(", "))}>
          Ask the agent to fix
        </button>
      </div>
    {/if}

    <div class="content">
      {#if !snap}
        <div class="empty">Loading…</div>
      {:else if tab === "pantry"}
        {#if snap.pantry}
          <PantryView pantry={snap.pantry} defs={snap.defs} />
        {:else}
          <div class="empty">The pantry is created during setup. Use <b>Get started</b> in the chat.</div>
        {/if}
      {:else if !plan}
        <div class="empty">
          {#if needsSetup}
            <h2>Welcome to Madplan</h2>
            <p>Before the first plan, the agent interviews you about your household and fills in your profile, recipe sources and pantry.</p>
            <button class="primary" disabled={agent.running} onclick={() => send("Hi! We'd like to set up Madplan for our household.")}>Get started</button>
          {:else}
            {#if current}Couldn't read {current.file} — see the validation status.{:else}No plans yet.{/if}
            <br />
            <button class="primary" disabled={agent.running} onclick={() => send("Let's plan next week.")}>Plan next week</button>
          {/if}
        </div>
      {:else if tab === "plan"}
        <PlanView {plan} />
      {:else}
        <ShoppingView {plan} />
      {/if}
    </div>
  </main>
  <Chat {needsSetup} />
</div>

<style>
  .app { display: grid; grid-template-columns: minmax(0, 1fr) 420px; height: 100%; }
  main { display: flex; flex-direction: column; min-height: 0; }
  nav { padding: 10px 16px; border-bottom: 1px solid var(--border); background: var(--panel); }
  h1 { font-size: 16px; margin-right: 8px; }
  .grow { flex: 1; }
  .tabs { display: inline-flex; gap: 2px; margin-left: 8px; }
  .tabs button { border-color: transparent; background: transparent; }
  .tabs button.active { background: var(--panel-2); border-color: var(--border); }
  .status { font-size: 13px; }
  .good { color: var(--good); }
  .bad { color: var(--bad); }
  .issues { margin: 12px 16px 0; font-size: 13px; }
  .issues ul { margin: 4px 0 8px; color: var(--bad); }
  .content { flex: 1; overflow-y: auto; padding: 16px; }
</style>
