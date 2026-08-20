import {
  escapeHtml,
  formatAction,
  invoke,
  setBusy,
  todayIso,
  weekStartIso,
} from "./api.js";
import { confirmModal, requestAuth } from "./auth.js";
import { icon } from "./icons.js";
import { createTableStore, mountInlineTable } from "./table.js";
import { mountRatesCardGrid, openRateAddModal } from "./rates-cards.js";

const app = document.getElementById("app");

const state = {
  status: null,
  logoDataUrl: "",
  staff: [],
  admin: null,
  currentStaff: null,
  sessionSource: null,
  adminView: "alerts",
  adminDbTable: "employees",
  selectedDbRow: null,
  enrollmentLog: [],
  staffView: "clock",
  selectedEmployee: null,
  stockFilter: "all",
  corniceRateMatches: [],
  // Session management
  sessionUser: null,
  sessionRole: null,
  idleTimer: null,
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
    invoke("storage_status").catch(() => {});
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
  const logo = state.logoDataUrl || "./assets/HPS.png";
  app.innerHTML = `
    <section class="home">
      <div class="clock-face">
        <img class="clock-logo" src="${logo}" alt="HPS" />
        <div class="clock-time"><span data-time-main>00:00:</span><span class="secs" data-time-secs>00</span></div>
        <div class="clock-date" data-date>00/00/0000 Monday</div>
      </div>
      <button class="primary start-button" data-start>Start</button>
    </section>
  `;
  app.querySelector("[data-start]").addEventListener("click", renderRoleMenu);
  tickClock();
}

let clockTimer = null;
let sessionTimer = null;
function tickClock() {
  if (clockTimer) clearInterval(clockTimer);
  const timeMain = app.querySelector("[data-time-main]");
  const timeSecs = app.querySelector("[data-time-secs]");
  const dateNode = app.querySelector("[data-date]");
  const update = () => {
    const now = new Date();
    if (timeMain || timeSecs) {
      const t = now.toLocaleTimeString("en-AU", {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hour12: false,
      });
      if (timeMain) timeMain.textContent = `${t.slice(0, 5)}:`;
      if (timeSecs) timeSecs.textContent = t.slice(6, 8);
    }
    if (dateNode) {
      const day = now.toLocaleDateString("en-AU", { weekday: "long" });
      const dd = String(now.getDate()).padStart(2, "0");
      const mm = String(now.getMonth() + 1).padStart(2, "0");
      dateNode.textContent = `${dd}/${mm}/${now.getFullYear()} ${day}`;
    }
  };
  update();
  clockTimer = setInterval(update, 1000);
}

const IDLE_TIMEOUT_MS = 5 * 60 * 1000;

function resetIdleTimer() {
  if (state.idleTimer) clearTimeout(state.idleTimer);
  if (!state.sessionUser) return;
  state.lastActivity = Date.now();
  state.idleTimer = setTimeout(() => {
    endSession();
    renderHome();
  }, IDLE_TIMEOUT_MS);
}

function stopIdleTimer() {
  if (state.idleTimer) {
    clearTimeout(state.idleTimer);
    state.idleTimer = null;
  }
}

function startSessionTimer() {
  if (sessionTimer) clearInterval(sessionTimer);
  sessionTimer = setInterval(() => {
    const pill = app.querySelector("[data-status-pill]");
    if (!pill || !state.sessionUser || !state.lastActivity) return;
    const elapsed = Date.now() - state.lastActivity;
    const remaining = Math.max(0, IDLE_TIMEOUT_MS - elapsed);
    const mins = Math.floor(remaining / 60000);
    const secs = Math.floor((remaining % 60000) / 1000);
    const timeStr = `${mins}:${String(secs).padStart(2, "0")}`;
    pill.dataset.remaining = timeStr;
    const remainingEl = app.querySelector("[data-session-remaining]");
    if (remainingEl) remainingEl.textContent = `· ${timeStr}`;
    const totalSecs = Math.floor(remaining / 1000);
    pill.classList.remove("warn", "danger");
    if (totalSecs <= 10) {
      pill.classList.add("danger");
    } else if (totalSecs <= 60) {
      pill.classList.add("warn");
    }
  }, 1000);
}

