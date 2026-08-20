import { escapeHtml } from "./api.js";
import { icon } from "./icons.js";

// Mount a 2-up grid of cornice cards for one series, backed by a shared store.
// Read-only when editable=false (staff view): no store, no menu, no edit/delete.
export function mountRatesCardGrid(rootEl, store, config) {
  const { rows, series, tableId, editable = true, actionsEl = null, refreshFn = null } = config;

  function allRows() {
    if (!editable || !store) return [...rows];
    const out = [...rows];
    for (const draft of store.drafts.values()) {
      if (draft.isNew && draft.tableId === tableId) out.push(draft.row);
    }
    return out;
  }
  const rowKeyOf = (row) => (row.id != null ? String(row.id) : row.__key);
  const findRow = (key) => allRows().find((row) => rowKeyOf(row) === key) || null;
  function valueOf(row, key) {
    const draft = store && store.draftFor(row);
    if (draft && draft.values && draft.values[key] !== undefined) return draft.values[key];
    return row[key];
  }
  const refreshActions = () => {
    if (actionsEl && store) store.renderActions(actionsEl, { refreshFn });
  };

  function cardHtml(row) {
    const draft = store ? store.draftFor(row) : null;
    if (draft && draft.deleted) return "";
    const key = rowKeyOf(row);
    const editing = !!(draft && draft.editing);
    const dirty = !!(draft && draft.dirty);
    const model = valueOf(row, "model");
    const unit = valueOf(row, "unit");
    const cls = ["rate-card", editing ? "editing" : "", dirty ? "dirty" : ""].filter(Boolean).join(" ");
    const modelField = editing
      ? `<input data-card-model value="${escapeHtml(model)}" />`
      : `<span class="rate-card-model">${escapeHtml(model)}</span>`;
    const unitField = editing
      ? `<input data-card-unit value="${escapeHtml(unit)}" />`
      : `<span class="rate-card-unit">${escapeHtml(unit)}</span>`;
    const menu =
      editable && !editing
        ? `<button class="icon ghost rate-card-menu" data-card-menu title="Options">${icon("more", 16)}</button>`
        : "";
    return `
      <div class="${cls}" data-card data-key="${escapeHtml(key)}">
        ${menu}
        <div class="rate-card-fields">
          <label class="rate-card-field">Cornice<div>${modelField}</div></label>
          <label class="rate-card-field">Unit<div>${unitField}</div></label>
        </div>
      </div>`;
  }

  function render() {
    const data = allRows().filter((row) => {
      const d = store ? store.draftFor(row) : null;
      return !(d && d.deleted);
    });
    if (!data.length) {
      rootEl.innerHTML = `<div class="message">No models in this series</div>`;
      refreshActions();
      return;
    }
    rootEl.innerHTML = `<div class="rate-cards">${data.map(cardHtml).join("")}</div>`;
    bind();
    refreshActions();
  }

  function closeMenu() {
    document.querySelectorAll(".rate-card-popover").forEach((m) => m.remove());
  }
  function openMenu(cardEl) {
    const menu = document.createElement("div");
    menu.className = "rate-card-popover";
    menu.innerHTML = `<button data-menu-edit>Edit</button><button data-menu-delete>Delete</button>`;
    cardEl.appendChild(menu);
    menu.querySelector("[data-menu-edit]").addEventListener("click", () => {
      closeMenu();
      startEdit(cardEl);
    });
    menu.querySelector("[data-menu-delete]").addEventListener("click", () => {
      closeMenu();
      const row = findRow(cardEl.dataset.key);
      if (row) {
        store.stageDelete(row);
        render();
      }
    });
  }
  function startEdit(cardEl) {
    const row = findRow(cardEl.dataset.key);
    if (!row) return;
    const draft = store.draftFor(row);
    if (!draft) return;
    draft.editing = true;
    render();
    const input = rootEl.querySelector(`[data-card="${CSS.escape(cardEl.dataset.key)}"] [data-card-model]`);
    if (input) {
      input.focus();
      input.select();
    }
  }
  function commitEdit(cardEl) {
    const modelInput = cardEl.querySelector("[data-card-model]");
    const unitInput = cardEl.querySelector("[data-card-unit]");
    if (!modelInput || !unitInput) return;
    const row = findRow(cardEl.dataset.key);
    if (!row) return;
    store.setValues(row, { model: modelInput.value.trim(), unit: unitInput.value.trim() });
    const draft = store.draftFor(row);
    if (draft) draft.editing = false;
    render();
  }

  function bind() {
    rootEl.querySelectorAll("[data-card-menu]").forEach((btn) => {
      btn.addEventListener("click", (e) => {
        e.stopPropagation();
        const cardEl = btn.closest("[data-card]");
        const alreadyOpen = cardEl.querySelector(".rate-card-popover");
        closeMenu();
        if (!alreadyOpen) openMenu(cardEl);
      });
    });
    rootEl.querySelectorAll("[data-card-model], [data-card-unit]").forEach((input) => {
      input.addEventListener("keydown", (e) => {
        const cardEl = input.closest("[data-card]");
        if (e.key === "Enter") {
          e.preventDefault();
          commitEdit(cardEl);
        } else if (e.key === "Escape") {
          e.preventDefault();
          const row = findRow(cardEl.dataset.key);
          const draft = row && store.draftFor(row);
          if (draft) draft.editing = false;
          render();
        }
      });
      input.addEventListener("blur", () => {
        const cardEl = input.closest("[data-card]");
        const row = findRow(cardEl.dataset.key);
        const draft = row && store.draftFor(row);
        if (draft && draft.editing) commitEdit(cardEl);
      });
    });
  }

  render();
  return { render, closeMenu };
}

// Themed add modal (matches the fingerprint/confirm modal), not prompt().
export function openRateAddModal(series, onConfirm) {
  const modalRoot = document.getElementById("modal-root");
  modalRoot.innerHTML = `
    <div class="modal-backdrop">
      <section class="modal" role="dialog" aria-modal="true">
        <header>
          <h2>Add cornice — ${escapeHtml(series)}</h2>
          <button class="icon ghost" data-rate-close title="Close">${icon("x")}</button>
        </header>
        <div class="body">
          <label class="rate-add-field">Cornice name
            <input data-rate-model placeholder="e.g. 491" />
          </label>
          <label class="rate-add-field">Unit
            <input data-rate-unit placeholder="e.g. 1.5" />
          </label>
        </div>
        <footer>
          <button class="ghost" data-rate-cancel>Cancel</button>
          <button class="primary" data-rate-confirm>Add</button>
        </footer>
      </section>
    </div>`;
  const close = () => {
    modalRoot.innerHTML = "";
  };
  modalRoot.querySelector("[data-rate-close]").addEventListener("click", close);
  modalRoot.querySelector("[data-rate-cancel]").addEventListener("click", close);
  modalRoot.querySelector("[data-rate-confirm]").addEventListener("click", () => {
    const model = modalRoot.querySelector("[data-rate-model]").value.trim();
    const unit = modalRoot.querySelector("[data-rate-unit]").value.trim();
    if (!model) return;
    close();
    onConfirm({ model, unit: unit || "Unknown" });
  });
  modalRoot.querySelector("[data-rate-model]").focus();
}
