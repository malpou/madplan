<script>
  let { plan } = $props();

  const label = (s) => (s ?? "").replaceAll("_", " ");
  const dkk = (n) => (n ?? 0).toLocaleString("da-DK", { minimumFractionDigits: 0, maximumFractionDigits: 2 }) + " kr";

  const list = $derived(plan.shopping_list ?? []);
  const b = $derived(plan.budget ?? {});
  const groups = $derived(Object.entries(Object.groupBy(list, (i) => i.category)));
  const organic = $derived(list.filter((i) => i.organic).length);
  const inBasket = $derived(list.filter((i) => i.in_basket).length);
  const onSale = $derived(list.filter((i) => i.on_sale).length);
  const pct = $derived(b.limit_dkk ? Math.min(100, (100 * (b.total_dkk ?? 0)) / b.limit_dkk) : 0);
</script>

<div class="stack">
  <div class="card stack">
    <div class="row">
      <h2>Budget</h2>
      <span class="grow"></span>
      <b class={b.within_budget ? "ok" : "over"}>{dkk(b.total_dkk)}</b>
      <span class="muted">of {dkk(b.limit_dkk)}</span>
    </div>
    <div class="bar"><div class:over={!b.within_budget} style:width="{pct}%"></div></div>
    <div class="row muted small">
      <span>Groceries {dkk(b.groceries_dkk)}</span>·<span>Delivery {dkk(b.delivery_fee_dkk)}</span>·
      <span>{list.length} items</span>·
      {#if onSale}<span class="saved">{onSale} on sale, saved {dkk(b.savings_dkk)}</span>·{/if}{#if organic}<span>{organic} organic</span>·{/if}<span>{inBasket}/{list.length} in basket</span>
    </div>
  </div>

  {#if !list.length}
    <div class="empty">No shopping list yet. It's filled once the plan is approved.</div>
  {/if}

  {#each groups as [category, items]}
    <div class="card">
      <h3>{label(category)}</h3>
      <table>
        <thead>
          <tr><th>Item</th><th class="num">Qty</th><th>Nemlig product</th><th></th><th class="num">Price</th><th>Basket</th></tr>
        </thead>
        <tbody>
          {#each items as i}
            <tr>
              <td>{i.item}</td>
              <td class="num">{i.quantity} {i.unit}</td>
              <td class="muted">{i.nemlig_name ?? "—"}</td>
              <td>
                {#if i.organic}<span class="chip good" title="Organic">Ø</span>{/if}
                {#if i.non_organic_reason}<span class="chip warn" title="Not organic">{label(i.non_organic_reason)}</span>{/if}
                {#if i.budget_range}<span class="chip" title="Budget range">budget</span>{/if}
                {#if i.danish}<span class="chip">DK</span>{/if}
              </td>
              <td class="num">
                {#if i.on_sale}<span class="chip good">sale</span> <s class="muted">{dkk(i.regular_price_dkk)}</s>{/if}
                {dkk(i.price_dkk)}
              </td>
              <td>{i.in_basket ? "✓" : ""}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/each}
</div>

<style>
  .grow { flex: 1; }
  .small { font-size: 13px; }
  .ok { color: var(--good); font-size: 18px; }
  .saved { color: var(--good); }
  .over { color: var(--bad); font-size: 18px; }
  .bar { height: 8px; background: var(--chip); border-radius: 999px; overflow: hidden; }
  .bar div { height: 100%; background: var(--good); }
  .bar div.over { background: var(--bad); }
  h3 { text-transform: capitalize; margin-bottom: 4px; }
  td:first-child { width: 22%; }
</style>
