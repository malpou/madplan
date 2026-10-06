<script>
  import { invoke } from "@tauri-apps/api/core";
  import { untrack } from "svelte";
  import { logError } from "./telemetry.js";

  let { pantry, defs } = $props();

  const STATUSES = ["stocked", "low", "out"];
  const label = (s) => (s ?? "").replaceAll("_", " ");
  const today = () => {
    const d = new Date();
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  };

  let items = $state([]);
  let dirty = $state(false);
  let changedOnDisk = $state(false);
  let errors = $state([]);
  let saving = $state(false);
  let filter = $state("");

  // Take the file's contents unless the user has unsaved edits.
  $effect(() => {
    const fresh = structuredClone($state.snapshot(pantry?.items ?? []));
    untrack(() => {
      if (dirty) changedOnDisk = true;
      else items = fresh;
    });
  });

  function reload() {
    items = structuredClone($state.snapshot(pantry?.items ?? []));
    dirty = changedOnDisk = false;
    errors = [];
  }

  const edit = () => (dirty = true);

  function add() {
    items.unshift({ name: "", category: "other", status: "stocked" });
    filter = "";
    edit();
  }

  function remove(i) {
    items.splice(i, 1);
    edit();
  }

  // Drop empty optional fields so the YAML stays clean.
  function clean(item) {
    const out = {};
    for (const [k, v] of Object.entries(item)) {
      if (v === "" || v === null || v === undefined || (k === "staple" && v === false)) continue;
      out[k] = k === "quantity" ? Number(v) : v;
    }
    return out;
  }

  async function save() {
    saving = true;
    try {
      const report = await invoke("save_pantry", { pantry: { updated: today(), items: items.map(clean) } });
      errors = report.errors;
      if (!errors.length) dirty = changedOnDisk = false;
    } catch (e) {
      logError("pantry_save_failed", { error: String(e) });
      errors = [String(e)];
    } finally {
      saving = false;
    }
  }

  const visible = $derived(
    items.map((item, i) => ({ item, i })).filter(({ item }) => !filter || `${item.name} ${item.name_da ?? ""}`.toLowerCase().includes(filter.toLowerCase()))
  );
</script>

<div class="stack">
  <div class="card row">
    <h2>Pantry</h2>
    <span class="muted">updated {pantry?.updated}</span>
    <span class="grow"></span>
    <input placeholder="Filter…" bind:value={filter} />
    <button onclick={add}>Add item</button>
    {#if dirty}<button onclick={reload}>Discard</button>{/if}
    <button class="primary" onclick={save} disabled={!dirty || saving}>Save</button>
  </div>

  {#if changedOnDisk}
    <div class="card warn">pantry.yaml changed on disk while you were editing. <button onclick={reload}>Load latest</button> (drops your edits)</div>
  {/if}
  {#if errors.length}
    <div class="card bad">
      Not saved — fix these first:
      <ul>{#each errors as e}<li>{e}</li>{/each}</ul>
    </div>
  {/if}

  <div class="card">
    <table>
      <thead>
        <tr><th>Name</th><th>Danish</th><th>Category</th><th>Status</th><th>Qty</th><th>Unit</th><th>Staple</th><th></th></tr>
      </thead>
      <tbody>
        {#each visible as { item, i } (item)}
          <tr>
            <td><input bind:value={item.name} oninput={edit} /></td>
            <td><input bind:value={item.name_da} oninput={edit} /></td>
            <td>
              <select bind:value={item.category} onchange={edit}>
                {#each defs?.category?.enum ?? [] as c}<option value={c}>{label(c)}</option>{/each}
              </select>
            </td>
            <td>
              <div class="seg">
                {#each STATUSES as s}
                  <button class:on={item.status === s} class={s} onclick={() => { item.status = s; edit(); }}>{s}</button>
                {/each}
              </div>
            </td>
            <td><input class="qty" type="number" min="0" step="any" bind:value={item.quantity} oninput={edit} /></td>
            <td>
              <select bind:value={item.unit} onchange={edit}>
                <option value={undefined}></option>
                {#each defs?.unit?.enum ?? [] as u}<option value={u}>{u}</option>{/each}
              </select>
            </td>
            <td><input type="checkbox" bind:checked={item.staple} onchange={edit} /></td>
            <td><button class="ghost" title="Remove" onclick={() => remove(i)}>✕</button></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>

<style>
  .grow { flex: 1; }
  .warn { border-color: var(--warn); }
  .bad { border-color: var(--bad); color: var(--bad); }
  td input:not([type="checkbox"]) { width: 100%; }
  .qty { width: 80px !important; }
  .seg { display: inline-flex; }
  .seg button { border-radius: 0; padding: 3px 8px; font-size: 12px; }
  .seg button:first-child { border-radius: 6px 0 0 6px; }
  .seg button:last-child { border-radius: 0 6px 6px 0; }
  .seg button.on.stocked { background: var(--good); color: var(--accent-text); }
  .seg button.on.low { background: var(--warn); color: #1d1d1b; }
  .seg button.on.out { background: var(--bad); color: #fff; }
</style>