function stopSessionTimer() {
  if (sessionTimer) {
    clearInterval(sessionTimer);
    sessionTimer = null;
  }
}

function startSession(user, role) {
  state.sessionUser = user;
  state.sessionRole = role;
  resetIdleTimer();
  startSessionTimer();
}

function endSession() {
  state.sessionUser = null;
  state.sessionRole = null;
  stopIdleTimer();
  stopSessionTimer();
  invoke("kill_fingerprint_helpers").catch(() => {});
}

document.addEventListener("click", resetIdleTimer);
document.addEventListener("keydown", resetIdleTimer);
document.addEventListener("touchstart", resetIdleTimer, { passive: true });

lockKioskKeys();
loadStatus();
renderHome();

function renderRoleMenu() {
  if (clockTimer) clearInterval(clockTimer);
  app.innerHTML = screenShell(
    "Choose Access",
    "Hopkins Plaster Studio",
    `
      <button class="role-tile" data-role="admin"><span class="tile-icon">${icon("shield")}</span><strong>Admin</strong><span>Full control</span></button>
      <button class="role-tile" data-role="staff"><span class="tile-icon">${icon("user")}</span><strong>Staff</strong><span>Clocking and daily logs</span></button>
      <button class="role-tile brochure-disabled" disabled title="Coming soon"><span class="tile-icon">${icon("book")}</span><strong>Brochure</strong><span>Coming soon</span></button>
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
    startSession(response.employee, "admin");
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
  state.staff = (await invoke("list_staff", { includeInactive: false })).filter((e) => !e.is_admin);
  app.innerHTML = screenShell(
    "Staff",
    "Choose your name",
    state.staff
      .map((employee) => {
        const initials = employee.name
          .split(/\s+/)
          .filter(Boolean)
          .slice(0, 2)
          .map((word) => word[0].toUpperCase())
          .join("");
        return `
        <button class="staff-tile" data-employee="${escapeHtml(employee.id)}">
          <span class="avatar">${escapeHtml(initials)}</span>
          <strong>${escapeHtml(employee.name)}</strong>
          <span>${escapeHtml(employee.id)}</span>
        </button>
      `;
      })
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
        state.sessionSource = response.source;
        startSession(response.employee, "staff");
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
    ["mould_inventory", "Mould Locations"],
    ["stock", "Stocks"],
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
  app.querySelector("[data-back]").addEventListener("click", () => { endSession(); renderHome(); });
  app.querySelectorAll("[data-tab]").forEach((button) => {
    button.addEventListener("click", () => {
      switchAdminTab(button.dataset.tab);
    });
  });
  renderAdminPanel();
}

function switchAdminTab(id) {
  if (id === state.adminView) return;
  state.adminView = id;
  app.querySelectorAll("[data-tab]").forEach((button) => {
    button.classList.toggle("active", button.dataset.tab === id);
  });
  renderAdminPanel();
}

async function renderAdminPanel() {
  if (state.adminView === "alerts") return renderAlertsPanel();
  if (state.adminView === "employees") return renderEmployeesPanel();
  if (state.adminView === "enroll") return renderEnrollPanel();
  if (state.adminView === "payroll") return renderPayrollPanel();
  if (state.adminView === "dispatch") return renderDispatchOrdersPanel();
  if (state.adminView === "mould_inventory") return renderMouldLocationsPanel();
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
      <div id="enroll-scan-status" class="scan-status" style="display:none"></div>
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
    state.activeEnrollJobId = null;
    state.enrollmentDone = false;
    renderEnrollmentLog();
    try {
      const start = await invoke("start_fingerprint_enroll", {
        employeeId: form.get("employee_id"),
        finger: form.get("finger") || "right-index",
      });
      state.activeEnrollJobId = start.job_id;
      // Show Cancel button inline — no re-render to avoid re-binding submit handler
      const cancelEl = document.createElement("button");
      cancelEl.type = "button";
      cancelEl.className = "ghost cancel-text";
      cancelEl.setAttribute("data-cancel-enroll", "");
      cancelEl.textContent = "Cancel";
      const jobIdForCancel = start.job_id;
      cancelEl.addEventListener("click", async (event) => {
        event.preventDefault();
        event.stopPropagation();
        await invoke("cancel_fingerprint_enroll", { jobId: jobIdForCancel });
        state.enrollmentDone = true;
        renderEnrollPanel();
      });
      button.disabled = true;
      button.textContent = "Enrolling...";
      button.parentNode.insertBefore(cancelEl, button.nextSibling);
      const scanStatus = document.getElementById("enroll-scan-status");
      let enrollAttempts = 0;
      let lastQuality = null;
      if (scanStatus) {
        scanStatus.style.display = "";
        scanStatus.className = "scan-status info";
        scanStatus.textContent = "Place your finger on the scanner...";
      }
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
          for (const line of status.lines) {
            const raw = fingerprintEventLine(line);
            if (raw.startsWith("RETRY|")) {
              enrollAttempts++;
              lastQuality = "retry";
              const reason = raw.split("|").slice(1).join("|");
              if (scanStatus) {
                scanStatus.className = "scan-status warn";
                scanStatus.textContent = `Attempt ${enrollAttempts}: ${mapRetryReason(reason)}`;
              }
            } else if (raw.startsWith("PROGRESS|")) {
              lastQuality = "good";
              const [, completed, total] = raw.split("|");
              if (scanStatus) {
                scanStatus.className = "scan-status ok";
                scanStatus.textContent = `✓ Good scan — stage ${completed}/${total} captured`;
              }
            } else if (raw.startsWith("READY|")) {
              if (scanStatus && !lastQuality) {
                scanStatus.className = "scan-status info";
                scanStatus.textContent = "Scanner ready — place your finger now";
              }
            } else if (raw.startsWith("TEMPLATE|")) {
              const [, num, total] = raw.split("|");
              if (scanStatus) {
                scanStatus.className = "scan-status info";
                scanStatus.textContent = `Enrolling template ${num} of ${total} — place your finger`;
              }
            }
          }
        }
        if (status.state === "done") {
          state.selectedEmployee = status.employee || state.selectedEmployee;
          state.enrollmentDone = true;
          if (scanStatus) scanStatus.style.display = "none";
          break;
        }
        if (status.state === "failed") {
          state.enrollmentDone = true;
          if (scanStatus) scanStatus.style.display = "none";
          throw new Error(status.error || "Enrollment failed.");
        }
      }
      if (state.adminView === "enroll") renderEnrollPanel();
    } catch (error) {
      state.enrollmentLog = [String(error.message || error)];
      renderEnrollmentLog();
      if (state.adminView === "enroll") renderEnrollPanel();
    } finally {
      setBusy(button, false);
    }
  });
}

async function renderStockPanel() {
  const items = await invoke("list_stock_items");
  const filter = state.stockFilter || "all";
  const rows = items.filter((item) =>
    filter === "all"
      ? true
      : filter === "cornice"
        ? item.item_type === "cornice"
        : item.item_type !== "cornice",
  );
  const store = createTableStore({
    commit: {
      add: (values) =>
        invoke("save_stock_item", {
          input: {
            id: null,
            item_type: values.item_type || "cornice",
            model: values.model,
            stock: values.stock,
            reserved: values.reserved,
            location: values.location,
            dimensions: values.dimensions,
            photo_path: values.photo_path,
            notes: values.notes,
          },
        }),
      save: (values) =>
        invoke("save_stock_item", {
          input: {
            id: values.id,
            item_type: values.item_type,
            model: values.model,
            stock: values.stock,
            reserved: values.reserved,
            location: values.location,
            dimensions: values.dimensions,
            photo_path: values.photo_path,
            notes: values.notes,
          },
        }),
      remove: (id) => invoke("delete_stock_item", { id }),
    },
    onDone: renderStockPanel,
  });
  setPanel(
    "Stocks",
    "",
    `
      <div style="margin-bottom:12px">
        <select data-stock-filter style="width:160px">
          <option value="all" ${filter === "all" ? "selected" : ""}>All types</option>
          <option value="cornice" ${filter === "cornice" ? "selected" : ""}>Cornice</option>
          <option value="other" ${filter === "other" ? "selected" : ""}>Other</option>
        </select>
      </div>
      <div data-stock-table></div>
    `,
  );
  const mounted = mountInlineTable(app.querySelector("[data-stock-table]"), store, {
    columns: [
      { key: "item_type", label: "Type", type: "select", options: ["cornice", "other"], editable: true },
      { key: "model", label: "Model", type: "text", editable: true },
      { key: "location", label: "Location", type: "text", editable: true },
      { key: "stock", label: "Stock", type: "number", editable: true, align: "right" },
      { key: "reserved", label: "Reserved", type: "number", editable: true, align: "right" },
      { key: "notes", label: "Notes", type: "text", editable: true },
    ],
    rows,
    tableId: "main",
    emptyText: "No stock items",
    actionsEl: app.querySelector("[data-panel-actions]"),
    refreshFn: renderStockPanel,
    extraActions: `<button class="ghost" data-add-stock>${icon("plus", 18)} Add</button>`,
    onActionsRendered: (el) => {
      el.querySelector("[data-add-stock]")?.addEventListener("click", () => {
        store.addNew("main", {
          item_type: "cornice",
          model: "",
          location: "",
          stock: 0,
          reserved: 0,
          notes: "",
          dimensions: "",
          photo_path: "",
        });
        mounted.render();
      });
    },
  });
  app.querySelector("[data-stock-filter]").addEventListener("change", (event) => {
    state.stockFilter = event.currentTarget.value;
    renderStockPanel();
  });
}

let ratesStore = null;

async function renderRatesPanel() {
  const rates = await invoke("list_cornice_rates");
  const groups = {};
  for (const rate of rates) {
    const series = rate.series || "(no series)";
    (groups[series] ||= []).push(rate);
  }
  const seriesNames = Object.keys(groups).sort();
  const displaySeriesNames = seriesNames.length ? seriesNames : ["New series"];
  setPanel(
    "Cornice Rates",
    "",
    `
      <div class="rate-series-layout">
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
      </div>
    `,
  );

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
  const store = ratesStore;
  const actionsEl = app.querySelector("[data-panel-actions]");
  const mounted = {};
  displaySeriesNames.forEach((series) => {
    mounted[series] = mountRatesCardGrid(
      app.querySelector(`[data-rate-group="${CSS.escape(series)}"]`),
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

  app.querySelectorAll("[data-rate-add]").forEach((btn) => {
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

async function renderDatabasePanel() {
  const tables = await invoke("list_admin_tables");
  if (!tables.some((table) => table.name === state.adminDbTable)) {
    state.adminDbTable = tables[0]?.name || "employees";
  }
  const data = await invoke("list_admin_table_rows", { table: state.adminDbTable });
  const storage = await invoke("storage_status");
  const storageLine = `<div class="message">DB ${fmtBytes(storage.db_size_bytes)} · Disk ${Math.round(storage.disk_used_pct)}% used · ${fmtBytes(storage.disk_free_bytes)} free</div>`;
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
    storageLine +
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
        <div class="metric"><span>Unresolved</span><strong class="${unresolved.length ? "metric-err" : ""}">${unresolved.length}</strong></div>
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
            `<span class="tag ${p.status === 'final' ? 'tag-ok' : p.status === 'unresolved' ? 'tag-err' : 'tag-warn'}">${escapeHtml(p.status)}</span>`,
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

// ==================== Admin: Mould Locations Panel ====================

let mouldStore = null;

async function renderMouldLocationsPanel() {
  const [locations, items] = await Promise.all([
    invoke("list_mould_locations"),
    invoke("list_mould_inventory"),
  ]);
  const actionsEl = app.querySelector("[data-panel-actions]");
  const locationColumns = [
    { key: "mould_name", label: "Mould Name", type: "text", editable: true },
    {
      key: "storage_location",
      label: "Location",
      type: "select",
      options: locations.map((loc) => loc.name),
      editable: true,
    },
    { key: "notes", label: "Notes", type: "text", editable: true },
  ];
  const boxes = locations
    .map((loc) => {
      const locItems = items.filter((item) => item.storage_location === loc.name);
      return `
        <div class="day-box">
          <h3>
            <span>${escapeHtml(loc.name)} <small>(${locItems.length})</small></span>
            <span style="display:flex;gap:8px">
              <button class="ghost" data-add-mould="${escapeHtml(loc.name)}" style="min-height:36px">${icon("plus", 16)} Add</button>
              ${loc.sort_order >= 3 ? `<button class="icon ghost" data-del-loc="${loc.id}" title="Delete location (only if empty)">${icon("x", 16)}</button>` : ""}
            </span>
          </h3>
          <div data-mould-table="${escapeHtml(loc.name)}"></div>
        </div>`;
    })
    .join("");
  const unmatched = items.filter((item) => !locations.some((loc) => loc.name === item.storage_location));
  const unassigned = unmatched.length
    ? `
      <div class="day-box">
        <h3><span>Unassigned <small>(legacy locations)</small></span></h3>
        <div data-mould-table="unassigned"></div>
      </div>`
    : "";
  setPanel("Mould Locations", "", `<div class="location-series-layout">${boxes}${unassigned}</div>` || `<div class="empty">No moulds registered.</div>`);

  if (!mouldStore) {
    mouldStore = createTableStore({
      commit: {
        add: (values) => invoke("save_mould_inventory", { input: values }),
        save: (values) => invoke("save_mould_inventory", { input: values }),
        remove: (id) => invoke("delete_mould_inventory", { id }),
      },
      onDone: () => {
        mouldStore = null;
        renderMouldLocationsPanel();
      },
    });
  }
  const store = mouldStore;
  store.renderActions(actionsEl, {
    refreshFn: renderMouldLocationsPanel,
    extraActions: `<button class="ghost" data-add-loc>${icon("plus", 18)} Add Location</button>`,
    onRendered: (el) => {
      el.querySelector("[data-add-loc]")?.addEventListener("click", async () => {
        const name = prompt("New mould location name:");
        if (!name || !name.trim()) return;
        try {
          await invoke("save_mould_location", { input: { id: null, name: name.trim() } });
          renderMouldLocationsPanel();
        } catch (error) {
          alert(String(error.message || error));
        }
      });
    },
  });
  const mountFor = (locName, locItems) =>
    mountInlineTable(app.querySelector(`[data-mould-table="${CSS.escape(locName)}"]`), store, {
      columns: locationColumns,
      rows: locItems,
      tableId: `loc-${CSS.escape(locName)}`,
      emptyText: "No moulds in this location",
      actionsEl,
      refreshFn: renderMouldLocationsPanel,
    });
  const mounted = {};
  locations.forEach((loc) => {
    mounted[loc.name] = mountFor(loc.name, items.filter((item) => item.storage_location === loc.name));
  });
  if (unmatched.length) mounted.unassigned = mountFor("unassigned", unmatched);

  locations.forEach((loc) => {
    app.querySelector(`[data-add-mould="${CSS.escape(loc.name)}"]`).addEventListener("click", () => {
      store.addNew(`loc-${CSS.escape(loc.name)}`, {
        mould_name: "",
        storage_location: loc.name,
        notes: "",
      });
      mounted[loc.name].render();
    });
  });
  locations.filter((loc) => loc.sort_order >= 3).forEach((loc) => {
    app.querySelector(`[data-del-loc="${loc.id}"]`)?.addEventListener("click", async () => {
      if (!confirm(`Delete location "${loc.name}"? Only works when it has no moulds.`)) return;
      try {
        await invoke("delete_mould_location", { id: loc.id });
        renderMouldLocationsPanel();
      } catch (error) {
        alert(String(error.message || error));
      }
    });
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
            `<span class="tag ${o.status === 'delivered' ? 'tag-ok' : o.status === 'pending' ? 'tag-err' : 'tag-warn'}">${escapeHtml(o.status)}</span>`,
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
  app.querySelector("[data-back]").addEventListener("click", () => { endSession(); renderHome(); });
  app.querySelectorAll("[data-tab]").forEach((button) => {
    button.addEventListener("click", () => {
      switchStaffTab(button.dataset.tab);
    });
  });
  renderStaffPanel();
}

function switchStaffTab(id) {
  if (id === state.staffView) return;
  state.staffView = id;
  app.querySelectorAll("[data-tab]").forEach((button) => {
    button.classList.toggle("active", button.dataset.tab === id);
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
  if (view === "cornice_stock") return renderStockPanel();
  if (view === "cornice_stock_ro") return renderStaffStockRO();
  if (view === "payroll") return renderStaffPayroll();
  return renderStaffRates();
}

async function renderStaffClock(message = "") {
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
let corniceModelSearchTimer = null;

function bindCorniceModelSearch() {
  const panelBody = app.querySelector("[data-panel-body]");
  if (!panelBody || panelBody.dataset.corniceSearchBound) return;
  panelBody.dataset.corniceSearchBound = "1";
  panelBody.addEventListener("input", (event) => {
    const input = event.target;
    if (!input.matches('input[data-key="model"]') || state.staffView !== "cornice") return;
    const query = input.value.trim();
    clearTimeout(corniceModelSearchTimer);
    if (query.length < 2) {
      app.querySelector("#cornice-search-results")?.style.setProperty("display", "none");
      return;
    }
    corniceModelSearchTimer = setTimeout(async () => {
      try {
        const resp = await invoke("search_cornice_rates", { request: { query } });
        state.corniceRateMatches = resp.matches || [];
        const box = app.querySelector("#cornice-search-results");
        if (!box) return;
        box.innerHTML = (resp.matches || []).length
          ? `<label class="search-match-label">${resp.matches.length} match(es)<select data-rate-match><option value="">Pick a rate...</option>${resp.matches
              .map((match, index) => `<option value="${index}">${escapeHtml(match.series ? `${match.series} · ` : "")}${escapeHtml(match.model)} · ${escapeHtml(match.unit_text || "Custom")}</option>`)
              .join("")}</select></label>`
          : "No match found — will be logged as unknown/custom.";
        box.querySelector("[data-rate-match]")?.addEventListener("change", (event) => {
          const match = state.corniceRateMatches[Number(event.currentTarget.value)];
          const modelInput = app.querySelector('input[data-key="model"]:focus') || app.querySelector('input[data-key="model"]');
          if (!match || !modelInput) return;
          modelInput.value = match.model;
          modelInput.dispatchEvent(new Event("change", { bubbles: true }));
          box.insertAdjacentHTML("beforeend", `<div class="message">${escapeHtml(match.unit_text || "Custom")} selected</div>`);
        });
        box.style.display = "block";
      } catch {
        /* ignore */
      }
    }, 200);
  });
}

async function renderStaffCornice() {
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
      <datalist id="staff-cornice-models"></datalist>
      <div id="cornice-search-results" class="message" style="display:none;margin-bottom:12px;font-size:0.9em;"></div>
      ${body || `<div class="empty">No log entries yet.</div>`}
    `,
  );

  try {
    const resp = await invoke("search_cornice_rates", { request: { query: "" } });
    state.corniceRateMatches = resp.matches || [];
    app.querySelector("#staff-cornice-models").innerHTML = (resp.matches || [])
      .map(
        (match) =>
          `<option value="${escapeHtml(match.model)}">${escapeHtml(match.series ? `${match.series} ` : "")}${escapeHtml(match.model)} (${match.unit_text})</option>`,
      )
      .join("");
  } catch {
    /* ignore */
  }
  bindCorniceModelSearch();

  if (!staffLogStore) {
    staffLogStore = createTableStore({
      commit: {
        add: (values) => {
          const match = state.corniceRateMatches.find(
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
              list: "staff-cornice-models",
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
              key: "unit_text",
              label: "Unit",
              type: "text",
              editable: false,
              cellHtml: (log) => escapeHtml(log.unit_text || "Custom"),
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
                unit_text: "",
                total_units: 0,
              });
              mountedDays[today]?.render();
            });
          },
        },
      );
    });
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
            `<span class="tag ${o.status === 'delivered' ? 'tag-ok' : o.status === 'pending' ? 'tag-err' : 'tag-warn'}">${escapeHtml(o.status)}</span>`,
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
  const groups = {};
  for (const rate of rates) {
    const series = rate.series || "(no series)";
    (groups[series] ||= []).push(rate);
  }
  const seriesNames = Object.keys(groups).sort();
  const body = seriesNames.length
    ? `<div class="rate-series-layout">${seriesNames
      .sort()
      .map(
        (series) => `
      <section class="rate-group"><h3>${escapeHtml(series)}</h3>
      ${table(["Model", "Unit"], groups[series].map((rate) => ({ cells: [rate.model, rate.unit_text] })))}</section>`,
      )
      .join("")}</div>`
    : `<div class="empty">No rates yet.</div>`;
  setPanel(
    "Cornice Rates (Read-Only)",
    `<button class="ghost" data-refresh>Refresh</button>`,
    body,
  );
  app.querySelector("[data-refresh]").addEventListener("click", renderStaffRates);
}

