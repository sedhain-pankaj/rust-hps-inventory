import { escapeHtml, invoke, setBusy } from "./api.js";

const modalRoot = document.getElementById("modal-root");

export function closeModal() {
  modalRoot.innerHTML = "";
}

export function requestAuth({ title, requireAdmin = false, employee = null }) {
  return new Promise((resolve, reject) => {
    const employeeLabel = employee ? `${employee.name} (${employee.id})` : "Admin";
    let fpFailures = 0;
    const maxFpFailures = 5;
    let passwordVisible = false;

    modalRoot.innerHTML = `
      <div class="modal-backdrop">
        <section class="modal" role="dialog" aria-modal="true">
          <header>
            <h2>${escapeHtml(title)}</h2>
            <button class="icon ghost" data-close title="Close">X</button>
          </header>
          <div class="body">
            <div class="message">${escapeHtml(employeeLabel)}</div>
            <div id="auth-fp-status" class="message" style="margin-top:0.5em;font-size:0.9em;">Scan your fingerprint to authenticate.</div>
            <label id="auth-password-label" style="display:none">
              Password
              <input data-password type="password" autocomplete="current-password" placeholder="Enter password…" />
            </label>
            <div class="message" data-message></div>
          </div>
          <footer>
            <button class="ghost" data-fingerprint>Fingerprint</button>
            <button class="primary" data-password-submit style="display:none">Continue</button>
          </footer>
        </section>
      </div>
    `;

    const passwordLabel = modalRoot.querySelector("#auth-password-label");
    const passwordInput = modalRoot.querySelector("[data-password]");
    const fpStatus = modalRoot.querySelector("#auth-fp-status");
    const message = modalRoot.querySelector("[data-message]");
    const closeButton = modalRoot.querySelector("[data-close]");
    const passwordButton = modalRoot.querySelector("[data-password-submit]");
    const fingerprintButton = modalRoot.querySelector("[data-fingerprint]");

    const showPasswordFallback = () => {
      if (passwordVisible) return;
      passwordVisible = true;
      passwordLabel.style.display = "";
      passwordButton.style.display = "";
      fpStatus.textContent = "Fingerprint failed. Enter your password below, or retry fingerprint.";
      fpStatus.style.color = "#c55";
      passwordInput.focus();
    };

    const fail = (error) => {
      message.textContent = (error && error.message) || String(error);
      message.classList.add("error");
    };

    closeButton.addEventListener("click", () => {
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
        closeModal();
        resolve(response);
      } catch (error) {
        fail(error);
      } finally {
        setBusy(passwordButton, false);
      }
    });

    fingerprintButton.addEventListener("click", async () => {
      setBusy(fingerprintButton);
      message.textContent = "Scanning…";
      message.classList.remove("error");
      try {
        const response = await invoke("authenticate_fingerprint", { requireAdmin });
        closeModal();
        resolve(response);
      } catch (error) {
        fpFailures++;
        fail(error);
        if (fpFailures >= maxFpFailures) {
          showPasswordFallback();
        } else {
          const remaining = maxFpFailures - fpFailures;
          fpStatus.textContent = `Scan failed (${fpFailures}/${maxFpFailures}). ${remaining} attempt${remaining !== 1 ? 's' : ''} remaining.`;
          fpStatus.style.color = "#c55";
        }
      } finally {
        setBusy(fingerprintButton, false);
      }
    });

    passwordInput.addEventListener("keydown", (event) => {
      if (event.key === "Enter") passwordButton.click();
    });
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
