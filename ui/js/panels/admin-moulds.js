import { escapeHtml, invoke, setBusy } from "../api.js";
import { icon } from "../icons.js";
import { alertModal, confirmModal } from "../modals.js";
import { mountSearchBox, matchesQuery } from "../search.js";
import { app, state, setPanel } from "../core.js";

// ==================== Admin: Mould Locations Panel ====================

export function mouldsInLocation(loc, columns, items) {
  const locCols = columns.filter((col) => col.location_id === loc.id);
  return items.filter(
    (item) =>
      (item.column_id != null && locCols.some((col) => col.id === item.column_id)) ||
      (item.column_id == null && item.storage_location === loc.name),
  );
}

// Shared board skeleton (location boxes -> column cards -> mould slots) used by
// both the admin (editable) and staff (read-only) mould views. View-specific
// bits — delete buttons, add forms, location actions — are injected via the
// columnHeader / slot / slotForm / locationHeader callbacks.
export function mouldBoardHtml({
  locations,
  columns,
  visibleItems,
  columnHeader,
  slot,
  slotForm = null,
  locationHeader,
}) {
  const columnsFor = (locationId) => columns.filter((col) => col.location_id === locationId);
  return locations
    .map((loc) => {
      const locItems = mouldsInLocation(loc, columns, visibleItems);
      const locCols = columnsFor(loc.id);
      const colCards = locCols
        .map((col) => {
          const colItems = visibleItems.filter((item) => item.column_id === col.id);
          return `
            <div class="mould-column-card">
              <h4>${columnHeader(col, loc, locCols)}</h4>
              <div class="mould-slots">
                ${colItems.map((item) => slot(item)).join("")}
                ${slotForm ? slotForm(col) : ""}
              </div>
            </div>`;
        })
        .join("");
      return `
        <div class="day-box">
          <h3>${locationHeader(loc, locItems.length)}</h3>
          <div class="mould-columns">${colCards}</div>
        </div>`;
    })
    .join("");
}

