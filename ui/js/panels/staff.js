import { escapeHtml, formatAction, invoke, setBusy, todayIso, weekStartIso } from "../api.js";
import { icon } from "../icons.js";
import { alertModal, confirmModal, promptModal } from "../modals.js";
import { createTableStore, mountInlineTable } from "../table.js";
import { mountRatesCardGrid } from "../rates-cards.js";
import { mountSearchBox, matchesQuery } from "../search.js";
import { app, state, setPanel, table, shiftIso, parsePrevValues, corniceLogCellHtml } from "../core.js";
import { mouldBoardHtml } from "./admin-moulds.js";

export async function renderStaffClock(message = "") {
  const [events, status] = await Promise.all([
    invoke("list_clock_events", { date: todayIso() }),
    invoke("get_clock_status", { employeeId: state.currentStaff.id }),
  ]);
  const nextAction = status.today_state === "in" ? "clock_out" : "clock_in";
  setPanel(
    "Clock",
    `<button class="primary" data-clock>${nextAction === "clock_in" ? "Clock In" : "Clock Out"}</button>`,
    `
      ${message ? `<div class="message">${escapeHtml(message)}</div>` : ""}
      ${table(
        ["Time", "Employee", "Action", "Note"],
        events
          .filter((event) => event.employee_id === state.currentStaff.id)
          .map((event) => ({
            review: event.needs_admin_review,
            cells: [
              event.timestamp.replace("T", " "),
              event.employee_name,
              formatAction(event.action),
              event.note,
            ],
          })),
      )}
    `,
  );
  app.querySelector("[data-clock]").addEventListener("click", async (event) => {
    const button = event.currentTarget;
    setBusy(button);
    try {
      const warning =
        nextAction === "clock_in" && status.missed_yesterday_clock_out
          ? "You didn't clock out yesterday."
          : null;
      await confirmModal({
        title: nextAction === "clock_in" ? "Clock in now?" : "Clock out now?",
        confirmLabel: nextAction === "clock_in" ? "Clock In" : "Clock Out",
        warning,
      });
      const result = await invoke("record_clock_event", {
        request: {
          employee_id: state.currentStaff.id,
          action: nextAction,
          source: state.sessionSource || "password",
        },
      });
      renderStaffClock(`${formatAction(result.action)} recorded at ${result.timestamp.slice(11)}`);
    } catch (error) {
      if (String(error.message || error) === "Cancelled.") {
        setBusy(button, false);
      } else {
        renderStaffClock(String(error.message || error));
      }
    } finally {
      setBusy(button, false);
    }
  });
}

let staffLogStore = null;

function corniceRateSearchBox(onNewRow) {
  return mountSearchBox(app.querySelector("[data-cornice-search]"), {
    placeholder: "Search cornice model…",
    searchFn: async (query) => {
      const resp = await invoke("search_cornice_rates", { request: { query } });
      state.corniceRateMatches = resp.matches || [];
      return resp.matches || [];
    },
    renderMatch: (match) =>
      `${escapeHtml(match.series ? `${match.series} · ` : "")}${escapeHtml(match.model)}<span class="search-result-meta">${escapeHtml(match.unit || "Custom")}</span>`,
    emptyText: "No match found — will be logged as unknown/custom.",
    onSelect: (match) => {
      const modelInput =
        app.querySelector('input[data-key="model"]:focus') ||
        app.querySelector('input[data-key="model"]');
      if (modelInput) {
        modelInput.value = match.model;
        modelInput.dispatchEvent(new Event("change", { bubbles: true }));
        return;
      }
      // No cell being edited: start a new row with the model pre-filled so
      // the staff member only has to enter the lengths.
      if (onNewRow) onNewRow(match);
    },
  });
}

