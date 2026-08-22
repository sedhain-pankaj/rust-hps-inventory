import { escapeHtml, invoke, setBusy } from "./api.js";
import { icon } from "./icons.js";

const modalRoot = document.getElementById("modal-root");

function wait(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function mapRetryReason(reason) {
  const lower = reason.toLowerCase();
  if (lower.includes("center") || lower.includes("not centered"))
    return "Finger not centered — reposition";
  if (lower.includes("remove") || lower.includes("lift"))
    return "Lift finger, wait for prompt, then place again";
  if (lower.includes("short") || lower.includes("too short"))
    return "Scan too short — keep finger steady longer";
  if (lower.includes("fast") || lower.includes("too fast"))
    return "Scan too fast — slow down and hold steady";
  if (lower.includes("minutiae"))
    return "Could not detect fingerprint details — try clean, dry finger";
  if (lower.includes("quality") || lower.includes("poor"))
    return "Poor scan quality — adjust pressure and angle";
  if (lower.includes("try again"))
    return "Scan unclear — reposition finger";
  return reason;
}

function parseAuthLine(line) {
  if (typeof line !== "string") return "";
  return line.trim();
}

export function closeModal() {
  modalRoot.innerHTML = "";
  invoke("kill_fingerprint_helpers").catch(() => {});
}

export function requestAuth({ title, requireAdmin = false, employee = null }) {
  return new Promise((resolve, reject) => {
    const employeeLabel = employee ? `${employee.name} (${employee.id})` : "Admin";
    let fpFailures = 0;
    const maxFpFailures = 5;
    let passwordVisible = false;
    let scanning = false;
    let aborted = false;

    modalRoot.innerHTML = `
      <div class="modal-backdrop">
        <section class="modal" role="dialog" aria-modal="true">
          <header>
            <h2>${escapeHtml(title)}</h2>
            <button class="icon ghost" data-close title="Close">${icon("x")}</button>
          </header>
          <div class="body">
            <div class="message">${escapeHtml(employeeLabel)}</div>
            <div id="auth-fp-icon" class="auth-fp-icon scanning">
              <span class="ring"></span>
              <span class="ring r2"></span>
              <img src="./assets/noun-fingerprint-1377758.svg" alt="Fingerprint" width="72" height="72" />
            </div>
            <div id="auth-fp-status" class="scan-status info">Scanning…</div>
            <label id="auth-password-label" style="display:none">
              Password
              <input data-password type="password" autocomplete="current-password" placeholder="Enter password…" />
            </label>
            <div class="message" data-message></div>
          </div>
          <footer>
            <button class="primary" data-password-submit style="display:none">Continue</button>
          </footer>
        </section>
      </div>
    `;

    const passwordLabel = modalRoot.querySelector("#auth-password-label");
    const passwordInput = modalRoot.querySelector("[data-password]");
    const fpStatus = modalRoot.querySelector("#auth-fp-status");
    const fpIcon = modalRoot.querySelector("#auth-fp-icon");
    const message = modalRoot.querySelector("[data-message]");
    const closeButton = modalRoot.querySelector("[data-close]");
    const passwordButton = modalRoot.querySelector("[data-password-submit]");

    const showPasswordFallback = () => {
      if (passwordVisible) return;
      passwordVisible = true;
      passwordLabel.style.display = "";
      passwordButton.style.display = "";
      fpStatus.className = "scan-status err";
      fpStatus.textContent = "5/5 tries failed — try again, or use password below.";
      passwordInput.focus();
    };

    const fail = (error) => {
      message.textContent = (error && error.message) || String(error);
      message.classList.add("error");
    };

    const doFingerprintScan = async () => {
      if (scanning || aborted) return;
      scanning = true;
      fpIcon.classList.add("scanning");
      message.textContent = "";
      message.classList.remove("error");
      fpStatus.className = "scan-status info";
      fpStatus.textContent = "Starting scan…";
      try {
        const start = await invoke("start_fingerprint_auth", { requireAdmin });
        let nextIndex = 0;
        let lastRetryReason = null;
        let scanAttempts = 0;

        while (true) {
          await wait(250);
          if (aborted) return;

          const status = await invoke("poll_fingerprint_auth", {
            jobId: start.job_id,
            fromIndex: nextIndex,
          });
          nextIndex = status.next_index ?? nextIndex;

          if (Array.isArray(status.lines) && status.lines.length) {
            for (const line of status.lines) {
              const raw = parseAuthLine(line);
              if (raw.startsWith("ATTEMPT|")) {
                const parts = raw.split("|");
                scanAttempts = parseInt(parts[1]) || scanAttempts;
                fpStatus.className = "scan-status info";
                fpStatus.textContent = `Scan attempt ${scanAttempts}/${parts[2]}: waiting for finger…`;
              } else if (raw.startsWith("RETRY|")) {
                lastRetryReason = raw.split("|").slice(1).join("|");
                fpStatus.className = "scan-status warn";
                fpStatus.textContent = `⚠ Attempt ${scanAttempts}: ${mapRetryReason(lastRetryReason)}`;
              } else if (raw.startsWith("ERROR|")) {
                const err = raw.split("|").slice(1).join("|");
                console.log("[auth] helper error:", err);
              }
            }
          }

          if (status.state === "done") {
            if (employee && status.employee.id !== employee.id) {
              fpStatus.className = "scan-status err";
              fpStatus.textContent = `Wrong fingerprint — this session is for ${employee.name}`;
              scanning = false;
              setTimeout(() => doFingerprintScan(), 1500);
              return;
            }
            closeModal();
            resolve({
              employee: status.employee,
              source: "fingerprint",
            });
            return;
          }
          if (status.state === "failed") {
            let reason = status.error || "Authentication failed.";
            console.log("[auth] fingerprint job failed:", reason);
            break;
          }
        }

        // Job failed — increment failure counter and show reason
        fpFailures++;
        const displayReason = lastRetryReason
          ? `${mapRetryReason(lastRetryReason)}`
          : (fpStatus.textContent || "Scan failed");

        if (fpFailures >= maxFpFailures) {
          showPasswordFallback();
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${displayReason}`;
        } else {
          fpStatus.className = "scan-status err";
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${displayReason}`;
        }
        scanning = false;
        setTimeout(() => doFingerprintScan(), 800);
      } catch (error) {
        fpFailures++;
        let reason = "Scan failed";
        if (typeof error === "string") reason = error;
        else if (error?.message) reason = error.message;
        console.log("[auth] fingerprint invoke error:", error, "->", reason);
        if (fpFailures >= maxFpFailures) {
          showPasswordFallback();
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${reason}`;
        } else {
          fpStatus.className = "scan-status err";
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${reason}`;
        }
        scanning = false;
        setTimeout(() => doFingerprintScan(), 800);
      }
    };

    closeButton.addEventListener("click", () => {
      aborted = true;
      scanning = false;
      closeModal();
      reject(new Error("Authentication cancelled."));
    });

    passwordButton.addEventListener("click", async () => {
      setBusy(passwordButton);
      message.textContent = "";
      message.classList.remove("error");
      try {
        const response = await invoke("authenticate_password", {
          employeeId: employee && employee.id ? employee.id : null,
          password: passwordInput.value,
          requireAdmin,
        });
        aborted = true;
        closeModal();
        resolve(response);
      } catch (error) {
        fail(error);
      } finally {
        setBusy(passwordButton, false);
      }
    });

    fpIcon.addEventListener("click", () => {
      if (!scanning) {
        scanning = false;
        doFingerprintScan();
      }
    });

    passwordInput.addEventListener("keydown", (event) => {
      if (event.key === "Enter") passwordButton.click();
    });

    setTimeout(() => doFingerprintScan(), 300);
  });
}

export function alertModal({ title, message, okLabel = "OK" }) {
  return new Promise((resolve) => {
    modalRoot.innerHTML = `
      <div class="modal-backdrop">
        <section class="modal" role="dialog" aria-modal="true">
          <header>
            <h2>${escapeHtml(title)}</h2>
            <button class="icon ghost" data-close title="Close">${icon("x")}</button>
          </header>
          <div class="body">
            <div class="auth-fp-icon">
              <img src="./assets/noun-fingerprint-1377758.svg" alt="Fingerprint" width="72" height="72" />
            </div>
            <div class="scan-status warn">${escapeHtml(message)}</div>
          </div>
          <footer>
            <button class="primary" data-ok>${escapeHtml(okLabel)}</button>
          </footer>
        </section>
      </div>
    `;
    const close = () => {
      closeModal();
      resolve();
    };
    modalRoot.querySelector("[data-close]").addEventListener("click", close);
    modalRoot.querySelector("[data-ok]").addEventListener("click", close);
  });
}

export function promptModal({
  title,
  label = "",
  placeholder = "",
  initialValue = "",
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
}) {
  return new Promise((resolve, reject) => {
    modalRoot.innerHTML = `
      <div class="modal-backdrop">
        <section class="modal" role="dialog" aria-modal="true">
          <header>
            <h2>${escapeHtml(title)}</h2>
            <button class="icon ghost" data-close title="Close">${icon("x")}</button>
          </header>
          <div class="body">
            ${
              label
                ? `<label class="rate-add-field">${escapeHtml(label)}
                  <input data-prompt-input type="text" placeholder="${escapeHtml(placeholder)}" value="${escapeHtml(initialValue)}" />
                </label>`
                : ""
            }
          </div>
          <footer>
            <button class="ghost" data-cancel>${escapeHtml(cancelLabel)}</button>
            <button class="primary" data-confirm>${escapeHtml(confirmLabel)}</button>
          </footer>
        </section>
      </div>
    `;
    const input = modalRoot.querySelector("[data-prompt-input]");
    const cancel = () => {
      closeModal();
      reject(new Error("Cancelled."));
    };
    const confirm = () => {
      const value = input ? input.value : "";
      closeModal();
      resolve(value);
    };
    modalRoot.querySelector("[data-close]").addEventListener("click", cancel);
    modalRoot.querySelector("[data-cancel]").addEventListener("click", cancel);
    modalRoot.querySelector("[data-confirm]").addEventListener("click", confirm);
    if (input) {
      input.focus();
      input.addEventListener("keydown", (event) => {
        if (event.key === "Enter") confirm();
      });
    }
  });
}

export function confirmModal({
  title,
  body = "",
  warning = null,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
}) {
  return new Promise((resolve, reject) => {
    modalRoot.innerHTML = `
      <div class="modal-backdrop">
        <section class="modal" role="dialog" aria-modal="true">
          <header>
            <h2>${escapeHtml(title)}</h2>
            <button class="icon ghost" data-close title="Close">${icon("x")}</button>
          </header>
          <div class="body">
            ${body ? `<p>${body}</p>` : ""}
            ${warning ? `<div class="scan-status warn">${escapeHtml(warning)}</div>` : ""}
          </div>
          <footer>
            <button class="ghost" data-cancel>${escapeHtml(cancelLabel)}</button>
            <button class="primary" data-confirm>${escapeHtml(confirmLabel)}</button>
          </footer>
        </section>
      </div>
    `;
    const cancel = () => {
      closeModal();
      reject(new Error("Cancelled."));
    };
    modalRoot.querySelector("[data-close]").addEventListener("click", cancel);
    modalRoot.querySelector("[data-cancel]").addEventListener("click", cancel);
    modalRoot.querySelector("[data-confirm]").addEventListener("click", () => {
      closeModal();
      resolve(true);
    });
  });
}
