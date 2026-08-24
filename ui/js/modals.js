import { escapeHtml, invoke } from "./api.js";
import { icon } from "./icons.js";

export const modalRoot = document.getElementById("modal-root");

export function wait(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export function closeModal() {
  modalRoot.innerHTML = "";
  invoke("kill_fingerprint_helpers").catch(() => {});
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
  fpIcon = true,
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
            ${fpIcon ? `
              <div class="auth-fp-icon confirm-fp-icon">
                <img src="./assets/noun-fingerprint-1377758.svg" alt="Fingerprint" width="56" height="56" />
              </div>` : ""}
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
