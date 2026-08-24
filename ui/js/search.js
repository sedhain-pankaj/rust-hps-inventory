import { escapeHtml } from "./api.js";
import { icon } from "./icons.js";

// One shared outside-click handler for all mounted boxes (boxes are re-mounted
// on every panel render, so per-box document listeners would leak).
const liveBoxes = new Set();
document.addEventListener("click", (event) => {
  for (const box of [...liveBoxes]) {
    if (!box.el.isConnected) {
      liveBoxes.delete(box);
      continue;
    }
    if (!box.el.contains(event.target)) box.close();
  }
});

// Reusable debounced search box with a keyboard-navigable results dropdown.
//
// config:
//   placeholder  - input placeholder text
//   minChars     - query length that triggers a search (default 2, 0 = always)
//   debounceMs   - input debounce (default 200)
//   searchFn(query) -> Promise<match[]>  (or sync array)
//   renderMatch(match, index) -> inner HTML of a result row (default: String(match))
//   onSelect(match) - called on pick (row click or Enter)
//   emptyText    - shown when a search returns nothing (default "No matches")
//   clearOnSelect - clear the input after a pick (default true)
//   autoFocus    - focus the input on mount (default false)
//   onQuery(query) - called on every debounced query (trimmed, "" when cleared);
//                    use for live filtering of the surrounding panel
//
// Returns { el, input, setQuery, close, destroy }.
export function mountSearchBox(rootEl, config) {
  const {
    placeholder = "Search…",
    minChars = 2,
    debounceMs = 200,
    searchFn,
    renderMatch = (match) => escapeHtml(String(match)),
    onSelect = () => {},
    emptyText = "No matches",
    clearOnSelect = true,
    autoFocus = false,
    onQuery = null,
  } = config;

  const el = document.createElement("div");
  el.className = "search-box";
  el.innerHTML = `
    <div class="search-input-wrap">
      <span class="search-input-icon">${icon("search", 16)}</span>
      <input type="text" class="search-input" placeholder="${escapeHtml(placeholder)}" autocomplete="off" spellcheck="false" />
      <button type="button" class="icon ghost search-clear" title="Clear" hidden>${icon("x", 14)}</button>
    </div>
    <div class="search-results" hidden></div>
  `;
  rootEl.appendChild(el);

  const input = el.querySelector(".search-input");
  const clearBtn = el.querySelector(".search-clear");
  const results = el.querySelector(".search-results");

  let matches = [];
  let activeIndex = -1;
  let timer = null;
  let destroyed = false;

  function close() {
    results.hidden = true;
    results.innerHTML = "";
    matches = [];
    activeIndex = -1;
  }

  function renderResults() {
    if (!matches.length) {
      results.innerHTML = `<div class="search-result empty">${escapeHtml(emptyText)}</div>`;
    } else {
      results.innerHTML = matches
        .map(
          (match, index) =>
            `<button type="button" class="search-result${index === activeIndex ? " active" : ""}" data-idx="${index}">${renderMatch(match, index)}</button>`,
        )
        .join("");
    }
    results.hidden = false;
  }

  function select(index) {
    const match = matches[index];
    if (match === undefined) return;
    close();
    if (clearOnSelect) input.value = "";
    clearBtn.hidden = true;
    onSelect(match);
  }

  function runSearch(query) {
    if (destroyed) return;
    const q = query.trim();
    if (onQuery) onQuery(q);
    if (q.length < minChars) {
      close();
      return;
    }
    Promise.resolve(searchFn(q))
      .then((found) => {
        if (destroyed) return;
        matches = found || [];
        activeIndex = matches.length ? 0 : -1;
        renderResults();
      })
      .catch(() => {
        /* ignore search failures */
      });
  }

  const box = {
    el,
    input,
    close,
    // trigger=false restores a query without re-running search/onQuery
    // (used when re-mounting a box that already drives a filtered panel).
    setQuery(value, { trigger = true } = {}) {
      input.value = value;
      clearBtn.hidden = !value;
      if (trigger) {
        runSearch(value);
      } else {
        close();
      }
    },
    destroy() {
      destroyed = true;
      clearTimeout(timer);
      liveBoxes.delete(box);
      el.remove();
    },
  };
  liveBoxes.add(box);

  input.addEventListener("input", () => {
    clearBtn.hidden = !input.value;
    clearTimeout(timer);
    timer = setTimeout(() => runSearch(input.value), debounceMs);
  });

  input.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown" && matches.length) {
      event.preventDefault();
      activeIndex = (activeIndex + 1) % matches.length;
      renderResults();
    } else if (event.key === "ArrowUp" && matches.length) {
      event.preventDefault();
      activeIndex = (activeIndex - 1 + matches.length) % matches.length;
      renderResults();
    } else if (event.key === "Enter") {
      event.preventDefault();
      select(activeIndex >= 0 ? activeIndex : 0);
    } else if (event.key === "Escape") {
      close();
      if (onQuery) onQuery("");
    }
  });

  results.addEventListener("click", (event) => {
    const row = event.target.closest(".search-result");
    if (!row || row.classList.contains("empty")) return;
    select(Number(row.dataset.idx));
  });

  clearBtn.addEventListener("click", () => {
    input.value = "";
    clearBtn.hidden = true;
    close();
    if (onQuery) onQuery("");
    input.focus();
  });

  if (autoFocus) input.focus();

  return box;
}

// Case-insensitive substring match helper for client-side filtering.
export function matchesQuery(value, query) {
  return String(value ?? "").toLowerCase().includes(String(query ?? "").toLowerCase());
}
