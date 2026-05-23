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
  staff: [],
  admin: null,
  currentStaff: null,
  adminView: "alerts",
  staffView: "clock",
  selectedEmployee: null,
  selectedStock: null,
  selectedRate: null,
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
      <button class="role-tile" data-role="customer"><strong>Customer</strong><span>Brochure</span></button>
    `,
    "role-grid",
  );
  app.querySelector("[data-back]").addEventListener("click", renderHome);
  app.querySelector('[data-role="admin"]').addEventListener("click", openAdmin);
  app.querySelector('[data-role="staff"]').addEventListener("click", renderStaffPicker);
  app.querySelector('[data-role="customer"]').addEventListener("click", renderCustomer);
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
    button.addEventListener("click", () => {
      state.currentStaff = state.staff.find((item) => item.id === button.dataset.employee);
      state.staffView = "clock";
      renderStaffDashboard();
    });
  });
}

function renderAdmin() {
  const tabs = [
    ["alerts", "Alerts"],
    ["employees", "Employees"],
    ["stock", "Stock"],
    ["rates", "Cornice Rates"],
    ["time", "Time Clock"],
    ["logs", "Daily Logs"],
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
  if (state.adminView === "stock") return renderStockPanel();
  if (state.adminView === "rates") return renderRatesPanel();
  if (state.adminView === "time") return renderTimePanel();
  return renderLogsPanel();
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
        <label>Password<input name="password" type="password" placeholder="Leave blank to keep" /></label>
        <label class="check"><input type="checkbox" name="active" ${selected.active ? "checked" : ""} /> Active</label>
        <label class="check"><input type="checkbox" name="is_admin" ${selected.is_admin ? "checked" : ""} /> Admin</label>
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
        ["Name", "ID", "Admin", "Fingerprint", "Active"],
        employees.map((employee) => ({
          clickable: true,
          attrs: `data-select-employee="${escapeHtml(employee.id)}"`,
          cells: [
            employee.name,
            employee.id,
            employee.is_admin ? "Yes" : "No",
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
      },
    });
    renderEmployeesPanel();
  });
  app.querySelector("[data-enroll]").addEventListener("click", async (event) => {
    setBusy(event.currentTarget);
    try {
      const form = new FormData(app.querySelector("[data-employee-form]"));
      state.selectedEmployee = await invoke("enroll_fingerprint", {
        employeeId: form.get("id"),
        finger: form.get("finger") || "right-index",
      });
      renderEmployeesPanel();
    } catch (error) {
      alert(String(error));
    } finally {
      setBusy(event.currentTarget, false);
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
        ["Time", "Employee", "Action", "Source", "Note"],
        events.map((event) => ({
          review: event.needs_admin_review,
          cells: [
            event.timestamp.replace("T", " "),
            event.employee_name,
            formatAction(event.action),
            event.source,
            event.note,
          ],
        })),
      )}
    `,
  );
  app.querySelector("[data-refresh]").addEventListener("click", renderTimePanel);
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

function renderStaffDashboard() {
  const employee = state.currentStaff;
  const tabs = [["clock", "Clock"]];
  if (employee.permissions.includes("cornice_log")) tabs.push(["cornice", "Cornice"]);
  if (employee.permissions.includes("production_log")) tabs.push(["production", "Production"]);
  if (employee.permissions.includes("overstock")) tabs.push(["overstock", "Overstock"]);
  if (employee.permissions.includes("deliveries")) tabs.push(["deliveries", "Deliveries"]);
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
        <label>Series<input name="series" placeholder="Optional" /></label>
        <label>Model<input name="model" required /></label>
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
  app.querySelector("[data-cornice-form]").addEventListener("submit", async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await invoke("add_cornice_log", {
      input: {
        employee_id: state.currentStaff.id,
        log_date: todayIso(),
        series: form.get("series"),
        model: form.get("model"),
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
  return `
    <header class="topbar">
      <button class="icon ghost" data-back title="Back">Back</button>
      <div class="brand">
        <img src="./assets/HPS.png" alt="" />
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
  return typeof value === "string" && value.trim().startsWith("<");
}

function emptyEmployee() {
  return {
    id: "",
    name: "",
    finger: "right-index",
    active: true,
    is_admin: false,
    permissions: ["clock"],
  };
}
