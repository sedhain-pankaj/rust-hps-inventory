import {
  escapeHtml,
  formatAction,
  invoke,
  setBusy,
  todayIso,
  weekStartIso,
} from "./api.js";
import { chooseClockAction, requestAuth } from "./auth.js";

const app = document.getElementById("app");

const state = {
  status: null,
  logoDataUrl: "",
  staff: [],
  admin: null,
  currentStaff: null,
  adminView: "alerts",
  adminDbTable: "employees",
  selectedDbRow: null,
  enrollmentLog: [],
  staffView: "clock",
  selectedEmployee: null,
  selectedStock: null,
  selectedRate: null,
  selectedCorniceStock: null,
  selectedMould: null,
  corniceRateMatches: [],
};

const permissionLabels = {
  clock: "Clock",
  cornice_log: "Cornice log",
  production_log: "Production log",
  overstock: "Overstock",
  deliveries: "Deliveries",
  cornice_rates_view: "Cornice rates",
  daily_production_all: "All production",
};


async function loadStatus() {
  try {
    state.status = await invoke("app_status");
    state.logoDataUrl = await invoke("get_asset_data_url", { key: "hps_logo" });
  } catch (error) {
    state.status = {
      fingerprint_helper_found: false,
      fingerprint_helper_path: String(error),
      database_path: "",
    };
  }
}

function lockKioskKeys() {
  document.addEventListener("contextmenu", (event) => event.preventDefault());
  document.addEventListener("keydown", (event) => {
    const key = event.key.toLowerCase();
    const blocked =
      key === "escape" ||
      key === "f5" ||
      key === "f11" ||
      (event.ctrlKey && ["r", "w", "l", "p", "s", "+", "-", "="].includes(key)) ||
      (event.altKey && ["arrowleft", "arrowright", "f4", "tab"].includes(key)) ||
      (event.metaKey && ["q", "w", "m", "h"].includes(key));
    if (blocked) event.preventDefault();
  });
}

function renderHome() {
  app.innerHTML = `
    <section class="home">
      <div class="clock-face">
        <div class="clock-time" data-time>00:00:00</div>
        <div class="clock-date" data-date>00/00/0000 Monday</div>
      </div>
      <button class="primary start-button" data-start>Start</button>
    </section>
  `;
  app.querySelector("[data-start]").addEventListener("click", renderRoleMenu);
  tickClock();
}

let clockTimer = null;
function tickClock() {
  if (clockTimer) clearInterval(clockTimer);
  const timeNode = app.querySelector("[data-time]");
  const dateNode = app.querySelector("[data-date]");
  const update = () => {
    const now = new Date();
    timeNode.textContent = now.toLocaleTimeString("en-AU", {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    });
    const day = now.toLocaleDateString("en-AU", { weekday: "long" });
    const dd = String(now.getDate()).padStart(2, "0");
    const mm = String(now.getMonth() + 1).padStart(2, "0");
    dateNode.textContent = `${dd}/${mm}/${now.getFullYear()} ${day}`;
  };
  update();
  clockTimer = setInterval(update, 1000);
}

lockKioskKeys();
loadStatus();
renderHome();

function renderRoleMenu() {
  if (clockTimer) clearInterval(clockTimer);
  app.innerHTML = screenShell(
    "Choose Access",
    "Hopkins Plaster Studio",
    `
      <button class="role-tile" data-role="admin"><strong>Admin</strong><span>Full control</span></button>
      <button class="role-tile" data-role="staff"><strong>Staff</strong><span>Clocking and daily logs</span></button>
      <button class="role-tile brochure-disabled" disabled title="Coming soon"><strong>Brochure</strong><span>Coming soon</span></button>
    `,
    "role-grid",
  );
  app.querySelector("[data-back]").addEventListener("click", renderHome);
  app.querySelector('[data-role="admin"]').addEventListener("click", openAdmin);
  app.querySelector('[data-role="staff"]').addEventListener("click", renderStaffPicker);
}

async function openAdmin() {
  try {
    const response = await requestAuth({ title: "Admin", requireAdmin: true });
    state.admin = response.employee;
    state.adminView = "alerts";
    renderAdmin();
  } catch {
    renderRoleMenu();
  }
}

function renderCustomer() {
  app.innerHTML = screenShell(
    "Customer",
    "Hopkins Plaster Studio",
    `<div class="empty">Brochure is under construction.</div>`,
  );
  app.querySelector("[data-back]").addEventListener("click", renderRoleMenu);
}

async function renderStaffPicker() {
  state.staff = await invoke("list_staff", { includeInactive: false });
  app.innerHTML = screenShell(
    "Staff",
    "Choose your name",
    state.staff
      .map(
        (employee) => `
        <button class="staff-tile" data-employee="${escapeHtml(employee.id)}">
          <strong>${escapeHtml(employee.name)}</strong>
          <span>${escapeHtml(employee.id)}</span>
        </button>
      `,
      )
      .join(""),
    "staff-grid",
  );
  app.querySelector("[data-back]").addEventListener("click", renderRoleMenu);
  app.querySelectorAll("[data-employee]").forEach((button) => {
    button.addEventListener("click", async () => {
      const employee = state.staff.find((item) => item.id === button.dataset.employee);
      try {
        const response = await requestAuth({ title: "Staff", requireAdmin: false, employee });
        state.currentStaff = response.employee;
        state.staffView = "clock";
        renderStaffDashboard();
      } catch {
        // Auth failed or cancelled — stay on list
      }
    });
  });
}

function renderAdmin() {
  const tabs = [
    ["alerts", "Alerts"],
    ["employees", "Employees"],
    ["enroll", "Enroll"],
    ["payroll", "Payroll"],
    ["dispatch", "Dispatch"],
    ["cornice_stock", "Cornice Stock"],
    ["mould_inventory", "Moulds"],
    ["stock", "Stock"],
    ["rates", "Cornice Rates"],
    ["time", "Time Clock"],
    ["logs", "Daily Logs"],
    ["database", "Database"],
  ];
  app.innerHTML = workspaceShell(
    "Admin",
    state.admin?.name || "Admin",
    tabs,
    state.adminView,
  );
  app.querySelector("[data-back]").addEventListener("click", renderRoleMenu);
  app.querySelectorAll("[data-tab]").forEach((button) => {
    button.addEventListener("click", () => {
      state.adminView = button.dataset.tab;
      renderAdmin();
    });
  });
  renderAdminPanel();
}

