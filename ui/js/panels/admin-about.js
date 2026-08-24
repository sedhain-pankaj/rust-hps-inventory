import { escapeHtml, invoke, setBusy } from "../api.js";
import { alertModal, requestAuth } from "../auth.js";
import { app, setPanel, fmtBytes } from "../core.js";

// ==================== Admin: About Panel ====================

function backupTierBlock(label, tier) {
  if (!tier.latest) {
    return `
      <div class="about-tier">
        <div class="about-tier-label">${label}</div>
        <div class="about-tier-sub">None yet</div>
      </div>`;
  }
  return `
    <div class="about-tier">
      <div class="about-tier-label">${label}</div>
      <div class="about-tier-value">${escapeHtml(tier.latest.name)}</div>
      <div class="about-tier-sub">${fmtBytes(tier.latest.size_bytes)}</div>
    </div>`;
}

export async function renderAboutPanel() {
  const [storage, backups] = await Promise.all([invoke("storage_status"), invoke("backup_status")]);
  const diskHot = storage.disk_used_pct > 90;
  const diskClass = diskHot ? "metric-err" : "";
  setPanel(
    "About",
    `<button class="ghost" data-refresh>Refresh</button>`,
    `
      <div class="about-sections">
        <div class="form-section stack">
          <h3 class="form-section-title plain">Storage</h3>
          <div class="about-dev">
            <div class="about-dev-row">
              <span class="about-dev-label">Database</span>
              <span class="about-dev-value" title="${escapeHtml(storage.db_path)}">${fmtBytes(storage.db_size_bytes)}</span>
            </div>
            <div class="about-dev-row">
              <span class="about-dev-label">Disk used</span>
              <span class="about-dev-value ${diskClass}">${Math.round(storage.disk_used_pct)}% of ${fmtBytes(storage.disk_total_bytes)}</span>
            </div>
            <div class="about-dev-row">
              <span class="about-dev-label">Disk free</span>
              <span class="about-dev-value ${diskClass}">${fmtBytes(storage.disk_free_bytes)}</span>
            </div>
          </div>
        </div>
        <div class="form-section stack">
          <h3 class="form-section-title plain">Backup</h3>
          <div class="about-dev">
            ${backupTierBlock("Latest weekly", backups.weekly)}
            ${backupTierBlock("Latest monthly", backups.monthly)}
            <div class="about-tier">
              <div class="about-tier-label">Automatic</div>
              <div class="about-tier-sub">Weekly: Every Sunday at 12pm</div>
              <div class="about-tier-sub">Monthly: First Sunday of the month at 12pm</div>
            </div>
          </div>
          <div>
            <button class="primary" data-backup-now>Backup now</button>
          </div>
        </div>
        <div class="form-section stack">
          <h3 class="form-section-title plain">Exit Kiosk</h3>
          <div class="message">Authenticate with an admin fingerprint to shut down the kiosk.</div>
          <div>
            <button class="danger" data-exit-kiosk>Exit Kiosk</button>
          </div>
        </div>
        <div class="form-section stack">
          <h3 class="form-section-title plain">Contact Developer</h3>
          <div class="about-dev">
            <div class="about-dev-row">
              <span class="about-dev-label">Name</span>
              <span class="about-dev-value">Pankaj Sedhain</span>
            </div>
            <div class="about-dev-row">
              <span class="about-dev-label">Email</span>
              <span class="about-dev-value">sedhain.pankaj@gmail.com</span>
            </div>
          </div>
        </div>
      </div>
    `,
  );
  app.querySelector("[data-refresh]").addEventListener("click", renderAboutPanel);
  app.querySelector("[data-backup-now]").addEventListener("click", async (event) => {
    const button = event.currentTarget;
    setBusy(button);
    try {
      const info = await invoke("create_database_backup");
      await alertModal({
        title: "Database Backup",
        message: `Backup created: ${info.name} (${fmtBytes(info.size_bytes)})`,
      });
    } catch (error) {
      await alertModal({
        title: "Database Backup",
        message: `Backup failed: ${error.message || error}`,
      });
    }
    renderAboutPanel();
  });
  app.querySelector("[data-exit-kiosk]").addEventListener("click", async () => {
    try {
      await requestAuth({ title: "Exit Kiosk", requireAdmin: true });
    } catch {
      return;
    }
    try {
      await invoke("exit_kiosk");
    } catch {
      // The app is terminating; nothing else to do.
    }
  });
}
