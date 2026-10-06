<script>
  import { agent, send } from "./agent.svelte.js";

  let { plan } = $props();

  const DAYS = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
  const label = (s) => (s ?? "").replaceAll("_", " ");

  const dish = (id) => plan.dishes?.find((d) => d.id === id);
  const training = (day) => plan.training?.find((t) => t.day === day);
  const dinner = (day) => plan.dinners?.find((d) => d.day === day);

  // The next step in the CLAUDE.md workflow, as a one-click message to the agent.
  const next = $derived.by(() => {
    const list = plan.shopping_list ?? [];
    switch (plan.status) {
      case "draft":
        return { label: "Approve plan", msg: "The plan looks good. Approve it, then fill the shopping list and price it on nemlig." };
      case "approved":
        if (!list.length || list.some((i) => !i.nemlig_product_id))
          return { label: "Price on nemlig", msg: "Fill the shopping list (minus pantry) and price every item on nemlig." };
        return { label: "Add to nemlig basket", msg: "Prices look good. Add the shopping list to the nemlig basket and show me the Sunday evening delivery slots. Don't check out." };
      case "ordered":
        return { label: "Mark week done", msg: "The week is over. Set the plan to done and update the pantry." };
      default:
        return null;
    }
  });

  const intensityClass = { hard: "bad", moderate: "warn", easy: "good", rest: "" };
</script>

<div class="stack">
  <div class="card row">
    <h2>Week {plan.week}</h2>
    <span class="chip">{plan.status}</span>
    <span class="muted">Delivery {plan.delivery?.date}{plan.delivery?.slot ? ` · ${plan.delivery.slot}` : ""}</span>
    <span class="grow"></span>
    {#if next}
      <button class="primary" disabled={agent.running} onclick={() => send(next.msg)}>{next.label}</button>
    {/if}
  </div>

  {#if plan.seasonal_focus?.length}
    <div class="row">
      <span class="muted">In season:</span>
      {#each plan.seasonal_focus as s}<span class="chip">{s}</span>{/each}
    </div>
  {/if}
  {#if plan.sale_focus?.length}
    <div class="row">
      <span class="muted">On sale:</span>
      {#each plan.sale_focus as d}
        <span class="chip good" title="Ends {d.ends}">
          {d.item} {d.price_dkk} kr{d.min_quantity ? ` (${d.min_quantity} for)` : ""} <s class="muted">{d.regular_price_dkk}</s>
        </span>
      {/each}
    </div>
  {/if}

  <div class="week">
    {#each DAYS as day}
      {@const t = training(day)}
      {@const d = dinner(day)}
      {@const ds = d?.dish_id ? dish(d.dish_id) : null}
      <div class="card day">
        <div class="row">
          <h3>{day}</h3>
          {#if t}<span class="chip {intensityClass[t.intensity]}">{t.intensity}</span>{/if}
        </div>
        {#if t?.activities?.length}<div class="muted small">{t.activities.join(", ")}</div>{/if}
        {#if d}
          <div class="dinner">
            <span class="chip">{d.source}</span>
            {#if ds}<div class="dish-name">{ds.name}</div>{/if}
            {#if d.assembly}<div class="muted small">{d.assembly}</div>{/if}
          </div>
        {/if}
      </div>
    {/each}
  </div>

  {#if plan.cook_sessions?.length}
    <h2>Cook sessions</h2>
    <div class="grid">
      {#each plan.cook_sessions as s}
        <div class="card">
          <div class="row"><h3>{s.day}</h3><span class="muted">~{s.est_minutes} min</span></div>
          <div class="small">{s.dishes.map((id) => dish(id)?.name ?? id).join(" · ")}</div>
          <ol class="small">{#each s.prep_order as step}<li>{step}</li>{/each}</ol>
        </div>
      {/each}
    </div>
  {/if}

  <h2>Dishes</h2>
  <div class="grid">
    {#each plan.dishes ?? [] as d}
      <div class="card">
        <div class="row">
          <h3>{d.name}</h3>
          {#if d.new}<span class="chip good">new</span>{/if}
          {#if d.rating}<span class="chip">{"★".repeat(d.rating)}</span>{/if}
        </div>
        <div class="row small muted">
          <span>{label(d.cuisine)}</span>·<span><b class="protein">{d.protein_g_per_serving} g</b> protein/serving</span>·
          <span>{d.servings} servings</span>·<span>{d.fridge_days} days in fridge</span>
          {#if d.freezes}·<span>freezes</span>{/if}
        </div>
        {#if d.source}
          <div class="small muted">
            Source: {d.source.source_id ?? d.source.type}{#if d.source.page}, p. {d.source.page}{/if}
            {#if d.source.url}· <a href={d.source.url} target="_blank" rel="noreferrer">link</a>{/if}
            {#if d.source.adapted}— {d.source.adapted}{/if}
          </div>
        {/if}
        {#if d.side_on_the_side?.length}<div class="small muted">On the side: {d.side_on_the_side.join(", ")}</div>{/if}
        <details>
          <summary class="small">Ingredients & steps</summary>
          <ul class="small">
            {#each d.ingredients as i}<li>{i.quantity} {i.unit} {i.name}{i.note ? ` (${i.note})` : ""}</li>{/each}
          </ul>
          <ol class="small">{#each d.steps as step}<li>{step}</li>{/each}</ol>
        </details>
      </div>
    {/each}
  </div>

  {#if plan.snacks?.length}
    <h2>Snacks</h2>
    <div class="grid">
      {#each plan.snacks as s}
        <div class="card">
          <div class="row"><h3>{s.name}</h3><span class="chip">{label(s.purpose)}</span></div>
          <div class="small muted">{s.ingredients.map((i) => `${i.quantity} ${i.unit} ${i.name}`).join(" · ")}</div>
          {#if s.prep || s.makes}<div class="small">{s.prep ?? ""} {s.makes ? `Makes ${s.makes}.` : ""}</div>{/if}
        </div>
      {/each}
    </div>
  {/if}

  {#if plan.notes?.length}
    <div class="card"><h3>Notes</h3><ul class="small">{#each plan.notes as n}<li>{n}</li>{/each}</ul></div>
  {/if}
</div>

<style>
  .grow { flex: 1; }
  .small { font-size: 13px; }
  .week { display: grid; grid-template-columns: repeat(7, minmax(0, 1fr)); gap: 8px; }
  .day h3 { text-transform: capitalize; }
  .dinner { margin-top: 8px; display: flex; flex-direction: column; gap: 4px; align-items: flex-start; }
  .dish-name { font-weight: 500; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 10px; }
  .protein { color: var(--good); }
  ol, ul { margin: 6px 0 0; padding-left: 20px; }
  details summary { cursor: pointer; margin-top: 6px; color: var(--muted); }
  @media (max-width: 1100px) { .week { grid-template-columns: repeat(4, minmax(0, 1fr)); } }
</style>
