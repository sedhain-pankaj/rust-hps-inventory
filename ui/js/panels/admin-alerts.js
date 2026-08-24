import { invoke } from "../api.js";
import { app, setPanel, table } from "../core.js";

export async function renderAlertsPanel() {
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