async function renderStaffMouldView() {
  const [locations, items] = await Promise.all([
    invoke("list_mould_locations"),
    invoke("list_mould_inventory"),
  ]);
  const boxes = locations
    .map((loc) => {
      const locItems = items.filter((item) => item.storage_location === loc.name);
      return `
        <div class="day-box">
          <h3><span>${escapeHtml(loc.name)}</span><small>${locItems.length}</small></h3>
          ${table(["Mould Name", "Notes"], locItems.map((item) => ({ cells: [item.mould_name, item.notes] })))}
        </div>`;
    })
    .join("");
  const unmatched = items.filter((item) => !locations.some((loc) => loc.name === item.storage_location));
  const unassigned = unmatched.length
    ? `<div class="day-box"><h3><span>Unassigned</span></h3>${table(
        ["Mould Name", "Location", "Notes"],
        unmatched.map((item) => ({ cells: [item.mould_name, item.storage_location, item.notes] })),
      )}</div>`
    : "";
  setPanel(
    "Mould Locations (Read-Only)",
    `<button class="ghost" data-refresh>Refresh</button>`,
    boxes + unassigned || `<div class="empty">No moulds registered.</div>`,
  );
  app.querySelector("[data-refresh]").addEventListener("click", renderStaffMouldView);
}