export async function renderStaffCornice() {
  const logs = await invoke("list_cornice_logs", {
    employeeId: state.currentStaff.id,
    date: null,
    weekStart: null,
  });
  const today = todayIso();
  const currentWeek = weekStartIso();

  const weeks = new Map();
  for (const log of logs) {
    if (!weeks.has(log.week_start)) weeks.set(log.week_start, new Map());
    const days = weeks.get(log.week_start);
    if (!days.has(log.log_date)) days.set(log.log_date, []);
    days.get(log.log_date).push(log);
  }
  if (!weeks.has(currentWeek)) weeks.set(currentWeek, new Map());
  const currentDays = weeks.get(currentWeek);
  if (!currentDays.has(today)) currentDays.set(today, []);

  const weekNames = [...weeks.keys()].sort().reverse();
  const body = weekNames
    .map((weekStart) => {
      const days = weeks.get(weekStart);
      const dayNames = [...days.keys()].sort().reverse();
      const weekTotal = [...days.values()].flat().reduce((sum, log) => sum + log.total_units, 0);
      const dayBoxes = dayNames
        .map((date) => {
          const dayLogs = days.get(date);
          const dayTotal = dayLogs.reduce((sum, log) => sum + log.total_units, 0);
          return `
            <div class="day-box">
              <h3><span>${escapeHtml(date)}</span><small>${dayTotal.toFixed(2)} units</small></h3>
              <div data-day-table="${escapeHtml(date)}"></div>
            </div>`;
        })
        .join("");
      return `
        <div class="week-box">
          <h3><span>${escapeHtml(weekStart)} – ${escapeHtml(shiftIso(weekStart, 6))}</span><small>${weekTotal.toFixed(2)} units</small></h3>
          ${dayBoxes}
        </div>`;
    })
    .join("");
  setPanel(
    "Cornice Log",
    "",
    `
      <div data-cornice-search></div>
      ${body || `<div class="empty">No log entries yet.</div>`}
    `,
  );

  try {
    const resp = await invoke("search_cornice_rates", { request: { query: "" } });
    state.corniceRateAll = resp.matches || [];
    state.corniceRateMatches = state.corniceRateAll;
  } catch {
    /* ignore */
  }

  if (!staffLogStore) {
    staffLogStore = createTableStore({
      commit: {
        add: (values) => {
          const match = (state.corniceRateAll || []).find(
            (item) => item.model.toLowerCase() === String(values.model).trim().toLowerCase(),
          );
          return invoke("add_cornice_log", {
            input: {
              employee_id: state.currentStaff.id,
              log_date: todayIso(),
              series: match?.series || "",
              model: values.model,
              lengths: values.lengths,
            },
          });
        },
        save: (values) =>
          invoke("update_cornice_log", {
            input: {
              id: values.id,
              actor_id: state.currentStaff.id,
              series: values.series,
              model: values.model,
              lengths: values.lengths,
            },
          }),
        remove: (id) => invoke("delete_cornice_log", { id, actorId: state.currentStaff.id }),
      },
      onDone: () => {
        staffLogStore = null;
        renderStaffCornice();
      },
    });
  }
  const store = staffLogStore;
  const mountedDays = {};
  weekNames.forEach((weekStart) => {
    const days = weeks.get(weekStart);
    [...days.keys()].sort().reverse().forEach((date) => {
      mountedDays[date] = mountInlineTable(
        app.querySelector(`[data-day-table="${CSS.escape(date)}"]`),
        store,
        {
          columns: [
            {
              key: "model",
              label: "Model",
              type: "text",
              editable: true,
              cellHtml: (log, col) => corniceLogCellHtml(log, col.key),
            },
            {
              key: "lengths",
              label: "Lengths",
              type: "number",
              editable: true,
              align: "right",
              cellHtml: (log, col) => corniceLogCellHtml(log, col.key),
            },
            {
              key: "unit",
              label: "Unit",
              type: "text",
              editable: false,
              cellHtml: (log) => escapeHtml(log.unit || "Custom"),
            },
            {
              key: "total_units",
              label: "Units",
              type: "number",
              editable: false,
              align: "right",
              cellHtml: (log) => log.total_units.toFixed(2),
            },
          ],
          rows: days.get(date),
          tableId: `day-${CSS.escape(date)}`,
          emptyText: "No entries",
          rowClass: (log) => (parsePrevValues(log)?.deleted ? "staged-delete" : ""),
          actionsEl: app.querySelector("[data-panel-actions]"),
          refreshFn: renderStaffCornice,
          extraActions: `<button class="ghost" data-add-log>${icon("plus", 18)} Add</button>`,
          onActionsRendered: (el) => {
            el.querySelector("[data-add-log]")?.addEventListener("click", () => {
              store.addNew(`day-${CSS.escape(today)}`, {
                id: null,
                series: "",
                model: "",
                lengths: 0,
                unit: "",
                total_units: 0,
              });
              mountedDays[today]?.render();
            });
          },
        },
      );
    });
  });

  corniceRateSearchBox((match) => {
    store.addNew(`day-${CSS.escape(today)}`, {
      id: null,
      series: match.series || "",
      model: match.model,
      lengths: 0,
      unit: match.unit || "",
      total_units: 0,
    });
    mountedDays[today]?.render();
  });
}

