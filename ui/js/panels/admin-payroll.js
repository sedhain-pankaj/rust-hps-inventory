import { escapeHtml, invoke, weekStartIso } from "../api.js";
import { alertModal, confirmModal, promptModal } from "../modals.js";
import { app, setPanel, table } from "../core.js";

// ==================== Admin: Payroll Panel ====================

export async function renderPayrollPanel() {
  setPanel("Weekly Payroll", `<button class="ghost" data-refresh>Refresh</button>`, `<div class="message">Loading payroll…</div>`);
  try {
    const weekStart = weekStartIso();
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
            const uv = await promptModal({
              title: "Set Rate",
              label: `Unit value for cornice "${model}" (lengths-to-units ratio)`,
              confirmLabel: "Set Rate",
            }).catch(() => null);
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
                await alertModal({ title: "Set Rate", message: `Error: ${e}` });
              }
            }
          });
        });

        app.querySelectorAll("[data-proration-accept]").forEach(btn => {
          btn.addEventListener("click", async () => {
            const accept = await confirmModal({
              title: "Weekly Payroll",
              body: "Accept the prorated base pay and unit threshold based on the clocked hours for this employee?",
              confirmLabel: "Accept",
            }).catch(() => false);
            if (!accept) return;
            try {
              await invoke("override_payroll_proration", {
                input: {
                  employee_id: btn.dataset.emp,
                  week_start: btn.dataset.week,
                  accept_prorated: true,
                },
              });
              renderPayrollPanel();
            } catch (e) {
              await alertModal({ title: "Weekly Payroll", message: String((e && e.message) || e) });
            }
          });
        });

        app.querySelectorAll("[data-proration-override]").forEach(btn => {
          btn.addEventListener("click", async () => {
            const override = await confirmModal({
              title: "Weekly Payroll",
              body: "Override to standard 40-hr / 180-unit week for this employee?",
              confirmLabel: "Override",
            }).catch(() => false);
            if (!override) return;
            try {
              await invoke("override_payroll_proration", {
                input: {
                  employee_id: btn.dataset.emp,
                  week_start: btn.dataset.week,
                  accept_prorated: false,
                },
              });
              renderPayrollPanel();
            } catch (e) {
              await alertModal({ title: "Weekly Payroll", message: String((e && e.message) || e) });
            }
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