async function renderStaffStockRO() {
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
        <div class="metric"><span>Status</span><strong class="${payroll.status === 'final' ? 'metric-ok' : payroll.status === 'unresolved' ? 'metric-err' : 'metric-warn'}">${escapeHtml(payroll.status)}</strong></div>
      </div>
    `;

    body += `<h3>Pay Breakdown</h3>`;
    body += `<div class="table-wrap"><table class="table"><tbody>`;
    body += `<tr><td>Base Pay</td><td><strong>$${payroll.base_pay.toFixed(2)}</strong></td></tr>`;
    body += `<tr><td>Known Units</td><td>${payroll.total_units_known.toFixed(1)}</td></tr>`;
    body += `<tr><td>Unit Threshold</td><td>${payroll.unit_threshold.toFixed(0)} <small>(${escapeHtml(payroll.threshold_note)})</small></td></tr>`;
    if (payroll.extra_unit_pay > 0) {
      body += `<tr><td>Extra Unit Pay (${payroll.total_units_known - payroll.unit_threshold} extra × $3.80)</td><td><strong>$${payroll.extra_unit_pay.toFixed(2)}</strong></td></tr>`;
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

function screenShell(title, subtitle, content, contentClass = "") {
  return `
    <section class="screen">
      ${topbar(title, subtitle)}
      <div class="${contentClass}">${content}</div>
    </section>
  `;
}

const tabIcons = {
  alerts: "bell",
  employees: "users",
  enroll: "fingerprint",
  payroll: "dollar",
  dispatch: "truck",
  mould_inventory: "map-pin",
  stock: "package",
  rates: "calculator",
  time: "clock",
  logs: "list",
  database: "database",
  clock: "clock",
  cornice: "box",
  moulds: "map-pin",
  production: "gauge",
  deliveries: "truck",
  cornice_stock: "package",
  cornice_stock_ro: "package",
  overstock: "box",
};

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
                  ${icon(tabIcons[id] || "list")}${escapeHtml(label)}
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
  const logo = state.logoDataUrl || "./assets/HPS.png";
  return `
    <header class="topbar">
      <button class="icon ghost" data-back title="Back">${icon("back-arrow")}</button>
      <div class="brand">
        <img src="${logo}" alt="" />
        <div class="title">
          <h1>${escapeHtml(title)}</h1>
          <p>${escapeHtml(subtitle)}</p>
        </div>
      </div>
      <div class="status-pill" data-status-pill title="${escapeHtml(state.status?.database_path || "")}">
        ${icon("clock", 16)}
        <span>Session <span data-session-remaining></span></span>
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
    <div class="table-wrap">
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
    </div>
  `;
}

function cellLooksHtml(value) {
  const v = typeof value === "string" ? value.trim() : "";
  return (
    v.startsWith("<button") ||
    v.startsWith('<span class="tag') ||
    v.startsWith('<span class="old-new') ||
    v.startsWith('<span class="amended-model')
  );
}

function fmtBytes(bytes) {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = Number(bytes);
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value >= 100 || index === 0 ? Math.round(value) : value.toFixed(1)} ${units[index]}`;
}

