const tauriCore = typeof window !== "undefined" && window.__TAURI__ && window.__TAURI__.core;
const tauriInvoke = tauriCore ? tauriCore.invoke : null;
const tauriEvent = typeof window !== "undefined" && window.__TAURI__ && window.__TAURI__.event;

export async function invoke(command, args = {}) {
  if (!tauriInvoke) {
    throw new Error("Tauri runtime is not available.");
  }
  return tauriInvoke(command, args);
}

export async function listen(event, handler) {
  if (!tauriEvent?.listen) return () => {};
  return tauriEvent.listen(event, handler);
}

export function escapeHtml(value) {
  const safeValue = value === null || value === undefined ? "" : value;
  return String(safeValue)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

export function todayIso() {
  const now = new Date();
  const yyyy = now.getFullYear();
  const mm = String(now.getMonth() + 1).padStart(2, "0");
  const dd = String(now.getDate()).padStart(2, "0");
  return `${yyyy}-${mm}-${dd}`;
}

export function weekStartIso(date = new Date()) {
  const copy = new Date(date);
  const day = (copy.getDay() + 6) % 7;
  const wednesday = 2;
  const delta = (day + 7 - wednesday) % 7;
  copy.setDate(copy.getDate() - delta);
  const yyyy = copy.getFullYear();
  const mm = String(copy.getMonth() + 1).padStart(2, "0");
  const dd = String(copy.getDate()).padStart(2, "0");
  return `${yyyy}-${mm}-${dd}`;
}

export function formatAction(action) {
  return action === "clock_in" ? "Clock in" : "Clock out";
}

export function setBusy(button, busy = true) {
  if (!button) return;
  button.disabled = busy;
}
