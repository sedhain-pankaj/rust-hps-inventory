import { escapeHtml, invoke, setBusy } from "./api.js";

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
            <button class="icon ghost" data-close title="Close">X</button>
          </header>
          <div class="body">
            <div class="message">${escapeHtml(employeeLabel)}</div>
            <div id="auth-fp-icon" class="auth-fp-icon scanning">
              <img src="./assets/noun-fingerprint-1377758.svg" alt="Fingerprint" width="80" height="80" />
            </div>
            <div id="auth-fp-status" class="message" style="margin-top:0.5em;font-size:0.9em;text-align:center;">Scanning…</div>
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
      fpStatus.textContent = "5/5 tries failed — try again, or use password below.";
      fpStatus.style.color = "#c55";
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
      fpStatus.textContent = "Starting scan…";
      fpStatus.style.color = "#2c3e50";
      fpStatus.style.background = "#eaf7ff";
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
                fpStatus.textContent = `Scan attempt ${scanAttempts}/${parts[2]}: waiting for finger…`;
                fpStatus.style.background = "#eaf7ff";
                fpStatus.style.color = "#2c3e50";
              } else if (raw.startsWith("RETRY|")) {
                lastRetryReason = raw.split("|").slice(1).join("|");
                fpStatus.textContent = `⚠ Attempt ${scanAttempts}: ${mapRetryReason(lastRetryReason)}`;
                fpStatus.style.background = "#fff3e6";
                fpStatus.style.color = "#c0571a";
              } else if (raw.startsWith("ERROR|")) {
                const err = raw.split("|").slice(1).join("|");
                console.log("[auth] helper error:", err);
              }
            }
          }

          if (status.state === "done") {
            if (employee && status.employee.id !== employee.id) {
              fpStatus.textContent = `Wrong fingerprint — this session is for ${employee.name}`;
              fpStatus.style.background = "#f8d7da";
              fpStatus.style.color = "#842029";
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
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${displayReason}`;
          fpStatus.style.background = "#fff3e6";
          fpStatus.style.color = "#c55";
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
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${reason}`;
          fpStatus.style.background = "#fff3e6";
          fpStatus.style.color = "#c55";
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

export function chooseClockAction(employee) {
  return new Promise((resolve, reject) => {
    modalRoot.innerHTML = `
      <div class="modal-backdrop">
        <section class="modal" role="dialog" aria-modal="true">
          <header>
            <h2>${escapeHtml(employee.name)}</h2>
            <button class="icon ghost" data-close title="Close">X</button>
          </header>
          <div class="body">
            <div class="message">${escapeHtml(employee.id)}</div>
          </div>
          <footer>
            <button class="warning" data-action="clock_out">Clock out</button>
            <button class="primary" data-action="clock_in">Clock in</button>
          </footer>
        </section>
      </div>
    `;

    modalRoot.querySelector("[data-close]").addEventListener("click", () => {
      closeModal();
      reject(new Error("Clock action cancelled."));
    });
    modalRoot.querySelectorAll("[data-action]").forEach((button) => {
      button.addEventListener("click", () => {
        const action = button.dataset.action;
        closeModal();
        resolve(action);
      });
    });
  });
}
