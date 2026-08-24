function makeEl() {
  return {
    innerHTML: "",
    textContent: "",
    value: "",
    style: {},
    dataset: {},
    classList: { add() {}, remove() {}, toggle() {}, contains() { return false; } },
    addEventListener() {},
    removeEventListener() {},
    querySelector() { return makeEl(); },
    querySelectorAll() { return []; },
    appendChild() {},
    remove() {},
    focus() {},
    blur() {},
    getBoundingClientRect() { return { top: 0, left: 0, width: 0, height: 0 }; },
    closest() { return null; },
    contains() { return false; },
  };
}

globalThis.window = {
  __TAURI__: {
    core: {
      invoke: async (cmd) => {
        if (cmd === "app_status") return { staff: [], admin: null, logo_data_url: "" };
        return {};
      },
    },
    event: null,
  },
  addEventListener() {},
  removeEventListener() {},
  location: { href: "" },
};
globalThis.document = {
  getElementById() { return makeEl(); },
  querySelector() { return makeEl(); },
  querySelectorAll() { return []; },
  createElement() { return makeEl(); },
  addEventListener() {},
  removeEventListener() {},
  body: makeEl(),
  documentElement: makeEl(),
};
globalThis.addEventListener = () => {};
globalThis.removeEventListener = () => {};