async function renderAdminPanel() {
  if (state.adminView === "alerts") return renderAlertsPanel();
  if (state.adminView === "employees") return renderEmployeesPanel();
  if (state.adminView === "enroll") return renderEnrollPanel();
  if (state.adminView === "payroll") return renderPayrollPanel();
  if (state.adminView === "dispatch") return renderDispatchOrdersPanel();
  if (state.adminView === "cornice_stock") return renderCorniceStockPanel();
  if (state.adminView === "mould_inventory") return renderMouldInventoryPanel();
  if (state.adminView === "stock") return renderStockPanel();
  if (state.adminView === "rates") return renderRatesPanel();
  if (state.adminView === "time") return renderTimePanel();
  if (state.adminView === "logs") return renderLogsPanel();
  return renderDatabasePanel();
}

async function renderAlertsPanel() {
  setPanel("Admin Alerts", "", `<div class="message">Loading...</div>`);
  const alerts = await invoke("list_admin_alerts");
  setPanel(
    "Admin Alerts",
    `<button class="ghost" data-refresh>Refresh</button>`,
    alerts.length
      ? table(
          ["Severity", "Kind", "Message", "Created", ""],
          alerts.map((alert) => ({
            review: alert.severity === "red",
            cells: [
              alert.severity,
              alert.kind,
              alert.message,
              alert.created_at.replace("T", " "),
              `<button data-resolve="${alert.id}">Resolve</button>`,
            ],
          })),
        )
      : `<div class="empty">No alerts</div>`,
  );
  app.querySelector("[data-refresh]")?.addEventListener("click", renderAlertsPanel);
  app.querySelectorAll("[data-resolve]").forEach((button) => {
    button.addEventListener("click", async () => {
      await invoke("resolve_alert", { id: Number(button.dataset.resolve) });
      renderAlertsPanel();
    });
  });
}

async function renderEmployeesPanel() {
  const employees = await invoke("list_staff", { includeInactive: true });
  const selected = state.selectedEmployee || emptyEmployee();
  setPanel(
    "Employees",
    `<button class="ghost" data-new-employee>New</button>`,
    `
      <form class="form-grid" data-employee-form>
        <label>Employee ID<input name="id" value="${escapeHtml(selected.id)}" /></label>
        <label>Name<input name="name" value="${escapeHtml(selected.name)}" /></label>
        <label>Finger<input name="finger" value="${escapeHtml(selected.finger)}" /></label>
        <label>Password<input name="password" type="password" placeholder="Leave blank to keep current password" /></label>
        <label class="check"><input type="checkbox" name="active" ${selected.active ? "checked" : ""} /> Active</label>
        <label class="check"><input type="checkbox" name="is_admin" ${selected.is_admin ? "checked" : ""} /> Admin</label>
        <label>Staff Role
          <select name="staff_category">
            ${['cornice_hand','storekeeper','non_cornice','driver','helper'].map(c =>
              `<option value="${c}" ${(selected.staff_category || 'cornice_hand') === c ? 'selected' : ''}>${c.replace('_', ' ')}</option>`
            ).join('')}
          </select>
        </label>
        <div class="wide checkbox-row">
          ${Object.entries(permissionLabels)
            .map(
              ([key, label]) => `
                <label class="check">
                  <input type="checkbox" name="permission" value="${key}"
                    ${selected.permissions?.includes(key) ? "checked" : ""} />
                  ${escapeHtml(label)}
                </label>
              `,
            )
            .join("")}
        </div>
        <div class="wide panel-actions">
          <button class="primary" type="submit">Save Employee</button>
          <button class="warning" type="button" data-enroll>Enroll Fingerprint</button>
        </div>
      </form>
      ${table(
        ["Name", "ID", "Role", "Admin", "Password", "Fingerprint", "Active"],
        employees.map((employee) => ({
          clickable: true,
          attrs: `data-select-employee="${escapeHtml(employee.id)}"`,
          cells: [
            employee.name,
            employee.id,
            (employee.staff_category || 'cornice_hand').replace('_', ' '),
            employee.is_admin ? "Yes" : "No",
            employee.has_password ? "Set" : "No",
            employee.has_fingerprint ? "Enrolled" : "No",
            employee.active ? "Yes" : "No",
          ],
        })),
      )}
    `,
  );

  app.querySelector("[data-new-employee]").addEventListener("click", () => {
    state.selectedEmployee = emptyEmployee();
    renderEmployeesPanel();
  });
  app.querySelectorAll("[data-select-employee]").forEach((row) => {
    row.addEventListener("click", () => {
      state.selectedEmployee = employees.find((item) => item.id === row.dataset.selectEmployee);
      renderEmployeesPanel();
    });
  });
  app.querySelector("[data-employee-form]").addEventListener("submit", async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const permissions = form.getAll("permission");
    state.selectedEmployee = await invoke("save_employee", {
      input: {
        id: form.get("id"),
        name: form.get("name"),
        finger: form.get("finger") || "right-index",
        active: form.get("active") === "on",
        is_admin: form.get("is_admin") === "on",
        password: form.get("password"),
        permissions,
        staff_category: form.get("staff_category") || "cornice_hand",
      },
    });
    renderEmployeesPanel();
  });
  app.querySelector("[data-enroll]").addEventListener("click", async (event) => {
    setBusy(event.currentTarget);
    try {
      const form = new FormData(app.querySelector("[data-employee-form]"));
      const response = await invoke("enroll_fingerprint", {
        employeeId: form.get("id"),
        finger: form.get("finger") || "right-index",
      });
      state.selectedEmployee = response.employee;
      state.enrollmentLog = response.messages || [];
      renderEmployeesPanel();
    } catch (error) {
      alert(String(error));
    } finally {
      setBusy(event.currentTarget, false);
    }
  });
}