function shiftIso(iso, days) {
  const [year, month, day] = iso.split("-").map(Number);
  const date = new Date(year, month - 1, day + days);
  const pad = (value) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function parsePrevValues(log) {
  if (!log.prev_values) return null;
  try {
    const parsed = JSON.parse(log.prev_values);
    return parsed && typeof parsed === "object" ? parsed : null;
  } catch {
    return null;
  }
}

// Raw-HTML cell for cornice log Model/Lengths cells (pending old→new, ✎ cue, staged delete).
// Returns null → caller falls back to the plain escaped value.
function corniceLogCellHtml(log, key) {
  const rateMatch = state.corniceRateMatches.find(
    (rate) => rate.model.toLowerCase() === String(log.model || "").trim().toLowerCase(),
  );
  if (key === "unit_text" && !log.unit_text && rateMatch) {
    return escapeHtml(rateMatch.unit_text || "Custom");
  }
  if (key === "total_units" && rateMatch && log.lengths != null) {
    return (Math.round(Number(rateMatch.unit_value || 0) * Number(log.lengths) * 100) / 100).toFixed(2);
  }
  const prev = parsePrevValues(log);
  if (prev && prev.deleted) {
    return `<span class="old-new"><s>${escapeHtml(log.model)}</s> <span class="tag warn">pending deletion</span></span>`;
  }
  let cell;
  if (prev && prev[key] !== undefined && String(prev[key]) !== String(log[key])) {
    cell = `<span class="old-new"><s>${escapeHtml(prev[key])}</s> → ${escapeHtml(log[key])}</span>`;
  } else {
    cell = escapeHtml(log[key]);
  }
  if (log.amended_at) {
    return `<span class="amended-model">${cell}<span class="amended" title="Amended ${escapeHtml(log.amended_at)} by ${escapeHtml(log.amended_by || "")}">✎</span></span>`;
  }
  return key === "model" ? null : cell;
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
    return `✓ Enrollment stage ${completed} of ${total}`;
  }
  if (line.startsWith("ENROLL_STAGES|")) {
    return `Reader requires ${line.split("|")[1]} enrollment stages`;
  }
  if (line.startsWith("TEMPLATE|")) {
    const [, num, total] = line.split("|");
    return `Enrolling template ${num} of ${total}`;
  }
  if (line.startsWith("DEVICE|")) {
    const [, name, driver, id] = line.split("|");
    return `Reader: ${name} (${driver}, ${id})`;
  }
  if (line.startsWith("READY|")) return `Ready for ${line.split("|")[1]}`;
  if (line.startsWith("RETRY|")) {
    const reason = line.split("|").slice(1).join("|");
    return mapRetryReason(reason);
  }
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

function mapRetryReason(reason) {
  const lower = reason.toLowerCase();
  if (lower.includes("center") || lower.includes("not centered"))
    return "⚠ Finger not centered — reposition and try again";
  if (lower.includes("remove") || lower.includes("lift"))
    return "⚠ Lift finger, wait for prompt, then place again";
  if (lower.includes("short") || lower.includes("too short"))
    return "⚠ Scan too short — keep finger steady longer";
  if (lower.includes("fast") || lower.includes("too fast"))
    return "⚠ Scan too fast — slow down and hold steady";
  if (lower.includes("minutiae"))
    return "⚠ Could not detect fingerprint details — try with clean, dry finger";
  if (lower.includes("quality") || lower.includes("poor"))
    return "⚠ Poor scan quality — adjust pressure and angle";
  if (lower.includes("try again"))
    return "⚠ Scan unclear — reposition finger and try again";
  return `⚠ Retry: ${reason}`;
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

function dbDisplay(columnName, value) {
  if (value === null || value === undefined || value === "") return "";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  const str = String(value);
  if (columnName && /timestamp|created_at|updated_at|edited_at/.test(columnName) && str.includes("T")) {
    return str.replace("T", " ");
  }
  return str;
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