export async function renderMouldLocationsPanel() {
  const [locations, columns, items] = await Promise.all([
    invoke("list_mould_locations"),
    invoke("list_mould_location_columns"),
    invoke("list_mould_inventory"),
  ]);
  const columnsFor = (locationId) => columns.filter((col) => col.location_id === locationId);
  const filter = state.mouldFilter || "";
  const visibleItems = filter ? items.filter((item) => matchesQuery(item.mould_name, filter)) : items;
  const locationLabel = (item) => {
    if (item.column_id != null) {
      const col = columns.find((c) => c.id === item.column_id);
      const loc = locations.find((l) => l.id === col?.location_id);
      return loc && col ? `${loc.name} · ${col.name}` : "Unassigned";
    }
    return item.storage_location || "Unassigned";
  };

  const boxes = mouldBoardHtml({
    locations,
    columns,
    visibleItems,
    columnHeader: (col, loc, locCols) => {
      const lastColId = locCols.length ? locCols[locCols.length - 1].id : null;
      const actualCount = items.filter((item) => item.column_id === col.id).length;
      const isLast = col.id === lastColId;
      const isOnly = locCols.length === 1;
      const delTitle = isOnly
        ? "A location must keep at least one column"
        : isLast
          ? actualCount
            ? `Delete column (moves ${actualCount} mould(s) to Unassigned)`
            : "Delete column"
          : "Only the last column can be deleted (delete from the end)";
      const delBtn = isOnly
        ? ""
        : `<button class="icon ghost" data-del-col="${col.id}" title="${escapeHtml(delTitle)}"${isLast ? "" : " disabled"}>${icon("trash", 14)}</button>`;
      return `<span>${escapeHtml(col.name)}</span>${delBtn}`;
    },
    slot: (item) => `
      <div class="mould-slot">
        <span>${escapeHtml(item.mould_name)}</span>
        <button class="icon ghost" data-del-mould="${item.id}" title="Delete mould">${icon("x", 14)}</button>
      </div>`,
    slotForm: (col) => `
      <form class="mould-slot-form" data-add-form="${col.id}">
        <input name="mould_name" placeholder="Mould name" autocomplete="off" />
      </form>`,
    locationHeader: (loc, count) => `
      <span>${escapeHtml(loc.name)} <small>(${count})</small></span>
      <span style="display:flex;gap:8px">
        <button class="ghost" data-add-col="${loc.id}" style="min-height:36px">${icon("plus", 16)} Add Column</button>
      </span>`,
  });
  const unmatched = visibleItems.filter((item) => item.column_id == null);
  const unassigned = unmatched.length
    ? `
      <div class="day-box">
        <h3><span>Unassigned <small>(legacy locations)</small></span></h3>
        ${unmatched
          .map(
            (item) => `
            <div class="mould-unassigned-row">
              <span>${escapeHtml(item.mould_name)} <small>(${escapeHtml(item.storage_location || "no location")})</small></span>
              <span style="display:flex;gap:8px;align-items:center">
                <select data-assign-col="${item.id}">
                  <option value="">Assign to column…</option>
                  ${locations
                    .map(
                      (loc) => `<optgroup label="${escapeHtml(loc.name)}">
                        ${columnsFor(loc.id)
                          .map((col) => `<option value="${col.id}">${escapeHtml(col.name)}</option>`)
                          .join("")}
                      </optgroup>`,
                    )
                    .join("")}
                </select>
                <button class="ghost" data-assign-go="${item.id}">Assign</button>
                <button class="icon ghost" data-del-mould="${item.id}" title="Delete mould">${icon("x", 16)}</button>
              </span>
            </div>`,
          )
          .join("")}
      </div>`
    : "";
  setPanel(
    "Mould Locations",
    `<button class="ghost" data-edit-locs>${icon("edit", 18)} Edit Location</button>
     <button class="ghost" data-refresh>${icon("refresh", 18)} Refresh</button>`,
    `
      <div data-mould-search></div>
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

  const mouldSearchBox = mountSearchBox(app.querySelector("[data-mould-search]"), {
    placeholder: "Search by mould name…",
    minChars: 1,
    searchFn: (query) => items.filter((item) => matchesQuery(item.mould_name, query)),
    renderMatch: (item) =>
      `${escapeHtml(item.mould_name)}<span class="search-result-meta">${escapeHtml(locationLabel(item))}</span>`,
    onQuery: (query) => {
      if (query === (state.mouldFilter || "")) return;
      state.mouldFilter = query;
      renderMouldLocationsPanel();
    },
    onSelect: (item) => {
      state.mouldFilter = item.mould_name;
      renderMouldLocationsPanel();
    },
  });
  if (filter) mouldSearchBox.setQuery(filter, { trigger: false });

  app.querySelector("[data-refresh]").addEventListener("click", renderMouldLocationsPanel);
  app.querySelector("[data-edit-locs]").addEventListener("click", async () => {
    await openLocationsModal(locations, columns, items);
    renderMouldLocationsPanel();
  });

  app.querySelectorAll("[data-add-col]").forEach((button) => {
    button.addEventListener("click", async () => {
      const locationId = Number(button.dataset.addCol);
      setBusy(button);
      try {
        // Empty name = backend auto-names it R{next}.
        await invoke("save_mould_location_column", {
          input: { id: null, location_id: locationId, name: "" },
        });
        renderMouldLocationsPanel();
      } catch (error) {
        await alertModal({ title: "Mould Locations", message: String(error.message || error) });
      } finally {
        setBusy(button, false);
      }
    });
  });

  app.querySelectorAll("[data-del-col]").forEach((button) => {
    button.addEventListener("click", async () => {
      const columnId = Number(button.dataset.delCol);
      const colName = columns.find((col) => col.id === columnId)?.name || "?";
      const mouldCount = items.filter((item) => item.column_id === columnId).length;
      const confirmed = await confirmModal({
        title: "Mould Locations",
        body: `Delete column "${colName}"?`,
        warning: mouldCount
          ? `This column holds ${mouldCount} mould(s). They will be moved to Unassigned.`
          : null,
        confirmLabel: "Delete",
      }).catch(() => false);
      if (!confirmed) return;
      try {
        await invoke("delete_mould_location_column", { id: columnId });
        renderMouldLocationsPanel();
      } catch (error) {
        await alertModal({ title: "Mould Locations", message: String(error.message || error) });
      }
    });
  });

  app.querySelectorAll("[data-del-mould]").forEach((button) => {
    button.addEventListener("click", async () => {
      const mouldId = Number(button.dataset.delMould);
      const mouldName = items.find((item) => item.id === mouldId)?.mould_name || "?";
      const confirmed = await confirmModal({
        title: "Mould Locations",
        body: `Delete mould "${mouldName}"?`,
        confirmLabel: "Delete",
      }).catch(() => false);
      if (!confirmed) return;
      try {
        await invoke("delete_mould_inventory", { id: mouldId });
        renderMouldLocationsPanel();
      } catch (error) {
        await alertModal({ title: "Mould Locations", message: String(error.message || error) });
      }
    });
  });

  app.querySelectorAll("[data-add-form]").forEach((form) => {
    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      const columnId = Number(form.dataset.addForm);
      const name = form.querySelector("input[name='mould_name']").value.trim();
      if (!name) return;
      try {
        await invoke("save_mould_inventory", {
          input: { id: null, mould_name: name, column_id: columnId },
        });
        renderMouldLocationsPanel();
      } catch (error) {
        await alertModal({ title: "Mould Locations", message: String(error.message || error) });
      }
    });
  });

  app.querySelectorAll("[data-assign-go]").forEach((button) => {
    button.addEventListener("click", async () => {
      const mouldId = Number(button.dataset.assignGo);
      const select = app.querySelector(`[data-assign-col="${mouldId}"]`);
      const columnId = Number(select?.value);
      if (!columnId) {
        await alertModal({ title: "Mould Locations", message: "Choose a column first." });
        return;
      }
      const item = items.find((row) => row.id === mouldId);
      try {
        await invoke("save_mould_inventory", {
          input: { id: mouldId, mould_name: item.mould_name, column_id: columnId },
        });
        renderMouldLocationsPanel();
      } catch (error) {
        await alertModal({ title: "Mould Locations", message: String(error.message || error) });
      }
    });
  });
}

// Add / rename / delete locations in one modal: each location is a row (click
// the name to rename, trash to delete), a plus row adds a new one, the save
// icon commits everything.
function openLocationsModal(locations, columns, items) {
  return new Promise((resolve) => {
    const root = document.getElementById("modal-root");
    const countFor = (loc) => mouldsInLocation(loc, columns, items).length;
    root.innerHTML = `
      <div class="modal-backdrop">
        <section class="modal" role="dialog" aria-modal="true">
          <header>
            <h2>Mould Locations</h2>
            <button class="icon ghost" data-close title="Close">${icon("x")}</button>
          </header>
          <div class="body">
            <div class="loc-editor">
              ${locations
                .map((loc) => {
                  const count = countFor(loc);
                  return `
                    <div class="loc-editor-row" data-loc-row="${loc.id}">
                      <span class="loc-name" data-loc-name="${loc.id}" title="Click to rename">${escapeHtml(loc.name)}</span>
                      <button class="icon ghost" data-loc-del="${loc.id}" title="${count ? `${count} mould(s) stored here — delete them first` : "Delete location"}">${icon("trash", 16)}</button>
                    </div>`;
                })
                .join("")}
              <div class="loc-editor-row loc-add-row">
                <span class="loc-add-plus">${icon("plus", 16)}</span>
                <input data-new-loc placeholder="New location name" autocomplete="off" />
              </div>
            </div>
            <div class="message" data-loc-error></div>
          </div>
          <footer>
            <button class="icon primary" data-loc-save title="Save">${icon("save", 20)}</button>
          </footer>
        </section>
      </div>
    `;

    const errorEl = root.querySelector("[data-loc-error]");
    const showError = (text) => {
      errorEl.textContent = text;
      errorEl.classList.add("error");
    };
    const close = () => {
      root.innerHTML = "";
      resolve();
    };
    root.querySelector("[data-close]").addEventListener("click", close);

    root.querySelectorAll("[data-loc-name]").forEach((nameEl) => {
      nameEl.addEventListener("click", () => {
        const row = nameEl.closest(".loc-editor-row");
        if (row.classList.contains("loc-deleting")) return;
        const input = document.createElement("input");
        input.value = nameEl.textContent;
        input.dataset.locInput = nameEl.dataset.locName;
        input.autocomplete = "off";
        nameEl.replaceWith(input);
        input.focus();
        input.select();
      });
    });

    root.querySelectorAll("[data-loc-del]").forEach((button) => {
      button.addEventListener("click", () => {
        const row = button.closest(".loc-editor-row");
        row.classList.toggle("loc-deleting");
        const input = row.querySelector("[data-loc-input]");
        if (input) input.disabled = row.classList.contains("loc-deleting");
      });
    });

    root.querySelector("[data-loc-save]").addEventListener("click", async (event) => {
      const button = event.currentTarget;
      const deletions = [...root.querySelectorAll(".loc-editor-row.loc-deleting")].map((row) =>
        Number(row.dataset.locRow),
      );
      const renames = [];
      root.querySelectorAll("[data-loc-input]").forEach((input) => {
        const id = Number(input.dataset.locInput);
        if (deletions.includes(id)) return;
        const original = locations.find((loc) => loc.id === id)?.name || "";
        const value = input.value.trim();
        if (value !== original) renames.push({ id, name: value });
      });
      const newName = root.querySelector("[data-new-loc]").value.trim();

      for (const { id, name } of renames) {
        if (!name) {
          showError("Location name cannot be empty.");
          return;
        }
      }
      for (const id of deletions) {
        const loc = locations.find((l) => l.id === id);
        const count = countFor(loc);
        if (count > 0) {
          showError(`Cannot delete "${loc.name}": ${count} mould(s) stored here.`);
          return;
        }
      }
      const keptNames = locations
        .filter((loc) => !deletions.includes(loc.id))
        .map((loc) => {
          const rename = renames.find((r) => r.id === loc.id);
          return (rename ? rename.name : loc.name).toLowerCase();
        });
      const allNames = newName ? [...keptNames, newName.toLowerCase()] : keptNames;
      const seen = new Set();
      for (const name of allNames) {
        if (seen.has(name)) {
          showError(`Duplicate location name: "${name}".`);
          return;
        }
        seen.add(name);
      }

      setBusy(button);
      errorEl.textContent = "";
      errorEl.classList.remove("error");
      try {
        for (const id of deletions) await invoke("delete_mould_location", { id });
        for (const { id, name } of renames) {
          await invoke("save_mould_location", { input: { id, name } });
        }
        if (newName) {
          await invoke("save_mould_location", { input: { id: null, name: newName } });
        }
        close();
      } catch (error) {
        showError(String(error.message || error));
      } finally {
        setBusy(button, false);
      }
    });
  });
}
