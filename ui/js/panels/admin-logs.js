import { escapeHtml, formatAction, invoke, todayIso, weekStartIso } from "../api.js";
import { alertModal, promptModal } from "../auth.js";
import { app, state, setPanel, table, corniceLogCellHtml, emptyDbValues, dbField, collectDbValues, dbDisplay } from "../core.js";

export async function renderTimePanel() {
  const events = await invoke("list_clock_events", { date: todayIso() });
  const today = await invoke("attendance_today");
  const week = await invoke("attendance_for_week", { weekStart: weekStartIso() });
  setPanel(
    "Time Clock",
    `<button class="ghost" data-refresh>Refresh</button>`,
    `
      <div class="metric-row">
        <div class="metric"><span>Today</span><strong>${today.length}</strong></div>
        <div class="metric"><span>Week Start</span><strong>${weekStartIso()}</strong></div>
        <div class="metric"><span>Review</span><strong>${week.filter((row) => row.needs_admin_review).length}</strong></div>
        <div class="metric"><span>Events</span><strong>${events.length}</strong></div>
      </div>
      <h3>Weekly Hours</h3>
      ${table(
        ["Employee", "Hours", "Status", "Note"],
        week.map((row) => ({
          review: row.needs_admin_review,
          cells: [row.employee_name, row.hours, row.status, row.note],
        })),
      )}
      <h3>Today's Events</h3>
      ${table(
        ["Time", "Employee", "Action", "Source", "Note", ""],
        events.map((event) => ({
          review: event.needs_admin_review,
          cells: [
            event.timestamp.replace("T", " "),
            event.employee_name,
            formatAction(event.action),
            event.source,
            event.note,
            `<button class="ghost" data-edit-clock="${event.id}">Edit</button>`,
          ],
        })),
      )}
    `,
  );
  app.querySelector("[data-refresh]").addEventListener("click", renderTimePanel);

  // Edit clock event handlers
  app.querySelectorAll("[data-edit-clock]").forEach(btn => {
    btn.addEventListener("click", async () => {
      const eventId = Number(btn.dataset.editClock);
      const field = await promptModal({
        title: "Edit Clock Event",
        label: "Field to edit (timestamp, action, work_date, source, note)",
      }).catch(() => null);
      if (!field) return;
      const newVal = await promptModal({
        title: "Edit Clock Event",
        label: `New value for "${field}"`,
      }).catch(() => null);
      if (newVal === null) return;
      const reason =
        (await promptModal({
          title: "Edit Clock Event",
          label: "Reason for edit (audit trail)",
          confirmLabel: "Save",
        }).catch(() => "")) || "";
      try {
        await invoke("edit_clock_event", {
          input: {
            event_id: eventId,
            field_name: field,
            new_value: newVal,
            reason,
          },
          edited_by: state.admin.id,
        });
        renderTimePanel();
      } catch (e) {
        await alertModal({ title: "Edit Clock Event", message: `Error: ${e}` });
      }
    });
  });
}

export async function renderLogsPanel() {
  const allLogs = await invoke("list_cornice_logs", {
    employeeId: null,
    date: null,
    weekStart: null,
  });
  const weekLogs = allLogs.filter((log) => log.week_start === weekStartIso());
  const pendingOlder = allLogs.filter(
    (log) => log.needs_admin_review && log.week_start !== weekStartIso(),
  );
  const production = await invoke("list_production_logs", { employeeId: null, date: null });
  const logColumns = ["Date", "Employee", "Model", "Lengths", "Units", "Week Units", ""];
  const logRow = (log) => ({
    review: log.needs_admin_review,
    cells: [
      log.log_date,
      log.employee_name,
      corniceLogCellHtml(log, "model") ?? log.model,
      corniceLogCellHtml(log, "lengths") ?? String(log.lengths),
      log.total_units.toFixed(2),
      log.weekly_units.toFixed(2),
      log.needs_admin_review
        ? `<button class="ghost" data-approve-log="${log.id}">Approve</button>`
        : "",
    ],
  });
  setPanel(
    "Daily Logs",
    `<button class="ghost" data-refresh>Refresh</button>`,
    `
      <h3>Cornice Units This Week</h3>
      ${table(logColumns, weekLogs.map(logRow))}
      ${
        pendingOlder.length
          ? `<h3>Pending Approval (previous weeks)</h3>${table(logColumns, pendingOlder.map(logRow))}`
          : ""
      }
      <h3>Production Logs</h3>
      ${table(
        ["Date", "Employee", "Item", "Quantity", "Notes"],
        production.map((log) => ({
          cells: [log.log_date, log.employee_name, log.item, log.quantity, log.notes],
        })),
      )}
    `,
  );
  app.querySelector("[data-refresh]").addEventListener("click", renderLogsPanel);
  app.querySelectorAll("[data-approve-log]").forEach((button) => {
    button.addEventListener("click", async () => {
      await invoke("approve_cornice_log", { id: Number(button.dataset.approveLog) });
      renderLogsPanel();
    });
  });
}

