import { escapeHtml, invoke } from "../api.js";
import { icon } from "../icons.js";
import { createTableStore } from "../table.js";
import { mountRatesCardGrid, openRateAddModal } from "../rates-cards.js";
import { mountSearchBox, matchesQuery } from "../search.js";
import { app, state, setPanel } from "../core.js";

let ratesStore = null;

export async function renderRatesPanel() {
  const rates = await invoke("list_cornice_rates");
  const filter = state.ratesFilter || "";
  // The search box lives in its own container so live filtering (renderRatesContent)
  // doesn't re-mount it — re-mounting on every keystroke dropped focus and killed
  // the results dropdown.
  setPanel("Cornice Rates", "", `<div data-rates-search></div><div data-rates-content></div>`);

  ratesStore = createTableStore({
    commit: {
      add: (values) => invoke("save_cornice_rate", { input: { id: null, ...values } }),
      save: (values) => invoke("save_cornice_rate", { input: values }),
      remove: (id) => invoke("delete_cornice_rate", { id }),
    },
    onDone: () => {
      ratesStore = null;
      renderRatesPanel();
    },
  });

  const searchBox = mountSearchBox(app.querySelector("[data-rates-search]"), {
    placeholder: "Search by cornice name…",
    minChars: 1,
    searchFn: (query) => rates.filter((rate) => matchesQuery(rate.model, query)),
    renderMatch: (rate) =>
      `${escapeHtml(rate.series ? `${rate.series} · ` : "")}${escapeHtml(rate.model)}<span class="search-result-meta">${escapeHtml(rate.unit || "Custom")}</span>`,
    onQuery: (query) => {
      if (query === (state.ratesFilter || "")) return;
      state.ratesFilter = query;
      renderRatesContent(rates);
    },
    onSelect: (rate) => {
      state.ratesFilter = rate.model;
      renderRatesContent(rates);
    },
  });
  if (filter) searchBox.setQuery(filter, { trigger: false });

  renderRatesContent(rates);
}

function renderRatesContent(rates) {
  const store = ratesStore;
  const contentEl = app.querySelector("[data-rates-content]");
  if (!store || !contentEl) return;
  const filter = state.ratesFilter || "";
  const visible = filter ? rates.filter((rate) => matchesQuery(rate.model, filter)) : rates;
  const groups = {};
  for (const rate of visible) {
    const series = rate.series || "(no series)";
    (groups[series] ||= []).push(rate);
  }
  const seriesNames = Object.keys(groups).sort();
  const displaySeriesNames = seriesNames.length
    ? seriesNames
    : rates.length
      ? []
      : ["New series"];

  contentEl.innerHTML = `
    ${
      filter
        ? `<div class="message" style="margin-bottom:12px">${visible.length} of ${rates.length} rates match "${escapeHtml(filter)}"</div>`
        : ""
    }
    ${
      displaySeriesNames.length
        ? `<div class="rate-series-layout">
      ${displaySeriesNames
        .map(
          (series) => `
        <section class="rate-group">
          <div class="rate-group-head">
            <h3>${escapeHtml(series)}</h3>
            <button class="icon ghost" data-rate-add data-series="${escapeHtml(series)}" title="Add cornice">${icon("plus", 18)}</button>
          </div>
          <div data-rate-group="${escapeHtml(series)}"></div>
        </section>`,
        )
        .join("")}
      </div>`
        : `<div class="empty">No rates match "${escapeHtml(filter)}".</div>`
    }
  `;

  const actionsEl = app.querySelector("[data-panel-actions]");
  const mounted = {};
  displaySeriesNames.forEach((series) => {
    mounted[series] = mountRatesCardGrid(
      contentEl.querySelector(`[data-rate-group="${CSS.escape(series)}"]`),
      store,
      {
        rows: groups[series] || [],
        series,
        tableId: `series-${CSS.escape(series)}`,
        editable: true,
        actionsEl,
        refreshFn: renderRatesPanel,
      },
    );
  });
  store.renderActions(actionsEl, { refreshFn: renderRatesPanel });

  contentEl.querySelectorAll("[data-rate-add]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const series = btn.dataset.series;
      openRateAddModal(series, (values) => {
        store.addNew(`series-${CSS.escape(series)}`, { series, ...values });
        mounted[series].render();
        store.renderActions(actionsEl, { refreshFn: renderRatesPanel });
      });
    });
  });
  ensureRateMenuListener();
}

let rateMenuListenerAdded = false;
function ensureRateMenuListener() {
  if (rateMenuListenerAdded) return;
  rateMenuListenerAdded = true;
  document.addEventListener("click", closeAllRateMenus);
}
function closeAllRateMenus() {
  app.querySelectorAll(".rate-card-popover").forEach((m) => m.remove());
}