export async function renderStaffProduction() {
  const allProduction = (state.currentStaff.permissions || []).includes("daily_production_all");
  const logs = await invoke("list_production_logs", {
    employeeId: allProduction ? null : state.currentStaff.id,
    date: null,
  });
  setPanel(
    allProduction ? "Production Log (All Staff)" : "Production Log",
    "",
    `
      <form class="form-grid" data-production-form>
        <label>Item<input name="item" required /></label>
        <label>Quantity<input name="quantity" type="number" min="1" required /></label>
        <label class="wide">Notes<textarea name="notes"></textarea></label>
        <div class="wide panel-actions"><button class="primary" type="submit">Add Log</button></div>
      </form>
      ${table(
        ["Date", "Item", "Quantity", "Notes"],
        logs.map((log) => ({
          cells: [log.log_date, log.item, log.quantity, log.notes],
        })),
      )}
    `,
  );
  app.querySelector("[data-production-form]").addEventListener("submit", async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    try {
      await invoke("add_production_log", {
        input: {
          employee_id: state.currentStaff.id,
          log_date: todayIso(),
          item: form.get("item"),
          quantity: Number(form.get("quantity")),
          notes: form.get("notes"),
        },
      });
      renderStaffProduction();
    } catch (error) {
      await alertModal({ title: "Production Log", message: String((error && error.message) || error) });
    }
  });
}

export async function renderStaffOverstock() {
  const items = await invoke("list_overstock");
  setPanel(
    "Overstock",
    "",
    `
      <form class="form-grid" data-overstock-form>
        <label>Model<input name="model" required /></label>
        <label>Quantity<input name="quantity" type="number" min="1" required /></label>
        <label>Aisle<input name="aisle" required /></label>
        <label>Notes<input name="notes" /></label>
        <div class="wide panel-actions"><button class="primary" type="submit">Add Overstock</button></div>
      </form>
      ${table(
        ["Model", "Quantity", "Aisle", "Updated", "Notes"],
        items.map((item) => ({
          cells: [item.model, item.quantity, item.aisle, item.updated_at.replace("T", " "), item.notes],
        })),
      )}
    `,
  );
  app.querySelector("[data-overstock-form]").addEventListener("submit", async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    try {
      await invoke("add_overstock", {
        input: {
          employee_id: state.currentStaff.id,
          model: form.get("model"),
          quantity: Number(form.get("quantity")),
          aisle: form.get("aisle"),
          notes: form.get("notes"),
        },
      });
      renderStaffOverstock();
    } catch (error) {
      await alertModal({ title: "Overstock", message: String((error && error.message) || error) });
    }
  });
}