async function renderEnrollPanel() {
  const employees = await invoke("list_staff", { includeInactive: true });
  const selected = state.selectedEmployee || employees[0] || emptyEmployee();
  const log = state.enrollmentLog || [];
  setPanel(
    "Fingerprint Enrollment",
    `<button class="ghost" data-refresh>Refresh</button>`,
    `
      <form class="form-grid" data-enroll-form>
        <label class="wide">Employee
          <select name="employee_id">
            ${employees
              .map(
                (employee) => `
                  <option value="${escapeHtml(employee.id)}" ${employee.id === selected.id ? "selected" : ""}>
                    ${escapeHtml(employee.name)} (${escapeHtml(employee.id)})
                  </option>
                `,
              )
              .join("")}
          </select>
        </label>
        <label>Finger
          <select name="finger">
            ${fingerOptions(selected.finger)}
          </select>
        </label>
        <div class="panel-actions">
          <button class="warning" type="submit">Enroll / Replace Fingerprint</button>
        </div>
      </form>
      <div class="message">The template is saved to SQLite as a BLOB. Temporary helper files are cleared after enrollment and scans.</div>
      <div class="log-box" data-enrollment-log>
        ${
          log.length
            ? log.map((line) => `<div>${escapeHtml(formatFingerprintLine(line))}</div>`).join("")
            : `<div>Ready to enroll.</div>`
        }
      </div>
      ${table(
        ["Name", "ID", "Admin", "Password", "Fingerprint"],
        employees.map((employee) => ({
          review: !employee.has_password || !employee.has_fingerprint,
          cells: [
            employee.name,
            employee.id,
            employee.is_admin ? "Yes" : "No",
            employee.has_password ? "Set" : "No",
            employee.has_fingerprint ? "Enrolled" : "No",
          ],
        })),
      )}
    `,
  );
  app.querySelector("[data-refresh]").addEventListener("click", renderEnrollPanel);
  app.querySelector("[data-enroll-form]").addEventListener("submit", async (event) => {
    event.preventDefault();
    const button = event.currentTarget.querySelector("button[type='submit']");
    const form = new FormData(event.currentTarget);
    setBusy(button);
    state.enrollmentLog = ["Starting enrollment. Follow the reader prompts."];
    renderEnrollmentLog();
    try {
      const start = await invoke("start_fingerprint_enroll", {
        employeeId: form.get("employee_id"),
        finger: form.get("finger") || "right-index",
      });
      let nextIndex = 0;
      while (true) {
        await wait(250);
        const status = await invoke("poll_fingerprint_enroll", {
          jobId: start.job_id,
          fromIndex: nextIndex,
        });
        nextIndex = status.next_index ?? nextIndex;
        if (Array.isArray(status.lines) && status.lines.length) {
          state.enrollmentLog.push(...status.lines);
          renderEnrollmentLog();
        }
        if (status.state === "done") {
          state.selectedEmployee = status.employee || state.selectedEmployee;
          break;
        }
        if (status.state === "failed") {
          throw new Error(status.error || "Enrollment failed.");
        }
      }
      renderEnrollPanel();
    } catch (error) {
      state.enrollmentLog = [String(error.message || error)];
      renderEnrollmentLog();
      renderEnrollPanel();
    } finally {
      setBusy(button, false);
    }
  });
}

async function renderStockPanel() {
  const items = await invoke("list_stock_items");
  const selected = state.selectedStock || {
    id: null,
    item_type: "cornice",
    model: "",
    stock: 0,
    location: "",
    dimensions: "",
    photo_path: "",
    notes: "",
  };
  setPanel(
    "Stock",
    `<button class="ghost" data-new-stock>New</button>`,
    `
      <form class="form-grid" data-stock-form>
        <input type="hidden" name="id" value="${escapeHtml(selected.id || "")}" />
        <label>Type<input name="item_type" value="${escapeHtml(selected.item_type)}" /></label>
        <label>Model<input name="model" value="${escapeHtml(selected.model)}" /></label>
        <label>Stock<input name="stock" type="number" value="${escapeHtml(selected.stock)}" /></label>
        <label>Location<input name="location" value="${escapeHtml(selected.location)}" /></label>
        <label>Dimensions<input name="dimensions" value="${escapeHtml(selected.dimensions)}" /></label>
        <label>Photo Path<input name="photo_path" value="${escapeHtml(selected.photo_path)}" /></label>
        <label class="wide">Notes<textarea name="notes">${escapeHtml(selected.notes)}</textarea></label>
        <div class="wide panel-actions"><button class="primary" type="submit">Save Stock</button></div>
      </form>
      ${table(
        ["Type", "Model", "Stock", "Location", "Dimensions"],
        items.map((item) => ({
          clickable: true,
          attrs: `data-stock="${item.id}"`,
          cells: [item.item_type, item.model, item.stock, item.location, item.dimensions],
        })),
      )}
    `,
  );
  app.querySelector("[data-new-stock]").addEventListener("click", () => {
    state.selectedStock = null;
    renderStockPanel();
  });
  app.querySelectorAll("[data-stock]").forEach((row) => {
    row.addEventListener("click", () => {
      state.selectedStock = items.find((item) => item.id === Number(row.dataset.stock));
      renderStockPanel();
    });
  });
  app.querySelector("[data-stock-form]").addEventListener("submit", async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    state.selectedStock = await invoke("save_stock_item", {
      input: {
        id: form.get("id") ? Number(form.get("id")) : null,
        item_type: form.get("item_type"),
        model: form.get("model"),
        stock: Number(form.get("stock") || 0),
        location: form.get("location"),
        dimensions: form.get("dimensions"),
        photo_path: form.get("photo_path"),
        notes: form.get("notes"),
      },
    });
    renderStockPanel();
  });
}

async function renderRatesPanel() {
  const rates = await invoke("list_cornice_rates");
  const selected = state.selectedRate || {
    id: null,
    series: "",
    model: "",
    unit_text: "",
    unit_value: "",
    is_confidential: true,
  };
  setPanel(
    "Cornice Rates",
    `<button class="ghost" data-new-rate>New</button>`,
    `
      <form class="form-grid" data-rate-form>
        <input type="hidden" name="id" value="${escapeHtml(selected.id || "")}" />
        <label>Series<input name="series" value="${escapeHtml(selected.series)}" /></label>
        <label>Model<input name="model" value="${escapeHtml(selected.model)}" /></label>
        <label>Unit Text<input name="unit_text" value="${escapeHtml(selected.unit_text)}" /></label>
        <label>Unit Value<input name="unit_value" type="number" step="0.01" value="${escapeHtml(selected.unit_value ?? "")}" /></label>
        <label class="check"><input type="checkbox" name="is_confidential" ${selected.is_confidential ? "checked" : ""} /> Confidential</label>
        <div class="wide panel-actions"><button class="primary" type="submit">Save Rate</button></div>
      </form>
      ${table(
        ["Series", "Model", "Unit", "Value", "Confidential"],
        rates.map((rate) => ({
          clickable: true,
          attrs: `data-rate="${rate.id}"`,
          cells: [
            rate.series,
            rate.model,
            rate.unit_text,
            rate.unit_value ?? "",
            rate.is_confidential ? "Yes" : "No",
          ],
        })),
      )}
    `,
  );
  app.querySelector("[data-new-rate]").addEventListener("click", () => {
    state.selectedRate = null;
    renderRatesPanel();
  });
  app.querySelectorAll("[data-rate]").forEach((row) => {
    row.addEventListener("click", () => {
      state.selectedRate = rates.find((rate) => rate.id === Number(row.dataset.rate));
      renderRatesPanel();
    });
  });
  app.querySelector("[data-rate-form]").addEventListener("submit", async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    state.selectedRate = await invoke("save_cornice_rate", {
      input: {
        id: form.get("id") ? Number(form.get("id")) : null,
        series: form.get("series"),
        model: form.get("model"),
        unit_text: form.get("unit_text"),
        unit_value: form.get("unit_value") ? Number(form.get("unit_value")) : null,
        is_confidential: form.get("is_confidential") === "on",
      },
    });
    renderRatesPanel();
  });
}

