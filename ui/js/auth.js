import { escapeHtml, invoke, setBusy } from "./api.js";
import { icon } from "./icons.js";
import { closeModal, modalRoot, wait } from "./modals.js";

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

// HINT|<employee_id>|<ncc_pct>|<slide_x_mm>|<slide_y_mm>
// slide is signed: +x = slide right, +y = slide down.
function formatSlideHint(hint) {
  const parts = [];
  if (Math.abs(hint.slideX) >= 0.1)
    parts.push(
      `${Math.abs(hint.slideX).toFixed(1)}mm ${hint.slideX > 0 ? "RIGHT" : "LEFT"}`,
    );
  if (Math.abs(hint.slideY) >= 0.1)
    parts.push(
      `${Math.abs(hint.slideY).toFixed(1)}mm ${hint.slideY > 0 ? "DOWN" : "UP"}`,
    );
  if (!parts.length)
    return "Please reposition your finger on the reader and try again.";
  return `Please shift your finger ${parts.join(" and ")}`;
}

function formatMatchHint(best, hint, nameById) {
  if (!best) return "";
  const name = (nameById && nameById.get(best.id)) || best.id;
  const lines = [`Match: ${best.score}/40 for ${name}`];
  if (hint && hint.id === best.id) {
    if (hint.ncc < 25)
      lines.push("Place your finger firmly on the reader — no reliable alignment.");
    else lines.push(formatSlideHint(hint));
  }
  return lines.join("\n");
}

