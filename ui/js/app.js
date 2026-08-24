import { escapeHtml, invoke } from "./api.js";
import { requestAuth } from "./auth.js";
import { icon } from "./icons.js";
import { app, state, setPanel } from "./core.js";
import { renderAlertsPanel } from "./panels/admin-alerts.js";
import { renderEmployeesPanel, renderEnrollPanel } from "./panels/admin-employees.js";
import { renderStockPanel } from "./panels/admin-stock.js";
import { renderRatesPanel } from "./panels/admin-rates.js";
import { renderTimePanel, renderLogsPanel, renderDatabasePanel } from "./panels/admin-logs.js";
import { renderAboutPanel } from "./panels/admin-about.js";
import { renderPayrollPanel } from "./panels/admin-payroll.js";
import { renderMouldLocationsPanel } from "./panels/admin-moulds.js";
import { renderDispatchOrdersPanel } from "./panels/admin-dispatch.js";
import {
  renderStaffClock,
  renderStaffCornice,
  renderStaffProduction,
  renderStaffOverstock,
  renderStaffDeliveries,
  renderDriverDispatchView,
  renderStaffRates,
  renderStaffMouldView,
  renderStaffStockRO,
  renderStaffPayroll,
} from "./panels/staff.js";

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
        state.staffView = null;
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
    ["enroll", "Fingerprint"],
    ["payroll", "Payroll"],
    ["dispatch", "Dispatch"],
    ["mould_inventory", "Mould Locations"],
    ["stock", "Stocks"],
    ["rates", "Cornice Rates"],
    ["time", "Time Clock"],
    ["logs", "Daily Logs"],
    ["database", "Database"],
    ["about", "About"],
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
  if (state.adminView === "about") return renderAboutPanel();
  return renderDatabasePanel();
}

function renderStaffDashboard() {
  const employee = state.currentStaff;
  const category = employee.staff_category || "cornice_hand";
  const hasPermission = (permission) => (employee.permissions || []).includes(permission);
  const tabs = [];
  if (hasPermission("clock")) tabs.push(["clock", "Clock"]);
  if (category === "cornice_hand" && hasPermission("cornice_log")) {
    tabs.push(["cornice", "Cornice"]);
    tabs.push(["payroll", "My Payroll"]);
  }
  if (category === "storekeeper") {
    if (hasPermission("cornice_log")) tabs.push(["cornice", "Cornice Logs"]);
    tabs.push(["cornice_stock", "Stock"]);
    if (hasPermission("production_log")) tabs.push(["production", "Production"]);
    if (hasPermission("deliveries")) tabs.push(["deliveries", "Deliveries"]);
  }
  if (category === "non_cornice" && hasPermission("production_log")) {
    tabs.push(["production", "Production"]);
    tabs.push(["payroll", "My Payroll"]);
  }
  if (category === "driver") {
    tabs.push(["dispatch", "Dispatch Orders"]);
    if (hasPermission("deliveries")) tabs.push(["deliveries", "Deliveries"]);
  }
  if (category === "helper") {
    tabs.push(["cornice_stock_ro", "Stock"]);
  }
  // Legacy permissions fallback
  if (hasPermission("overstock")) tabs.push(["overstock", "Overstock"]);
  if (hasPermission("cornice_rates_view")) tabs.push(["rates", "Rates"]);
  // Mould Locations: view-only, granted per employee via Role & Permissions
  if (hasPermission("mould_view") && !tabs.some(([id]) => id === "moulds")) {
    tabs.push(["moulds", "Moulds"]);
  }

  state.staffTabs = tabs.map(([id]) => id);
  if (!state.staffTabs.includes(state.staffView)) {
    state.staffView = state.staffTabs[0] || null;
  }

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
  if (!view || !state.staffTabs.includes(view)) {
    setPanel("Staff", "", `<div class="empty">No access — this account has no enabled permissions.</div>`);
    return;
  }
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
  about: "info",
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