export async function renderStaffDeliveries() {
  const deliveries = await invoke("list_deliveries", { date: todayIso() });
  setPanel(
    "Deliveries",
    "",
    `
      <form class="form-grid" data-delivery-form>
        <label class="wide">Address<input name="address" required /></label>
        <label class="wide">Items<textarea name="items" required></textarea></label>
        <label class="wide">Notes<textarea name="notes"></textarea></label>
        <div class="wide panel-actions"><button class="primary" type="submit">Add Delivery</button></div>
      </form>
      ${table(
        ["Date", "Address", "Items", "Notes"],
        deliveries
          .filter((delivery) => delivery.driver_id === state.currentStaff.id)
          .map((delivery) => ({
            cells: [delivery.delivery_date, delivery.address, delivery.items, delivery.notes],
          })),
      )}
    `,
  );
  app.querySelector("[data-delivery-form]").addEventListener("submit", async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    try {
      await invoke("add_delivery", {
        input: {
          driver_id: state.currentStaff.id,
          delivery_date: todayIso(),
          address: form.get("address"),
          items: form.get("items"),
          notes: form.get("notes"),
        },
      });
      renderStaffDeliveries();
    } catch (error) {
      await alertModal({ title: "Deliveries", message: String((error && error.message) || error) });
    }
  });
}

export async function renderDriverDispatchView() {
  const pending = await invoke("list_dispatch_orders", { status: "pending" });
  const inProgress = await invoke("list_dispatch_orders", { status: "in_progress" });
  let body = "";

  if (pending.length > 0 || inProgress.length > 0) {
    const allOrders = [...inProgress, ...pending];
    body = table(
      ["Model", "Qty", "Location", "Status", "Created", ""],
      allOrders.map(o => ({
        review: o.status === "pending",
        cells: [
          o.cornice_model,
          o.quantity,
          o.delivery_location,
            `<span class="tag ${o.status === 'delivered' ? 'tag-ok' : o.status === 'pending' ? 'tag-err' : 'tag-warn'}">${escapeHtml(o.status)}</span>`,
          o.created_at ? o.created_at.replace("T", " ") : "—",
          `<button data-deliver-order="${o.id}">Mark Delivered</button>`,
        ],
      }))
    );
  } else {
    body = `<div class="empty">No pending dispatch orders.</div>`;
  }

  setPanel("Dispatch Orders", `<button class="ghost" data-refresh>Refresh</button>`, body);
  app.querySelector("[data-refresh]")?.addEventListener("click", renderDriverDispatchView);

  app.querySelectorAll("[data-deliver-order]").forEach(btn => {
    btn.addEventListener("click", async () => {
      const remarks =
        (await promptModal({
          title: "Mark Delivered",
          label: "Delivery remarks (optional)",
          confirmLabel: "Deliver",
        }).catch(() => "")) || "";
      try {
        await invoke("update_dispatch_order", {
          input: {
            id: Number(btn.dataset.deliverOrder),
            cornice_model: "",
            quantity: 0,
            delivery_location: "",
            status: "delivered",
            remarks,
          },
          updatedBy: state.currentStaff.id,
        });
        renderDriverDispatchView();
      } catch (error) {
        await alertModal({ title: "Dispatch Orders", message: String((error && error.message) || error) });
      }
    });
  });
}

