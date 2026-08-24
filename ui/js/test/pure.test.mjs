import "./setup.mjs";
import { test } from "node:test";
import assert from "node:assert/strict";

import { escapeHtml, weekStartIso } from "../api.js";
import {
  unitValue,
  shiftIso,
  parsePrevValues,
  dbInputValue,
  dbDisplay,
  fmtBytes,
  cellLooksHtml,
  emptyDbValues,
} from "../core.js";
import { matchesQuery } from "../search.js";

test("escapeHtml escapes all five HTML metacharacters", () => {
  assert.equal(escapeHtml(`<a href="x">&'</a>`), "&lt;a href=&quot;x&quot;&gt;&amp;&#039;&lt;/a&gt;");
  assert.equal(escapeHtml(null), "");
  assert.equal(escapeHtml(42), "42");
});

test("weekStartIso returns the Wednesday that starts the current week", () => {
  // 2026-08-24 is a Monday -> week started Wednesday 2026-08-19.
  assert.equal(weekStartIso(new Date(2026, 7, 24)), "2026-08-19");
  // Wednesday itself maps to itself.
  assert.equal(weekStartIso(new Date(2026, 7, 19)), "2026-08-19");
  // Tuesday before the Wednesday maps back to the prior Wednesday.
  assert.equal(weekStartIso(new Date(2026, 7, 18)), "2026-08-12");
});

test("unitValue mirrors db::unit_value", () => {
  assert.equal(unitValue(""), 0);
  assert.equal(unitValue(null), 0);
  assert.equal(unitValue("unknown"), 0);
  assert.equal(unitValue("UNKNOWN"), 0);
  assert.equal(unitValue("1/2"), 0.5);
  // Zero denominator falls back to the leading number, same as db::unit_value.
  assert.equal(unitValue("1/0"), 1);
  assert.equal(unitValue("2"), 2);
  assert.equal(unitValue("1.5m"), 1.5);
  assert.equal(unitValue("abc"), 0);
});

test("shiftIso adds days across month boundaries", () => {
  assert.equal(shiftIso("2026-01-01", 1), "2026-01-02");
  assert.equal(shiftIso("2026-01-31", 1), "2026-02-01");
  assert.equal(shiftIso("2026-03-01", -1), "2026-02-28");
  assert.equal(shiftIso("2024-02-28", 1), "2024-02-29");
  assert.equal(shiftIso("2026-12-31", 1), "2027-01-01");
});

test("parsePrevValues handles valid, invalid and missing JSON", () => {
  assert.deepEqual(parsePrevValues({ prev_values: '{"model":"A"}' }), { model: "A" });
  assert.equal(parsePrevValues({ prev_values: "not-json" }), null);
  assert.equal(parsePrevValues({ prev_values: '"just a string"' }), null);
  assert.equal(parsePrevValues({}), null);
});

test("dbInputValue normalises null/boolean/number", () => {
  assert.equal(dbInputValue(null), "");
  assert.equal(dbInputValue(undefined), "");
  assert.equal(dbInputValue(true), "1");
  assert.equal(dbInputValue(false), "0");
  assert.equal(dbInputValue(7), "7");
});

test("dbDisplay renders booleans and timestamp columns", () => {
  assert.equal(dbDisplay("active", true), "Yes");
  assert.equal(dbDisplay("active", false), "No");
  assert.equal(dbDisplay("active", null), "");
  assert.equal(dbDisplay("created_at", "2026-08-24T10:00:00"), "2026-08-24 10:00:00");
  assert.equal(dbDisplay("name", "2026-08-24T10:00:00"), "2026-08-24T10:00:00");
});

test("fmtBytes formats binary sizes", () => {
  assert.equal(fmtBytes(0), "0 B");
  assert.equal(fmtBytes(512), "512 B");
  assert.equal(fmtBytes(1024), "1.0 KB");
  assert.equal(fmtBytes(1536), "1.5 KB");
  assert.equal(fmtBytes(5 * 1024 * 1024), "5.0 MB");
  assert.equal(fmtBytes(150 * 1024 * 1024), "150 MB");
});

test("cellLooksHtml only passes through known raw-HTML cells", () => {
  assert.equal(cellLooksHtml('<button class="x">go</button>'), true);
  assert.equal(cellLooksHtml('<span class="tag warn">w</span>'), true);
  assert.equal(cellLooksHtml('<span class="old-new">a</span>'), true);
  assert.equal(cellLooksHtml('<span class="amended-model">a</span>'), true);
  assert.equal(cellLooksHtml("<script>alert(1)</script>"), false);
  assert.equal(cellLooksHtml("plain text"), false);
  assert.equal(cellLooksHtml(null), false);
});

test("emptyDbValues seeds defaults by column kind", () => {
  const values = emptyDbValues([
    { name: "active", kind: "bool" },
    { name: "qty", kind: "integer" },
    { name: "price", kind: "real" },
    { name: "name", kind: "text" },
  ]);
  assert.deepEqual(values, { active: false, qty: null, price: null, name: "" });
});

test("matchesQuery is a case-insensitive substring match", () => {
  assert.equal(matchesQuery("Cornice A", "corn"), true);
  assert.equal(matchesQuery("Cornice A", "B"), false);
  assert.equal(matchesQuery(null, "x"), false);
  assert.equal(matchesQuery("anything", ""), true);
});