export async function renderDatabasePanel() {
  const tables = await invoke("list_admin_tables");
  if (!tables.some((table) => table.name === state.adminDbTable)) {
    state.adminDbTable = tables[0]?.name || "employees";
  }
  const data = await invoke("list_admin_table_rows", { table: state.adminDbTable });
  const readOnly = !data.editable;
  const selected =
    state.selectedDbRow && state.selectedDbRow.table === data.table
      ? state.selectedDbRow
      : { table: data.table, rowid: null, values: emptyDbValues(data.columns) };
  const visibleColumns = data.columns.slice(0, 8);
  setPanel(
    `Database Tables${readOnly ? " (read-only)" : ""}`,
    `
      <select data-db-table>
        ${tables
          .map(
            (table) => `
              <option value="${escapeHtml(table.name)}" ${table.name === data.table ? "selected" : ""}>
                ${escapeHtml(table.label)}
              </option>
            `,
          )
          .join("")}
      </select>
      ${readOnly ? '' : `<button class="ghost" data-new-db-row>New Row</button>`}
      <button class="ghost" data-refresh>Refresh</button>
    `,
    (readOnly
      ? `<div class="message">This table is read-only. Use the dedicated panel to manage records.</div>
         ${table(
           visibleColumns.map((column) => column.label),
           data.rows.map((row) => ({
             review: row.values.needs_admin_review === true || row.values.resolved === false,
              cells: visibleColumns.map((column) => dbDisplay(column.name, row.values[column.name])),
            })),
          )}`
       : `
        <form class="form-grid db-form" data-db-form>
          ${data.columns.map((column) => dbField(column, selected.values[column.name])).join("")}
          <div class="wide panel-actions">
            <button class="primary" type="submit">Save Row</button>
            ${
              selected.rowid
                ? `<button class="danger" type="button" data-delete-db-row>Delete Row</button>`
                : ""
            }
          </div>
        </form>
        ${table(
          visibleColumns.map((column) => column.label),
          data.rows.map((row) => ({
            clickable: true,
            attrs: `data-db-row="${row.rowid}"`,
            review: row.values.needs_admin_review === true || row.values.resolved === false,
            cells: visibleColumns.map((column) => dbDisplay(column.name, row.values[column.name])),
          })),
        )}
     `
      ),
  );

  app.querySelector("[data-db-table]").addEventListener("change", (event) => {
    state.adminDbTable = event.currentTarget.value;
    state.selectedDbRow = null;
    renderDatabasePanel();
  });
  if (!readOnly) {
    app.querySelector("[data-new-db-row]").addEventListener("click", () => {
      state.selectedDbRow = { table: data.table, rowid: null, values: emptyDbValues(data.columns) };
      renderDatabasePanel();
    });
    app.querySelector("[data-db-form]").addEventListener("submit", async (event) => {
      event.preventDefault();
      const values = collectDbValues(data.columns, event.currentTarget);
      const result = await invoke("save_admin_table_row", {
        input: {
          table: data.table,
          rowid: selected.rowid,
          values,
        },
      });
      state.selectedDbRow = null;
      state.adminDbTable = result.table;
      renderDatabasePanel();
    });
    app.querySelector("[data-delete-db-row]")?.addEventListener("click", async () => {
      if (!selected.rowid) return;
      await invoke("delete_admin_table_row", {
        table: data.table,
        rowid: selected.rowid,
      });
      state.selectedDbRow = null;
      renderDatabasePanel();
    });
  }
  app.querySelector("[data-refresh]").addEventListener("click", () => {
    state.selectedDbRow = null;
    renderDatabasePanel();
  });
  if (!readOnly) {
    app.querySelectorAll("[data-db-row]").forEach((rowElement) => {
      rowElement.addEventListener("click", () => {
        const row = data.rows.find((item) => item.rowid === Number(rowElement.dataset.dbRow));
        state.selectedDbRow = row ? { table: data.table, rowid: row.rowid, values: row.values } : null;
        renderDatabasePanel();
      });
    });
  }
}