export async function renderStaffRates() {
  const rates = await invoke("list_cornice_rates");
  const filter = state.staffRatesFilter || "";
  const visible = filter ? rates.filter((rate) => matchesQuery(rate.model, filter)) : rates;
  const groups = {};
  for (const rate of visible) {
    const series = rate.series || "(no series)";
    (groups[series] ||= []).push(rate);
  }
  const seriesNames = Object.keys(groups).sort();
  const body = seriesNames.length
    ? `<div class="rate-series-layout">${seriesNames
        .map(
          (series) => `
        <section class="rate-group">
          <div class="rate-group-head"><h3>${escapeHtml(series)}</h3></div>
          <div data-rate-group="${escapeHtml(series)}"></div>
        </section>`,
        )
        .join("")}</div>`
    : `<div class="empty">${filter ? `No rates match "${escapeHtml(filter)}".` : "No rates yet."}</div>`;
  setPanel(
    "Cornice Rates (Read-Only)",
    `<button class="ghost" data-refresh>Refresh</button>`,
    `
      <div data-staff-rates-search></div>
      ${
        filter
          ? `<div class="message" style="margin-bottom:12px">${visible.length} of ${rates.length} rates match "${escapeHtml(filter)}"</div>`
          : ""
      }
      ${body}
    `,
  );
  seriesNames.forEach((series) => {
    mountRatesCardGrid(app.querySelector(`[data-rate-group="${CSS.escape(series)}"]`), null, {
      rows: groups[series] || [],
      series,
      tableId: `series-${CSS.escape(series)}`,
      editable: false,
    });
  });
  const searchBox = mountSearchBox(app.querySelector("[data-staff-rates-search]"), {
    placeholder: "Search by cornice name…",
    minChars: 1,
    searchFn: (query) => rates.filter((rate) => matchesQuery(rate.model, query)),
    renderMatch: (rate) =>
      `${escapeHtml(rate.series ? `${rate.series} · ` : "")}${escapeHtml(rate.model)}<span class="search-result-meta">${escapeHtml(rate.unit || "Custom")}</span>`,
    onQuery: (query) => {
      if (query === (state.staffRatesFilter || "")) return;
      state.staffRatesFilter = query;
      renderStaffRates();
    },
    onSelect: (rate) => {
      state.staffRatesFilter = rate.model;
      renderStaffRates();
    },
  });
  if (filter) searchBox.setQuery(filter, { trigger: false });
  app.querySelector("[data-refresh]").addEventListener("click", renderStaffRates);
}

export async function renderStaffMouldView() {
  const [locations, columns, items] = await Promise.all([
    invoke("list_mould_locations"),
    invoke("list_mould_location_columns"),
    invoke("list_mould_inventory"),
  ]);
  const filter = state.staffMouldFilter || "";
  const visibleItems = filter ? items.filter((item) => matchesQuery(item.mould_name, filter)) : items;
  const boxes = mouldBoardHtml({
    locations,
    columns,
    visibleItems,
    columnHeader: (col) => `<span>${escapeHtml(col.name)}</span>`,
    slot: (item) => `<div class="mould-slot"><span>${escapeHtml(item.mould_name)}</span></div>`,
    locationHeader: (loc, count) => `<span>${escapeHtml(loc.name)} <small>(${count})</small></span>`,
  });
  const unmatched = visibleItems.filter((item) => item.column_id == null);
  const unassigned = unmatched.length
    ? `<div class="day-box"><h3><span>Unassigned</span></h3>${table(
        ["Mould Name", "Location"],
        unmatched.map((item) => ({ cells: [item.mould_name, item.storage_location || "—"] })),
      )}</div>`
    : "";
  setPanel(
    "Mould Locations (Read-Only)",
    `<button class="ghost" data-refresh>Refresh</button>`,
    `
      <div data-staff-mould-search></div>
      ${
        filter
          ? `<div class="message" style="margin-bottom:12px">${visibleItems.length} of ${items.length} moulds match "${escapeHtml(filter)}"</div>`
          : ""
      }
      ${
        filter && !visibleItems.length
          ? `<div class="empty">No moulds match "${escapeHtml(filter)}".</div>`
          : `<div class="location-series-layout">${boxes}${unassigned}</div>`
      }
    `,
  );
  const searchBox = mountSearchBox(app.querySelector("[data-staff-mould-search]"), {
    placeholder: "Search by mould name…",
    minChars: 1,
    searchFn: (query) => items.filter((item) => matchesQuery(item.mould_name, query)),
    renderMatch: (item) => {
      const col = columns.find((c) => c.id === item.column_id);
      const loc = locations.find((l) => l.id === col?.location_id);
      const where = loc && col ? `${loc.name} · ${col.name}` : item.storage_location || "Unassigned";
      return `${escapeHtml(item.mould_name)}<span class="search-result-meta">${escapeHtml(where)}</span>`;
    },
    onQuery: (query) => {
      if (query === (state.staffMouldFilter || "")) return;
      state.staffMouldFilter = query;
      renderStaffMouldView();
    },
    onSelect: (item) => {
      state.staffMouldFilter = item.mould_name;
      renderStaffMouldView();
    },
  });
  if (filter) searchBox.setQuery(filter, { trigger: false });
  app.querySelector("[data-refresh]").addEventListener("click", renderStaffMouldView);
}

