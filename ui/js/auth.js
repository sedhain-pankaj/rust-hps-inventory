import { escapeHtml, invoke, setBusy } from "./api.js";

const modalRoot = document.getElementById("modal-root");

export function closeModal() {
  modalRoot.innerHTML = "";
}

export function requestAuth({ title, requireAdmin = false, employee = null }) {
  return new Promise((resolve, reject) => {
    const employeeLabel = employee ? `${employee.name} (${employee.id})` : "Admin";
    modalRoot.innerHTML = `
      <div class="modal-backdrop">
        <section class="modal" role="dialog" aria-modal="true">
          <header>
            <h2>${escapeHtml(title)}</h2>
            <button class="icon ghost" data-close title="Close">X</button>
          </header>
          <div class="body">
            <div class="message">${escapeHtml(employeeLabel)}</div>
            <label>
              Password
              <input data-password type="password" autocomplete="current-password" />
            </label>
            <div class="message" data-message></div>
          </div>
          <footer>
            <button class="ghost" data-fingerprint>Fingerprint</button>
            <button class="primary" data-password-submit>Continue</button>
          </footer>
        </section>
      </div>
    `;

    const passwordInput = modalRoot.querySelector("[data-password]");
    const message = modalRoot.querySelector("[data-message]");
    const closeButton = modalRoot.querySelector("[data-close]");
    const passwordButton = modalRoot.querySelector("[data-password-submit]");
    const fingerprintButton = modalRoot.querySelector("[data-fingerprint]");

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
      message.textContent = "Scanning...";
      message.classList.remove("error");
      try {
        const response = await invoke("authenticate_fingerprint", { requireAdmin });
        closeModal();
        resolve(response);
      } catch (error) {
        fail(error);
      } finally {
        setBusy(fingerprintButton, false);
      }
    });

    passwordInput.addEventListener("keydown", (event) => {
      if (event.key === "Enter") passwordButton.click();
    });
    passwordInput.focus();
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