async function renderTimePanel() {
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
      const field = prompt("Field to edit (timestamp, action, work_date, source, note):");
      if (!field) return;
      const newVal = prompt(`New value for "${field}":`);
      if (newVal === null) return;
      const reason = prompt("Reason for edit (audit trail):") || "";
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
        alert(`Error: ${e}`);
      }
    });
  });
}

async function renderLogsPanel() {
  const cornice = await invoke("list_cornice_logs", {
    employeeId: null,
    date: null,
    weekStart: weekStartIso(),
  });
  const production = await invoke("list_production_logs", { employeeId: null, date: null });
  setPanel(
    "Daily Logs",
    `<button class="ghost" data-refresh>Refresh</button>`,
    `
      <h3>Cornice Units This Week</h3>
      ${table(
        ["Date", "Employee", "Model", "Lengths", "Units", "Week Units"],
        cornice.map((log) => ({
          review: log.needs_admin_review,
          cells: [
            log.log_date,
            log.employee_name,
            log.model,
            log.lengths,
            log.total_units.toFixed(2),
            log.weekly_units.toFixed(2),
          ],
        })),
      )}
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
}

async function renderDatabasePanel() {
  const tables = await invoke("list_admin_tables");
  if (!tables.some((table) => table.name === state.adminDbTable)) {
    state.adminDbTable = tables[0]?.name || "employees";
  }
  const data = await invoke("list_admin_table_rows", { table: state.adminDbTable });
  const selected =
    state.selectedDbRow && state.selectedDbRow.table === data.table
      ? state.selectedDbRow
      : { table: data.table, rowid: null, values: emptyDbValues(data.columns) };
  const visibleColumns = data.columns.slice(0, 8);
  setPanel(
    "Database Tables",
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
      <button class="ghost" data-new-db-row>New Row</button>
      <button class="ghost" data-refresh>Refresh</button>
    `,
    `
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
          cells: visibleColumns.map((column) => dbDisplay(row.values[column.name])),
        })),
      )}
    `,
  );

  app.querySelector("[data-db-table]").addEventListener("change", (event) => {
    state.adminDbTable = event.currentTarget.value;
    state.selectedDbRow = null;
    renderDatabasePanel();
  });
  app.querySelector("[data-new-db-row]").addEventListener("click", () => {
    state.selectedDbRow = { table: data.table, rowid: null, values: emptyDbValues(data.columns) };
    renderDatabasePanel();
  });
  app.querySelector("[data-refresh]").addEventListener("click", () => {
    state.selectedDbRow = null;
    renderDatabasePanel();
  });
  app.querySelectorAll("[data-db-row]").forEach((rowElement) => {
    rowElement.addEventListener("click", () => {
      const row = data.rows.find((item) => item.rowid === Number(rowElement.dataset.dbRow));
      state.selectedDbRow = row ? { table: data.table, rowid: row.rowid, values: row.values } : null;
      renderDatabasePanel();
    });
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

// ==================== Admin: Payroll Panel ====================

async function renderPayrollPanel() {
  setPanel("Weekly Payroll", `<button class="ghost" data-refresh>Refresh</button>`, `<div class="message">Loading payroll…</div>`);
  try {
    const weekStart = getWeekStartForDate(new Date().toISOString().slice(0, 10));
    const payrollData = await invoke("get_all_payroll_week", { request: { week_start: weekStart } });
    const unresolved = payrollData.filter(p => p.status === "unresolved");

    let body = `
      <div class="metric-row">
        <div class="metric"><span>Week</span><strong>${weekStart}</strong></div>
        <div class="metric"><span>Employees</span><strong>${payrollData.length}</strong></div>
        <div class="metric"><span>Unresolved</span><strong style="color:${unresolved.length ? '#e55' : 'inherit'}">${unresolved.length}</strong></div>
      </div>
    `;

    if (payrollData.length === 0) {
      body += `<div class="empty">No employees to calculate payroll for.</div>`;
    } else {
      body += table(
        ["Employee", "Hours", "Known Units", "Threshold", "Base Pay", "Extra Pay", "Gross", "Status", ""],
        payrollData.map(p => ({
          review: p.status === "unresolved" || p.needs_admin_review,
          cells: [
            `${escapeHtml(p.employee_name)} (${escapeHtml(p.employee_id)})`,
            `${p.total_hours.toFixed(1)}h`,
            p.total_units_known.toFixed(1),
            `${p.unit_threshold.toFixed(0)}`,
            `$${p.base_pay.toFixed(2)}`,
            `$${p.extra_unit_pay.toFixed(2)}`,
            p.gross_pay !== null && p.gross_pay !== undefined ? `$${p.gross_pay.toFixed(2)}` : `<em>unresolved</em>`,
            `<span style="color:${p.status === 'final' ? '#5a5' : p.status === 'unresolved' ? '#e55' : '#da5'}">${escapeHtml(p.status)}</span>`,
            p.status === "review"
              ? `<button data-proration-accept emp="${escapeHtml(p.employee_id)}" week="${escapeHtml(p.week_start)}">Accept Prorated</button> <button data-proration-override emp="${escapeHtml(p.employee_id)}" week="${escapeHtml(p.week_start)}">Use Standard 180</button>`
              : "",
          ],
        }))
      );

      if (unresolved.length > 0) {
        body += `<h3>Unresolved Rates</h3>`;
        body += table(
          ["Employee", "Unknown Model", "Qty", "Action"],
          unresolved.flatMap(p =>
            p.unknown_rate_details.map(d => ({
              cells: [
                `${escapeHtml(p.employee_name)} (${escapeHtml(p.employee_id)})`,
                escapeHtml(d.model),
                d.quantity,
                `<button data-resolve-rate model="${escapeHtml(d.model)}">Set Rate</button>`,
              ],
            }))
          )
        );
        app.querySelectorAll("[data-resolve-rate]").forEach(btn => {
          btn.addEventListener("click", async () => {
            const model = btn.dataset.model;
            const uv = prompt(`Enter unit value for cornice "${model}" (lengths-to-units ratio):`);
            if (uv && !isNaN(uv) && parseFloat(uv) > 0) {
              try {
                await invoke("resolve_unknown_rate", {
                  input: {
                    model,
                    unit_value: parseFloat(uv),
                    series: null,
                  },
                });
                renderPayrollPanel();
              } catch (e) {
                alert(`Error: ${e}`);
              }
            }
          });
        });

        app.querySelectorAll("[data-proration-accept]").forEach(btn => {
          btn.addEventListener("click", async () => {
            if (!confirm("Accept the prorated unit threshold for this employee?")) return;
            await invoke("override_payroll_proration", {
              input: {
                employee_id: btn.dataset.emp,
                week_start: btn.dataset.week,
                accept_prorated: true,
              },
            });
            renderPayrollPanel();
          });
        });

        app.querySelectorAll("[data-proration-override]").forEach(btn => {
          btn.addEventListener("click", async () => {
            if (!confirm("Override to standard 40-hr / 180-unit week for this employee?")) return;
            await invoke("override_payroll_proration", {
              input: {
                employee_id: btn.dataset.emp,
                week_start: btn.dataset.week,
                accept_prorated: false,
              },
            });
            renderPayrollPanel();
          });
        });
      }
    }

    setPanel("Weekly Payroll", `<button class="ghost" data-refresh>Refresh</button>`, body);
    app.querySelector("[data-refresh]")?.addEventListener("click", renderPayrollPanel);
  } catch (error) {
    setPanel("Weekly Payroll", "", `<div class="message">Error loading payroll: ${escapeHtml(String(error))}</div>`);
  }
}

// ==================== Admin: Cornice Stock Panel ====================

async function renderCorniceStockPanel() {
  const items = await invoke("list_cornice_stock");
  const selected = state.selectedCorniceStock || { id: null, model: "", aisle: "", quantity_in_stock: 0, quantity_reserved: 0, remarks: "" };
  setPanel(
    "Cornice Stock",
    `<button class="ghost" data-new-cornice-stock>New</button>`,
    `
      <form class="form-grid" data-cornice-stock-form>
        <input type="hidden" name="id" value="${selected.id || ''}" />
        <label>Model<input name="model" value="${escapeHtml(selected.model)}" /></label>
        <label>Aisle<input name="aisle" value="${escapeHtml(selected.aisle)}" /></label>
        <label>In Stock<input name="quantity_in_stock" type="number" value="${selected.quantity_in_stock}" /></label>
        <label>Reserved<input name="quantity_reserved" type="number" value="${selected.quantity_reserved}" /></label>
        <label class="wide">Remarks<textarea name="remarks">${escapeHtml(selected.remarks)}</textarea></label>
        <div class="wide panel-actions"><button class="primary" type="submit">Save Stock</button></div>
      </form>
      ${table(
        ["Model", "Aisle", "In Stock", "Reserved", "Remarks"],
        items.map(item => ({
          clickable: true,
          attrs: `data-cornice-stock="${item.id}"`,
          cells: [item.model, item.aisle, item.quantity_in_stock, item.quantity_reserved, item.remarks],
        }))
      )}
    `
  );
  app.querySelector("[data-new-cornice-stock]").addEventListener("click", () => {
    state.selectedCorniceStock = null;
    renderCorniceStockPanel();
  });
  app.querySelectorAll("[data-cornice-stock]").forEach(row => {
    row.addEventListener("click", () => {
      state.selectedCorniceStock = items.find(i => i.id === Number(row.dataset.corniceStock));
      renderCorniceStockPanel();
    });
  });
  app.querySelector("[data-cornice-stock-form]").addEventListener("submit", async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    state.selectedCorniceStock = await invoke("save_cornice_stock", {
      input: {
        id: form.get("id") ? Number(form.get("id")) : null,
        model: form.get("model"),
        aisle: form.get("aisle"),
        quantity_in_stock: Number(form.get("quantity_in_stock") || 0),
        quantity_reserved: Number(form.get("quantity_reserved") || 0),
        remarks: form.get("remarks"),
      },
    });
    renderCorniceStockPanel();
  });
}

// ==================== Admin: Mould Inventory Panel ====================

async function renderMouldInventoryPanel() {
  const items = await invoke("list_mould_inventory");
  const selected = state.selectedMould || { id: null, mould_name: "", storage_location: "", notes: "" };
  setPanel(
    "Mould Inventory",
    `<button class="ghost" data-new-mould>New</button>`,
    `
      <form class="form-grid" data-mould-form>
        <input type="hidden" name="id" value="${selected.id || ''}" />
        <label>Mould Name<input name="mould_name" value="${escapeHtml(selected.mould_name)}" /></label>
        <label>Storage Location<input name="storage_location" value="${escapeHtml(selected.storage_location)}" /></label>
        <label class="wide">Notes<textarea name="notes">${escapeHtml(selected.notes)}</textarea></label>
        <div class="wide panel-actions"><button class="primary" type="submit">Save Mould</button></div>
      </form>
      ${table(
        ["Mould Name", "Location", "Notes"],
        items.map(item => ({
          clickable: true,
          attrs: `data-mould="${item.id}"`,
          cells: [item.mould_name, item.storage_location, item.notes],
        }))
      )}
    `
  );
  app.querySelector("[data-new-mould]").addEventListener("click", () => {
    state.selectedMould = null;
    renderMouldInventoryPanel();
  });
  app.querySelectorAll("[data-mould]").forEach(row => {
    row.addEventListener("click", () => {
      state.selectedMould = items.find(i => i.id === Number(row.dataset.mould));
      renderMouldInventoryPanel();
    });
  });
  app.querySelector("[data-mould-form]").addEventListener("submit", async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    state.selectedMould = await invoke("save_mould_inventory", {
      input: {
        id: form.get("id") ? Number(form.get("id")) : null,
        mould_name: form.get("mould_name"),
        storage_location: form.get("storage_location"),
        notes: form.get("notes"),
      },
    });
    renderMouldInventoryPanel();
  });
}

// ==================== Admin: Dispatch Orders Panel ====================

async function renderDispatchOrdersPanel() {
  const orders = await invoke("list_dispatch_orders", { status: null });
  setPanel(
    "Dispatch Orders",
    `<button class="ghost" data-new-dispatch>New Order</button>`,
    `
      <form class="form-grid" data-dispatch-form style="display:none">
        <label>Cornice Model<input name="cornice_model" required /></label>
        <label>Quantity<input name="quantity" type="number" min="1" required /></label>
        <label>Delivery Location<input name="delivery_location" required /></label>
        <label class="wide">Remarks<textarea name="remarks"></textarea></label>
        <div class="wide panel-actions">
          <button class="primary" type="submit">Create Order</button>
          <button class="ghost" type="button" data-cancel-dispatch>Cancel</button>
        </div>
      </form>
      ${table(
        ["Model", "Qty", "Location", "Status", "Created", "Delivered By", ""],
        orders.map(o => ({
          review: o.status === "pending",
          cells: [
            o.cornice_model,
            o.quantity,
            o.delivery_location,
            `<span style="color:${o.status === 'delivered' ? '#5a5' : o.status === 'pending' ? '#e55' : '#da5'}">${escapeHtml(o.status)}</span>`,
            o.created_at.replace("T", " "),
            o.delivered_by_name || "—",
            o.status === "pending" ? `<button data-mark-progress="${o.id}">Start</button>` : "",
          ],
        }))
      )}
    `
  );

  const form = app.querySelector("[data-dispatch-form]");
  app.querySelector("[data-new-dispatch]").addEventListener("click", () => {
    form.style.display = "";
  });
  app.querySelector("[data-cancel-dispatch]")?.addEventListener("click", () => {
    form.style.display = "none";
  });
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const fd = new FormData(event.currentTarget);
    await invoke("create_dispatch_order", {
      input: {
        id: null,
        cornice_model: fd.get("cornice_model"),
        quantity: Number(fd.get("quantity")),
        delivery_location: fd.get("delivery_location"),
        status: null,
        remarks: fd.get("remarks"),
      },
      createdBy: state.admin.id,
    });
    renderDispatchOrdersPanel();
  });

  app.querySelectorAll("[data-mark-progress]").forEach(btn => {
    btn.addEventListener("click", async () => {
      await invoke("update_dispatch_order", {
        input: {
          id: Number(btn.dataset.markProgress),
          cornice_model: "",
          quantity: 0,
          delivery_location: "",
          status: "in_progress",
          remarks: "",
        },
        updatedBy: state.admin.id,
      });
      renderDispatchOrdersPanel();
    });
  });
}