export async function renderStaffStockRO() {
  const items = await invoke("list_stock_items");
  setPanel(
    "Stocks (Read-Only)",
    `<button class="ghost" data-refresh>Refresh</button>`,
    table(
      ["Type", "Model", "Location", "Stock", "Reserved"],
      items.map((item) => ({
        cells: [item.item_type, item.model, item.location, item.stock, item.reserved],
      })),
    ),
  );
  app.querySelector("[data-refresh]").addEventListener("click", renderStaffStockRO);
}

export async function renderStaffPayroll() {
  try {
    const payroll = await invoke("get_payroll_week", {
      request: {
        employee_id: state.currentStaff.id,
        week_start: null,
      },
    });
    let body = `
      <div class="metric-row">
        <div class="metric"><span>Week</span><strong>${escapeHtml(payroll.week_start)}</strong></div>
        <div class="metric"><span>Hours</span><strong>${payroll.total_hours.toFixed(1)}h</strong></div>
        <div class="metric"><span>Status</span><strong class="${payroll.status === 'final' ? 'metric-ok' : payroll.status === 'unresolved' ? 'metric-err' : 'metric-warn'}">${escapeHtml(payroll.status)}</strong></div>
      </div>
    `;

    body += `<h3>Pay Breakdown</h3>`;
    body += `<div class="table-wrap"><table class="table"><tbody>`;
    body += `<tr><td>Base Pay</td><td><strong>$${payroll.base_pay.toFixed(2)}</strong></td></tr>`;
    body += `<tr><td>Known Units</td><td>${payroll.total_units_known.toFixed(1)}</td></tr>`;
    body += `<tr><td>Unit Threshold</td><td>${payroll.unit_threshold.toFixed(0)} <small>(${escapeHtml(payroll.threshold_note)})</small></td></tr>`;
    if (payroll.extra_unit_pay > 0) {
      const extraUnits = Math.round((payroll.total_units_known - payroll.unit_threshold) * 100) / 100;
      body += `<tr><td>Extra Unit Pay (${extraUnits} extra × $3.80)</td><td><strong>$${payroll.extra_unit_pay.toFixed(2)}</strong></td></tr>`;
    }
    if (payroll.gross_pay !== null && payroll.gross_pay !== undefined) {
      body += `<tr style="font-size:1.2em"><td><strong>Gross Pay</strong></td><td><strong>$${payroll.gross_pay.toFixed(2)}</strong></td></tr>`;
    }
    body += `</tbody></table></div>`;

    if (payroll.unknown_rate_details.length > 0) {
      body += `<div class="message" style="margin-top:1em">Unknown-rate cornices pending admin resolution:</div>`;
      body += table(
        ["Model", "Quantity"],
        payroll.unknown_rate_details.map(d => ({
          review: true,
          cells: [d.model, d.quantity],
        }))
      );
      body += `<div class="message">Equation: ${escapeHtml(payroll.pay_equation)}</div>`;
    } else {
      body += `<div class="message">Pay equation: ${escapeHtml(payroll.pay_equation)}</div>`;
    }

    setPanel("My Weekly Payroll", "", body);
  } catch (error) {
    setPanel("My Weekly Payroll", "", `<div class="message">Error: ${escapeHtml(String(error))}</div>`);
  }
}
