import { escapeHtml } from "./api.js";
import { alertModal } from "./auth.js";
import { icon } from "./icons.js";

// Draft store shared by one or more inline tables within a single panel.
export function createTableStore({ commit, onDone }) {
  const drafts = new Map();
  let seq = 0;

  function draftFor(row) {
    const key = row.id != null ? `id-${row.id}` : row.__key;
    if (key == null) return null;
    if (!drafts.has(key)) {
      drafts.set(key, {
        row: { ...row },
        values: null,
        dirty: false,
        deleted: false,
        editing: false,
        isNew: false,
        tableId: null,
      });
    }
    return drafts.get(key);
  }

  function addNew(tableId, defaults) {
    const key = `new-${++seq}`;
    const row = { ...defaults, id: null, __key: key };
    drafts.set(key, {
      row,
      values: { ...defaults },
      dirty: true,
      deleted: false,
      editing: true,
      isNew: true,
      tableId,
    });
    return row;
  }

  function setValues(row, values) {
    const draft = draftFor(row);
    if (!draft) return;
    draft.values = { ...(draft.values || {}), ...values };
    if (draft.isNew) draft.row = { ...draft.row, ...values };
    draft.dirty = true;
  }

  function stageDelete(row) {
    const draft = draftFor(row);
    if (draft) {
      draft.deleted = true;
      draft.dirty = false;
    }
  }

  function unstageDelete(row) {
    const draft = draftFor(row);
    if (draft) draft.deleted = false;
  }

  function pendingDrafts() {
    return [...drafts.values()].filter((draft) => draft.deleted || (draft.dirty && draft.values));
  }

  function hasPending() {
    return pendingDrafts().length > 0;
  }

  function discardAll() {
    drafts.clear();
  }

  async function saveAll() {
    const pending = pendingDrafts();
    if (!pending.length) return;
    for (const draft of pending) {
      if (draft.deleted) {
        if (draft.row.id != null) await commit.remove(draft.row.id);
      } else if (draft.isNew) {
        await commit.add(draft.values);
      } else {
        await commit.save({ ...draft.row, ...draft.values });
      }
    }
    drafts.clear();
    if (onDone) await onDone();
  }

  function renderActions(actionsEl, { refreshFn, extraActions = "", onRendered = null }) {
    if (!actionsEl) return;
    const right = hasPending()
      ? `<button class="icon primary" data-store-save title="Save changes">${icon("save", 20)}</button>
         <button class="icon ghost" data-store-discard title="Discard changes">${icon("x", 20)}</button>`
      : `<button class="ghost" data-store-refresh>${icon("refresh", 18)} Refresh</button>`;
    actionsEl.innerHTML = `${extraActions}${right}`;
    actionsEl.querySelector("[data-store-save]")?.addEventListener("click", () => {
      saveAll().catch((error) => {
        alertModal({ title: "Save Changes", message: String((error && error.message) || error) });
      });
    });
    actionsEl.querySelector("[data-store-discard]")?.addEventListener("click", () => {
      discardAll();
      if (refreshFn) refreshFn();
    });
    actionsEl.querySelector("[data-store-refresh]")?.addEventListener("click", () => {
      if (refreshFn) refreshFn();
    });
    if (onRendered) onRendered(actionsEl);
  }

  return {
    drafts,
    draftFor,
    addNew,
    setValues,
    stageDelete,
    unstageDelete,
    hasPending,
    discardAll,
    saveAll,
    renderActions,
  };
}

