import { escapeHtml, invoke } from "../api.js";
import { alertModal } from "../modals.js";
import { app, state, setPanel, table } from "../core.js";

// ==================== Admin: Dispatch Orders Panel ====================

export async function renderDispatchOrdersPanel() {
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
            o.created_at ? o.created_at.replace("T", " ") : "—",
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
    try {
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
    } catch (error) {
      await alertModal({ title: "Dispatch Orders", message: String((error && error.message) || error) });
    }
  });

  app.querySelectorAll("[data-mark-progress]").forEach(btn => {
    btn.addEventListener("click", async () => {
      try {
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
      } catch (error) {
        await alertModal({ title: "Dispatch Orders", message: String((error && error.message) || error) });
      }
    });
  });
}