function getWeekStartForDate(dateStr) {
  const d = new Date(dateStr);
  const day = d.getDay();
  const diff = (day + 5) % 7;
  d.setDate(d.getDate() - diff);
  return d.toISOString().slice(0, 10);
}

function renderStaffDashboard() {
  const employee = state.currentStaff;
  const category = employee.staff_category || "cornice_hand";
  const tabs = [["clock", "Clock"]];
  if (category === "cornice_hand" && employee.permissions.includes("cornice_log")) {
    tabs.push(["cornice", "Cornice"]);
    tabs.push(["payroll", "My Payroll"]);
  }
  if (category === "storekeeper") {
    if (employee.permissions.includes("cornice_log")) tabs.push(["cornice", "Cornice Logs"]);
    tabs.push(["moulds", "Moulds"]);
    tabs.push(["cornice_stock", "Stock"]);
    if (employee.permissions.includes("production_log")) tabs.push(["production", "Production"]);
    if (employee.permissions.includes("deliveries")) tabs.push(["deliveries", "Deliveries"]);
  }
  if (category === "non_cornice" && employee.permissions.includes("production_log")) {
    tabs.push(["production", "Production"]);
    tabs.push(["payroll", "My Payroll"]);
  }
  if (category === "driver") {
    tabs.push(["dispatch", "Dispatch Orders"]);
    if (employee.permissions.includes("deliveries")) tabs.push(["deliveries", "Deliveries"]);
    tabs.push(["moulds", "Moulds"]);
  }
  if (category === "helper") {
    tabs.push(["moulds", "Moulds"]);
    tabs.push(["cornice_stock_ro", "Stock"]);
  }
  // Legacy permissions fallback
  if (employee.permissions.includes("overstock")) tabs.push(["overstock", "Overstock"]);
  if (employee.permissions.includes("cornice_rates_view")) tabs.push(["rates", "Rates"]);

  app.innerHTML = workspaceShell("Staff", employee.name, tabs, state.staffView);
  app.querySelector("[data-back]").addEventListener("click", renderStaffPicker);
  app.querySelectorAll("[data-tab]").forEach((button) => {
    button.addEventListener("click", () => {
      state.staffView = button.dataset.tab;
      renderStaffDashboard();
    });
  });
  renderStaffPanel();
}