export async function requestAuth({ title, requireAdmin = false, employee = null }) {
  // id -> name map so the match hint can show the employee's name.
  const nameById = new Map();
  if (employee) nameById.set(employee.id, employee.name);
  try {
    const staff = await invoke("list_staff", { includeInactive: true });
    for (const entry of staff) nameById.set(entry.id, entry.name);
  } catch {
    // Fall back to IDs in the hint.
  }

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
      // After a failure, keep the failure message + hint visible while the
      // next scan runs (the animated ring shows the scan is active).
      if (fpFailures === 0) {
        message.textContent = "";
        message.classList.remove("error");
        fpStatus.className = "scan-status info";
        fpStatus.textContent = "Starting scan…";
      }
      try {
        const start = await invoke("start_fingerprint_auth", {
          requireAdmin,
          employeeId: employee ? employee.id : null,
        });
        let nextIndex = 0;
        let lastRetryReason = null;
        let lastBest = null;
        let lastHint = null;
        let jobError = null;
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
                if (fpFailures === 0) {
                  fpStatus.className = "scan-status info";
                  fpStatus.textContent = `Scan attempt ${scanAttempts}/${parts[2]}: waiting for finger…`;
                }
              } else if (raw.startsWith("RETRY|")) {
                lastRetryReason = raw.split("|").slice(1).join("|");
                if (fpFailures === 0) {
                  fpStatus.className = "scan-status warn";
                  fpStatus.textContent = `⚠ Attempt ${scanAttempts}: ${mapRetryReason(lastRetryReason)}`;
                }
              } else if (raw.startsWith("BEST|")) {
                const parts = raw.split("|");
                const score = parseInt(parts[2]);
                if (parts[1] && !Number.isNaN(score))
                  lastBest = { id: parts[1], score };
              } else if (raw.startsWith("HINT|")) {
                const parts = raw.split("|");
                const ncc = parseInt(parts[2]);
                const slideX = parseFloat(parts[3]);
                const slideY = parseFloat(parts[4]);
                if (parts[1] && !Number.isNaN(ncc))
                  lastHint = {
                    id: parts[1],
                    ncc,
                    slideX: Number.isNaN(slideX) ? 0 : slideX,
                    slideY: Number.isNaN(slideY) ? 0 : slideY,
                  };
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
            jobError = status.error || "Authentication failed.";
            console.log("[auth] fingerprint job failed:", jobError);
            break;
          }
        }

        // Job failed — increment failure counter and show reason
        fpFailures++;
        const displayReason = lastRetryReason
          ? mapRetryReason(lastRetryReason)
          : (jobError || fpStatus.textContent || "Scan failed");
        const matchHint = formatMatchHint(lastBest, lastHint, nameById);

        if (fpFailures >= maxFpFailures) {
          showPasswordFallback();
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${displayReason}`;
        } else {
          fpStatus.className = "scan-status err";
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${displayReason}`;
        }
        if (matchHint) {
          message.textContent = matchHint;
          message.style.whiteSpace = "pre-line";
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

// Guided enrollment modal: replaces the old terminal-style enrollment log.
// Shows a per-stage finger-placement prompt, a progress bar, and the
// good-scan / poor-scan feedback live. Resolves on success, rejects on
// cancel or when the user closes after a failure.
export function requestEnroll({ employee, finger }) {
  return new Promise((resolve, reject) => {
    let aborted = false;
    let jobId = null;
    let totalStages = 15;
    let completedStages = 0;
    let settled = false;

    const stagePrompt = (stage) => {
      if (stage <= 5) return "Place your finger in the CENTER of the reader";
      if (stage <= 8) return "Move your finger DOWN ~1-2mm (lower part of the finger)";
      if (stage <= 11) return "Move your finger UP ~1-2mm (upper part of the finger)";
      if (stage <= 13) return "Move your finger LEFT ~1-2mm";
      return "Move your finger RIGHT ~1-2mm";
    };

    // Which zone of the reader the current stage targets — drives the
    // fingerprint glyph offset so the icon shows where the finger goes.
    const stageOffset = (stage) => {
      if (stage <= 5) return "center";
      if (stage <= 8) return "down";
      if (stage <= 11) return "up";
      if (stage <= 13) return "left";
      return "right";
    };

    const OFFSETS = ["center", "down", "up", "left", "right"];

    const fingerLabel = String(finger).replace(/-/g, " ");
    modalRoot.innerHTML = `
      <div class="modal-backdrop">
        <section class="modal" role="dialog" aria-modal="true">
          <header>
            <h2>Fingerprint Enrollment</h2>
            <button class="icon ghost" data-close title="Close">${icon("x")}</button>
          </header>
          <div class="body">
            <div class="message">${escapeHtml(`${employee.name} (${employee.id}) — ${fingerLabel}`)}</div>
            <div class="auth-fp-icon scanning" id="enroll-fp-icon">
              <span class="ring"></span>
              <span class="ring r2"></span>
              <img src="./assets/noun-fingerprint-1377758.svg" alt="Fingerprint" width="72" height="72" />
            </div>
            <div class="enroll-prompt" id="enroll-prompt">${stagePrompt(1)}</div>
            <div class="enroll-progress">
              <div class="enroll-progress-fill" id="enroll-progress-fill"></div>
            </div>
            <div class="message" id="enroll-stage-label">Stage 1 of ${totalStages}</div>
            <div class="scan-status info" id="enroll-status">Starting enrollment…</div>
          </div>
          <footer>
            <button class="ghost" data-cancel>Cancel</button>
          </footer>
        </section>
      </div>
    `;

    const fpIcon = modalRoot.querySelector("#enroll-fp-icon");
    const promptEl = modalRoot.querySelector("#enroll-prompt");
    const fillEl = modalRoot.querySelector("#enroll-progress-fill");
    const stageLabel = modalRoot.querySelector("#enroll-stage-label");
    const statusEl = modalRoot.querySelector("#enroll-status");
    const cancelButton = modalRoot.querySelector("[data-cancel]");

    const renderProgress = () => {
      const pct = totalStages ? Math.round((completedStages / totalStages) * 100) : 0;
      fillEl.style.width = `${pct}%`;
      const done = completedStages >= totalStages;
      const stage = Math.min(completedStages + 1, totalStages);
      stageLabel.textContent = done
        ? `All ${totalStages} stages captured`
        : `Stage ${stage} of ${totalStages}`;
      promptEl.textContent = stagePrompt(stage);
      const offset = done ? "center" : stageOffset(stage);
      OFFSETS.forEach((o) => fpIcon.classList.toggle(`offset-${o}`, o === offset));
    };

    let failureText = null;

    const cancel = async () => {
      if (failureText) {
        closeModal();
        reject(new Error(failureText));
        return;
      }
      if (settled || aborted) return;
      aborted = true;
      if (jobId) {
        await invoke("cancel_fingerprint_enroll", { jobId }).catch(() => {});
      }
      settled = true;
      closeModal();
      reject(new Error("Enrollment cancelled."));
    };

    const finish = (errorText) => {
      if (settled) return;
      settled = true;
      failureText = errorText;
      statusEl.className = "scan-status err";
      statusEl.textContent = errorText;
      fpIcon.classList.remove("scanning");
      cancelButton.textContent = "Close";
    };

    const run = async () => {
      renderProgress();
      try {
        const start = await invoke("start_fingerprint_enroll", {
          employeeId: employee.id,
          finger,
        });
        jobId = start.job_id;
        if (aborted) {
          // Cancelled before the job id was known — cancel now that it is.
          await invoke("cancel_fingerprint_enroll", { jobId }).catch(() => {});
          return;
        }
        let nextIndex = 0;
        let lastQuality = null;

        while (true) {
          await wait(250);
          if (aborted || settled) return;

          const status = await invoke("poll_fingerprint_enroll", {
            jobId,
            fromIndex: nextIndex,
          });
          nextIndex = status.next_index ?? nextIndex;

          if (Array.isArray(status.lines) && status.lines.length) {
            for (const line of status.lines) {
              const raw = parseAuthLine(line);
              if (raw.startsWith("ENROLL_STAGES|")) {
                totalStages = parseInt(raw.split("|")[1]) || totalStages;
                renderProgress();
              } else if (raw.startsWith("PROGRESS|")) {
                const [, completed, total] = raw.split("|");
                totalStages = parseInt(total) || totalStages;
                completedStages = parseInt(completed) || completedStages;
                lastQuality = "good";
                statusEl.className = "scan-status ok";
                statusEl.textContent = `✓ Good scan — stage ${completedStages}/${totalStages} captured`;
                renderProgress();
              } else if (raw.startsWith("RETRY|")) {
                lastQuality = "retry";
                const reason = raw.split("|").slice(1).join("|");
                statusEl.className = "scan-status warn";
                statusEl.textContent = `⚠ ${mapRetryReason(reason)}`;
              } else if (raw.startsWith("READY|")) {
                if (!lastQuality) {
                  statusEl.className = "scan-status info";
                  statusEl.textContent = "Scanner ready — place your finger now";
                }
              } else if (raw.startsWith("ERROR|")) {
                console.log("[enroll] helper error:", raw.split("|").slice(1).join("|"));
              }
            }
          }

          if (status.state === "done") {
            if (aborted || settled) return;
            settled = true;
            completedStages = totalStages;
            renderProgress();
            statusEl.className = "scan-status ok";
            statusEl.textContent = "✓ Enrollment complete";
            fpIcon.classList.remove("scanning");
            setTimeout(() => {
              closeModal();
              resolve({ employeeId: employee.id, finger });
            }, 900);
            return;
          }
          if (status.state === "failed") {
            finish(status.error || "Enrollment failed.");
            return;
          }
        }
      } catch (error) {
        const reason =
          (typeof error === "string" ? error : error?.message) || "Enrollment failed.";
        console.log("[enroll] start failed:", reason);
        finish(reason);
      }
    };

    cancelButton.addEventListener("click", cancel);
    modalRoot.querySelector("[data-close]").addEventListener("click", cancel);

    setTimeout(() => run(), 300);
  });
}
