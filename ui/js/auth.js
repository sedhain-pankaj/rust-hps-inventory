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
    let scanning = false;

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
              <svg xmlns="http://www.w3.org/2000/svg" width="80" height="80" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="M2 12C2 6.5 6.5 2 12 2a10 10 0 0 1 8 4"/>
                <path d="M5 19.5C5.5 18 6 15 6 12c0-3.5 2.5-6 6-6"/>
                <path d="M10 19c-.5-1-1-3.5-1-7"/>
                <path d="M16 17c.8-1 1.5-3 2-5.5"/>
                <path d="M14 17.5c.2-1 .5-2.5.5-4.5s-.3-3.5-.5-4.5"/>
                <path d="M8 12c0-1 .2-2 .5-2.5"/>
                <path d="M19 10.5c.3-.5.5-1 .5-1.5s-.2-1-.5-1.5"/>
              </svg>
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
      if (scanning) return;
      scanning = true;
      fpIcon.classList.add("scanning");
      message.textContent = "";
      message.classList.remove("error");
      try {
        const response = await invoke("authenticate_fingerprint", { requireAdmin });
        closeModal();
        resolve(response);
      } catch (error) {
        fpFailures++;
        if (fpFailures >= maxFpFailures) {
          showPasswordFallback();
          fpStatus.textContent = `${fpFailures}/5 tries failed — try again.`;
        } else {
          fpStatus.textContent = `${fpFailures}/5 tries failed — try again.`;
          fpStatus.style.color = "#c55";
        }
        scanning = false;
        setTimeout(() => doFingerprintScan(), 600);
      }
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
