import { invoke } from "../api.js";
import { icon } from "../icons.js";
import { createTableStore, mountInlineTable } from "../table.js";
import { app, state, setPanel, table } from "../core.js";

export async function renderStockPanel() {
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
