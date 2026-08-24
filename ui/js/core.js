import { escapeHtml } from "./api.js";

export const app = document.getElementById("app");

export const state = {
  status: null,
  logoDataUrl: "",
  staff: [],
  admin: null,
  currentStaff: null,
  sessionSource: null,
  adminView: "alerts",
  adminDbTable: "employees",
  selectedDbRow: null,
  staffView: null,
  staffTabs: [],
  selectedEmployee: null,
  employeeFormMode: null,
  stockFilter: "all",
  corniceRateMatches: [],
  corniceRateAll: [],
  ratesFilter: "",
  mouldFilter: "",
  staffRatesFilter: "",
  staffMouldFilter: "",
  // Session management
  sessionUser: null,
  sessionRole: null,
  idleTimer: null,
};

export const permissionLabels = {
  clock: "Clock",
  cornice_log: "Cornice log",
  production_log: "Production log",
  overstock: "Overstock",
  deliveries: "Deliveries",
  cornice_rates_view: "Cornice rates",
  daily_production_all: "All production",
  mould_view: "Mould Locations (view)",
};

export function setPanel(title, actions, body) {
  app.querySelector("[data-panel-title]").textContent = title;
  app.querySelector("[data-panel-actions]").innerHTML = actions;
  app.querySelector("[data-panel-body]").innerHTML = body;
}

export function table(headers, rows) {
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

export function cellLooksHtml(value) {
  const v = typeof value === "string" ? value.trim() : "";
  return (
    v.startsWith("<button") ||
    v.startsWith('<span class="tag') ||
    v.startsWith('<span class="old-new') ||
    v.startsWith('<span class="amended-model')
  );
}

export function fmtBytes(bytes) {
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

export function shiftIso(iso, days) {
  const [year, month, day] = iso.split("-").map(Number);
  const date = new Date(year, month - 1, day + days);
  const pad = (value) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

export function getWeekStartForDate(dateStr) {
  const d = new Date(dateStr);
  const day = d.getDay();
  const diff = (day + 5) % 7;
  d.setDate(d.getDate() - diff);
  return d.toISOString().slice(0, 10);
}

export function parsePrevValues(log) {
  if (!log.prev_values) return null;
  try {
    const parsed = JSON.parse(log.prev_values);
    return parsed && typeof parsed === "object" ? parsed : null;
  } catch {
    return null;
  }
}

// Numeric rate from a unit string (mirror of db::unit_value): Unknown→0, a/b→fraction, else first number.
export function unitValue(unit) {
  const t = String(unit || "").trim();
  if (!t || t.toLowerCase() === "unknown") return 0;
  if (t.includes("/")) {
    const [a, b] = t.split("/");
    const num = parseFloat(a);
    const den = parseFloat(b);
    if (!Number.isNaN(num) && !Number.isNaN(den) && den !== 0) return num / den;
  }
  const n = parseFloat(t);
  return Number.isNaN(n) ? 0 : n;
}

// Raw-HTML cell for cornice log Model/Lengths cells (pending old→new, ✎ cue, staged delete).
// Returns null → caller falls back to the plain escaped value.
export function corniceLogCellHtml(log, key) {
  const rateMatch = state.corniceRateMatches.find(
    (rate) => rate.model.toLowerCase() === String(log.model || "").trim().toLowerCase(),
  );
  if (key === "unit" && !log.unit && rateMatch) {
    return escapeHtml(rateMatch.unit || "Custom");
  }
  if (key === "total_units" && rateMatch && log.lengths != null) {
    return (Math.round(unitValue(rateMatch.unit) * Number(log.lengths) * 100) / 100).toFixed(2);
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

export function fingerOptions(selected = "right-index") {
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

export function formatTimestamp(value) {
  if (!value) return "—";
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return value;
  return d.toLocaleString("en-AU", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

export function emptyDbValues(columns) {
  return Object.fromEntries(
    columns.map((column) => [
      column.name,
      column.kind === "bool" ? false : column.kind === "integer" || column.kind === "real" ? null : "",
    ]),
  );
}

export function dbField(column, value) {
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

export function collectDbValues(columns, form) {
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

export function dbInputValue(value) {
  if (value === null || value === undefined) return "";
  if (typeof value === "boolean") return value ? "1" : "0";
  return String(value);
}

export function dbDisplay(columnName, value) {
  if (value === null || value === undefined || value === "") return "";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  const str = String(value);
  if (columnName && /timestamp|created_at|updated_at|edited_at/.test(columnName) && str.includes("T")) {
    return str.replace("T", " ");
  }
  return str;
}

export function emptyEmployee() {
  return {
    id: "",
    name: "",
    finger: "",
    active: true,
    is_admin: false,
    permissions: ["clock"],
    staff_category: "cornice_hand",
  };
}