async function renderStaffPanel() {
  const view = state.staffView;
  if (view === "clock") return renderStaffClock();
  if (view === "cornice") return renderStaffCornice();
  if (view === "production") return renderStaffProduction();
  if (view === "overstock") return renderStaffOverstock();
  if (view === "deliveries") return renderStaffDeliveries();
  if (view === "dispatch") return renderDriverDispatchView();
  if (view === "moulds") return renderStaffMouldView();
  if (view === "cornice_stock") return renderCorniceStockPanel();
  if (view === "cornice_stock_ro") return renderStaffCorniceStockRO();
  if (view === "payroll") return renderStaffPayroll();
  return renderStaffRates();
}

async function renderStaffClock(message = "") {
  const events = await invoke("list_clock_events", { date: todayIso() });
  setPanel(
    "Clock",
    `<button class="primary" data-clock>Clock In / Out</button>`,
    `
      <div class="message">${escapeHtml(message)}</div>
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
    setBusy(event.currentTarget);
    try {
      const auth = await requestAuth({
        title: "Clock",
        employee: state.currentStaff,
        requireAdmin: false,
      });
      const action = await chooseClockAction(auth.employee);
      const result = await invoke("record_clock_event", {
        request: {
          employee_id: auth.employee.id,
          action,
          source: auth.source,
        },
      });
      renderStaffClock(`${formatAction(result.action)} recorded at ${result.timestamp.slice(11)}`);
    } catch (error) {
      renderStaffClock(String(error.message || error));
    } finally {
      setBusy(event.currentTarget, false);
    }
  });
}

async function renderStaffCornice() {
  const logs = await invoke("list_cornice_logs", {
    employeeId: state.currentStaff.id,
    date: null,
    weekStart: weekStartIso(),
  });
  setPanel(
    "Cornice Log",
    "",
    `
      <form class="form-grid" data-cornice-form>
        <label>Series<input name="series" placeholder="Auto-filled on match" /></label>
        <label>Model
          <input name="model" id="cornice-model-input" required list="cornice-models-datalist" placeholder="Start typing cornice model…" autocomplete="off" />
          <datalist id="cornice-models-datalist"></datalist>
          <div id="cornice-search-results" class="message" style="display:none;margin-top:0.5em;font-size:0.9em;"></div>
        </label>
        <label>Lengths<input name="lengths" type="number" min="1" required /></label>
        <div class="panel-actions"><button class="primary" type="submit">Add Log</button></div>
      </form>
      ${table(
        ["Date", "Model", "Lengths", "Unit", "Units", "Week Units"],
        logs.map((log) => ({
          review: log.needs_admin_review,
          cells: [
            log.log_date,
            log.model,
            log.lengths,
            log.unit_text || "Custom",
            log.total_units.toFixed(2),
            log.weekly_units.toFixed(2),
          ],
        })),
      )}
    `,
  );

  // Fuzzy search autocomplete for cornice model
  const modelInput = app.querySelector("#cornice-model-input");
  const datalist = app.querySelector("#cornice-models-datalist");
  const resultsBox = app.querySelector("#cornice-search-results");
  let searchTimeout = null;

  modelInput.addEventListener("input", async () => {
    const query = modelInput.value.trim();
    clearTimeout(searchTimeout);
    if (query.length < 2) {
      datalist.innerHTML = "";
      resultsBox.style.display = "none";
      return;
    }
    searchTimeout = setTimeout(async () => {
      try {
        const resp = await invoke("search_cornice_rates", { request: { query } });
        const matches = resp.matches || [];
        state.corniceRateMatches = matches;
        datalist.innerHTML = matches.slice(0, 15).map(m =>
          `<option value="${escapeHtml(m.model)}">${escapeHtml(m.series ? m.series + ' ' : '')}${escapeHtml(m.model)} (${m.unit_text})</option>`
        ).join("");
        if (matches.length > 0) {
          resultsBox.textContent = `${matches.length} match(es). Select or keep typing.`;
          resultsBox.style.display = "block";
        } else {
          resultsBox.textContent = `No match found — will be logged as unknown/custom.`;
          resultsBox.style.display = "block";
        }
      } catch { /* ignore */ }
    }, 200);
  });

  modelInput.addEventListener("focus", async () => {
    const query = modelInput.value.trim();
    if (query.length >= 2) return;
    // Show all rates on focus if no input
    try {
      const resp = await invoke("search_cornice_rates", { request: { query: "" } });
      datalist.innerHTML = "";
    } catch { /* ignore */ }
  });

  app.querySelector("[data-cornice-form]").addEventListener("submit", async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const modelVal = form.get("model");
    // Auto-fill series from match
    let series = form.get("series");
    if (!series) {
      const match = state.corniceRateMatches.find(m => m.model.toLowerCase() === modelVal.toLowerCase());
      if (match) series = match.series;
    }
    await invoke("add_cornice_log", {
      input: {
        employee_id: state.currentStaff.id,
        log_date: todayIso(),
        series: series || "",
        model: modelVal,
        lengths: Number(form.get("lengths")),
      },
    });
    renderStaffCornice();
  });
}

async function renderStaffProduction() {
  const logs = await invoke("list_production_logs", {
    employeeId: state.currentStaff.id,
    date: null,
  });
  setPanel(
    "Production Log",
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
  });
}

async function renderStaffOverstock() {
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
  });
}

async function renderStaffDeliveries() {
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
  });
}

async function renderDriverDispatchView() {
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
          `<span style="color:${o.status === 'delivered' ? '#5a5' : o.status === 'pending' ? '#e55' : '#da5'}">${escapeHtml(o.status)}</span>`,
          o.created_at.replace("T", " "),
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
      const remarks = prompt("Delivery remarks (optional):") || "";
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
    });
  });
}

async function renderStaffRates() {
  const rates = await invoke("list_cornice_rates");
  setPanel(
    "Cornice Rates",
    "",
    table(
      ["Series", "Model", "Unit"],
      rates.map((rate) => ({
        cells: [rate.series, rate.model, rate.unit_text],
      })),
    ),
  );
}

async function renderStaffMouldView() {
  const items = await invoke("list_mould_inventory");
  setPanel(
    "Mould Inventory (Read-Only)",
    `<button class="ghost" data-refresh>Refresh</button>`,
    items.length > 0
      ? table(
          ["Mould Name", "Location", "Notes"],
          items.map(item => ({
            cells: [item.mould_name, item.storage_location, item.notes],
          }))
        )
      : `<div class="empty">No moulds registered.</div>`
  );
  app.querySelector("[data-refresh]")?.addEventListener("click", renderStaffMouldView);
}

async function renderStaffCorniceStockRO() {
  const items = await invoke("list_cornice_stock");
  setPanel(
    "Cornice Stock (Read-Only)",
    `<button class="ghost" data-refresh>Refresh</button>`,
    items.length > 0
      ? table(
          ["Model", "Aisle", "In Stock", "Reserved"],
          items.map(item => ({
            cells: [item.model, item.aisle, item.quantity_in_stock, item.quantity_reserved],
          }))
        )
      : `<div class="empty">No cornice stock registered.</div>`
  );
  app.querySelector("[data-refresh]")?.addEventListener("click", renderStaffCorniceStockRO);
}

async function renderStaffPayroll() {
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
        <div class="metric"><span>Status</span><strong style="color:${payroll.status === 'final' ? '#5a5' : payroll.status === 'unresolved' ? '#e55' : '#da5'}">${escapeHtml(payroll.status)}</strong></div>
      </div>
    `;

    body += `<h3>Pay Breakdown</h3>`;
    body += `<table class="table"><tbody>`;
    body += `<tr><td>Base Pay</td><td><strong>$${payroll.base_pay.toFixed(2)}</strong></td></tr>`;
    body += `<tr><td>Known Units</td><td>${payroll.total_units_known.toFixed(1)}</td></tr>`;
    body += `<tr><td>Unit Threshold</td><td>${payroll.unit_threshold.toFixed(0)} <small>(${escapeHtml(payroll.threshold_note)})</small></td></tr>`;
    if (payroll.extra_unit_pay > 0) {
      body += `<tr><td>Extra Unit Pay (${payroll.total_units_known - payroll.unit_threshold} extra × $3.80)</td><td><strong>$${payroll.extra_unit_pay.toFixed(2)}</strong></td></tr>`;
    }
    if (payroll.gross_pay !== null && payroll.gross_pay !== undefined) {
      body += `<tr style="font-size:1.2em"><td><strong>Gross Pay</strong></td><td><strong>$${payroll.gross_pay.toFixed(2)}</strong></td></tr>`;
    }
    body += `</tbody></table>`;

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

function screenShell(title, subtitle, content, contentClass = "") {
  return `
    <section class="screen">
      ${topbar(title, subtitle)}
      <div class="${contentClass}">${content}</div>
    </section>
  `;
}

function workspaceShell(title, subtitle, tabs, active) {
  return `
    <section class="screen">
      ${topbar(title, subtitle)}
      <div class="workspace">
        <nav class="side-nav">
          ${tabs
            .map(
              ([id, label]) => `
                <button data-tab="${id}" class="${active === id ? "active" : ""}">
                  ${escapeHtml(label)}
                </button>
              `,
            )
            .join("")}
        </nav>
        <section class="panel">
          <div class="panel-header"><h2 data-panel-title></h2><div class="panel-actions" data-panel-actions></div></div>
          <div class="panel-body" data-panel-body></div>
        </section>
      </div>
    </section>
  `;
}

function topbar(title, subtitle) {
  const helper = state.status?.fingerprint_helper_found ? "Fingerprint ready" : "Fingerprint helper missing";
  const logo = state.logoDataUrl || "./assets/HPS.png";
  return `
    <header class="topbar">
      <button class="icon ghost" data-back title="Back">Back</button>
      <div class="brand">
        <img src="${logo}" alt="" />
        <div class="title">
          <h1>${escapeHtml(title)}</h1>
          <p>${escapeHtml(subtitle)}</p>
        </div>
      </div>
      <div class="status-pill" title="${escapeHtml(state.status?.database_path || "")}">
        ${escapeHtml(helper)}
      </div>
    </header>
  `;
}

function setPanel(title, actions, body) {
  app.querySelector("[data-panel-title]").textContent = title;
  app.querySelector("[data-panel-actions]").innerHTML = actions;
  app.querySelector("[data-panel-body]").innerHTML = body;
}

function table(headers, rows) {
  if (!rows.length) return `<div class="message">No records</div>`;
  return `
    <table class="table">
      <thead><tr>${headers.map((header) => `<th>${escapeHtml(header)}</th>`).join("")}</tr></thead>
      <tbody>
        ${rows
          .map(
            (row) => `
              <tr class="${row.review ? "review" : ""} ${row.clickable ? "clickable" : ""}" ${row.attrs || ""}>
                ${row.cells.map((cell) => `<td>${cellLooksHtml(cell) ? cell : escapeHtml(cell)}</td>`).join("")}
              </tr>
            `,
          )
          .join("")}
      </tbody>
    </table>
  `;
}

function cellLooksHtml(value) {
  return typeof value === "string" && value.trim().startsWith("<button");
}

function fingerOptions(selected = "right-index") {
  return [
    "right-index",
    "right-thumb",
    "right-middle",
    "right-ring",
    "right-little",
    "left-index",
    "left-thumb",
    "left-middle",
    "left-ring",
    "left-little",
  ]
    .map(
      (finger) => `
        <option value="${finger}" ${finger === selected ? "selected" : ""}>
          ${finger.replace("-", " ")}
        </option>
      `,
    )
    .join("");
}

function formatFingerprintLine(line) {
  line = fingerprintEventLine(line);
  if (!line) return "";
  if (line.startsWith("PROGRESS|")) {
    const [, completed, total] = line.split("|");
    return `Enrollment stage ${completed} of ${total}`;
  }
  if (line.startsWith("ENROLL_STAGES|")) {
    return `Reader requires ${line.split("|")[1]} enrollment stages`;
  }
  if (line.startsWith("DEVICE|")) {
    const [, name, driver, id] = line.split("|");
    return `Reader: ${name} (${driver}, ${id})`;
  }
  if (line.startsWith("READY|")) return `Ready for ${line.split("|")[1]}`;
  if (line.startsWith("RETRY|")) return `Retry: ${line.split("|")[1]}`;
  if (line.startsWith("ENROLLED|")) return "Enrollment completed and stored in SQLite";
  const lower = line.toLowerCase();
  if (lower.includes("place") && lower.includes("finger")) {
    return "Place your finger on the scanner.";
  }
  if (lower.includes("remove") && lower.includes("finger")) {
    return "Lift your finger, then place it again.";
  }
  return line;
}

function fingerprintEventLine(payload) {
  if (typeof payload === "string") return payload.trim();
  if (payload === null || payload === undefined) return "";
  if (typeof payload === "object") {
    if (typeof payload.line === "string") return payload.line.trim();
    if (typeof payload.message === "string") return payload.message.trim();
    if (typeof payload.payload === "string") return payload.payload.trim();
  }
  return String(payload).trim();
}

function renderEnrollmentLog() {
  const logBox = app.querySelector("[data-enrollment-log]");
  if (!logBox) return;
  const log = state.enrollmentLog || [];
  logBox.innerHTML = log.length
    ? log.map((line) => `<div>${escapeHtml(formatFingerprintLine(line))}</div>`).join("")
    : `<div>Ready to enroll.</div>`;
  logBox.scrollTop = logBox.scrollHeight;
}

function wait(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function emptyDbValues(columns) {
  return Object.fromEntries(
    columns.map((column) => [
      column.name,
      column.kind === "bool" ? false : column.kind === "integer" || column.kind === "real" ? null : "",
    ]),
  );
}

function dbField(column, value) {
  const safeValue = dbInputValue(value);
  const disabled = !column.editable || column.protected ? "disabled" : "";
  const protectedClass = column.protected ? " protected" : "";
  if (column.kind === "bool") {
    return `
      <label class="check${protectedClass}">
        <input type="checkbox" name="${escapeHtml(column.name)}" ${value ? "checked" : ""} ${disabled} />
        ${escapeHtml(column.label)}
      </label>
    `;
  }
  if (column.kind === "blob" || column.protected) {
    return `
      <label class="${protectedClass}">${escapeHtml(column.label)}
        <input name="${escapeHtml(column.name)}" value="${escapeHtml(safeValue)}" disabled />
      </label>
    `;
  }
  if (String(safeValue).length > 80) {
    return `
      <label class="wide">${escapeHtml(column.label)}
        <textarea name="${escapeHtml(column.name)}">${escapeHtml(safeValue)}</textarea>
      </label>
    `;
  }
  return `
    <label>${escapeHtml(column.label)}
      <input name="${escapeHtml(column.name)}" value="${escapeHtml(safeValue)}" />
    </label>
  `;
}

function collectDbValues(columns, form) {
  const formData = new FormData(form);
  const values = {};
  columns.forEach((column) => {
    if (!column.editable || column.protected) return;
    if (column.kind === "bool") {
      values[column.name] = formData.get(column.name) === "on";
    } else if (column.kind === "integer") {
      const raw = formData.get(column.name);
      values[column.name] = raw === "" || raw === null ? null : Number.parseInt(raw, 10);
    } else if (column.kind === "real") {
      const raw = formData.get(column.name);
      values[column.name] = raw === "" || raw === null ? null : Number(raw);
    } else {
      values[column.name] = formData.get(column.name) ?? "";
    }
  });
  return values;
}

function dbInputValue(value) {
  if (value === null || value === undefined) return "";
  if (typeof value === "boolean") return value ? "1" : "0";
  return String(value);
}

function dbDisplay(value) {
  if (value === null || value === undefined || value === "") return "";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  return String(value);
}

function emptyEmployee() {
  return {
    id: "",
    name: "",
    finger: "right-index",
    active: true,
    is_admin: false,
    permissions: ["clock"],
    staff_category: "cornice_hand",
  };
}