// Mount one inline-editable table into rootEl, backed by a shared store.
export function mountInlineTable(rootEl, store, config) {
  const {
    columns,
    rows,
    tableId = "main",
    emptyText = "No records",
    canEdit = () => true,
    canDelete = () => true,
    cellHtml = null,
    rowClass = () => "",
    actionsEl = null,
    refreshFn = null,
    extraActions = "",
    onActionsRendered = null,
  } = config;

  function valueOf(row, col) {
    const draft = store.draftFor(row);
    if (draft && draft.values && draft.values[col.key] !== undefined) return draft.values[col.key];
    return row[col.key];
  }

  function displayCell(row, col) {
    if (cellHtml) {
      const custom = cellHtml(row, col);
      if (custom != null) return custom;
    }
    const value = valueOf(row, col);
    if (value === null || value === undefined || value === "") return "";
    if (col.type === "bool") return value ? "Yes" : "No";
    return escapeHtml(String(value));
  }

  function editControl(row, col) {
    const value = valueOf(row, col);
    if (col.type === "select") {
      const options = (col.options || [])
        .map(
          (opt) =>
            `<option value="${escapeHtml(opt)}" ${String(value) === String(opt) ? "selected" : ""}>${escapeHtml(opt)}</option>`,
        )
        .join("");
      return `<select data-edit data-key="${col.key}">${options}</select>`;
    }
    if (col.type === "bool") {
      return `<select data-edit data-key="${col.key}">
        <option value="true" ${value ? "selected" : ""}>Yes</option>
        <option value="false" ${value ? "" : "selected"}>No</option>
      </select>`;
    }
    const type = col.type === "number" ? "number" : "text";
    const listAttr = col.list ? ` list="${col.list}"` : "";
    return `<input data-edit data-key="${col.key}" type="${type}" value="${escapeHtml(value === null || value === undefined ? "" : value)}"${listAttr} ${col.type === "number" ? 'step="any" min="0"' : ""} />`;
  }

  function allRows() {
    const out = [...rows];
    for (const draft of store.drafts.values()) {
      if (draft.isNew && draft.tableId === tableId) out.push(draft.row);
    }
    return out;
  }

  function rowKeyOf(row) {
    return row.id != null ? String(row.id) : row.__key;
  }

  function findRow(tr) {
    const key = tr.dataset.rowkey;
    return allRows().find((row) => rowKeyOf(row) === key) || null;
  }

  function refreshActions() {
    if (actionsEl) {
      store.renderActions(actionsEl, { refreshFn, extraActions, onRendered: onActionsRendered });
    }
  }

  function render() {
    const data = allRows();
    if (!data.length) {
      rootEl.innerHTML = `<div class="message">${escapeHtml(emptyText)}</div>`;
      refreshActions();
      return;
    }
    rootEl.innerHTML = `
      <div class="table-wrap">
        <table class="table itable">
          <thead>
            <tr>
              ${columns.map((col) => `<th class="${col.align === "right" ? "num" : ""}">${escapeHtml(col.label)}</th>`).join("")}
              <th class="row-actions-head"></th>
            </tr>
          </thead>
          <tbody>
            ${data
              .map((row) => {
                const draft = store.draftFor(row);
                const cls = [
                  draft && draft.deleted ? "staged-delete" : "",
                  draft && draft.dirty ? "dirty" : "",
                  rowClass(row),
                ]
                  .filter(Boolean)
                  .join(" ");
                return `
                  <tr class="${cls}" data-rowkey="${escapeHtml(rowKeyOf(row))}">
                    ${columns
                      .map(
                        (col) => `
                        <td data-cell data-key="${col.key}" class="${col.editable && canEdit(row) && draft?.editing ? "editable" : ""} ${col.align === "right" ? "num" : ""}">
                          ${displayCell(row, col)}
                        </td>`,
                      )
                      .join("")}
                    <td class="row-actions">
                        ${canEdit(row) && !draft?.editing ? `<button class="icon ghost" data-edit-row title="Edit">${icon("more", 16)}</button>` : ""}
                      ${canDelete(row) ? `<button class="icon ghost" data-del title="Delete">${icon("x", 16)}</button>` : ""}
                    </td>
                  </tr>`;
              })
              .join("")}
          </tbody>
        </table>
      </div>
    `;
    bind();
    refreshActions();
  }

  function startEdit(td) {
    if (td.querySelector("[data-edit]")) return;
    const row = findRow(td.closest("tr"));
    const col = columns.find((c) => c.key === td.dataset.key);
    if (!row || !col) return;
    td.innerHTML = editControl(row, col);
    const input = td.querySelector("[data-edit]");
    input.focus();
    if (typeof input.select === "function") input.select();

    let committed = false;
    const commitCell = () => {
      if (committed) return;
      committed = true;
      let value = input.value;
      if (col.type === "number") value = value === "" ? 0 : Number(value);
      if (col.type === "bool") value = value === "true";
      store.setValues(row, { [col.key]: value });
      render();
    };
    input.addEventListener("change", commitCell);
    input.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        commitCell();
      } else if (event.key === "Escape") {
        event.preventDefault();
        committed = true;
        render();
      }
    });
    input.addEventListener("blur", () => {
      if (!committed) {
        committed = true;
        render();
      }
    });
  }

  function bind() {
    rootEl.querySelectorAll("[data-edit-row]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const row = findRow(btn.closest("tr"));
        const draft = row && store.draftFor(row);
        if (!draft) return;
        draft.editing = true;
        render();
      });
    });
    rootEl.querySelectorAll("td.editable").forEach((td) => {
      td.addEventListener("click", () => startEdit(td));
    });
    rootEl.querySelectorAll("[data-del]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const row = findRow(btn.closest("tr"));
        if (!row) return;
        const draft = store.draftFor(row);
        if (draft && draft.deleted) store.unstageDelete(row);
        else store.stageDelete(row);
        render();
      });
    });

  }

  render();
  return { render };
}
