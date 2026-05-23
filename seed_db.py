#!/usr/bin/env python3
import sqlite3
from pathlib import Path
from datetime import datetime
import sys

ROOT = Path(__file__).resolve().parent
DB_PATH = ROOT / "hps.db"
CORNICE_CSV = ROOT / "assets" / "cornice_rate.csv"
STOCK_CSV = ROOT / "assets" / "overall_stock.csv"


def parse_csv(content: str):
    rows = []
    row = []
    cell = []
    i = 0
    n = len(content)
    in_quotes = False
    while i < n:
        ch = content[i]
        peek = content[i+1] if i+1 < n else None
        if ch == '"' and in_quotes and peek == '"':
            cell.append('"')
            i += 1
        elif ch == '"':
            in_quotes = not in_quotes
        elif ch == ',' and not in_quotes:
            row.append(''.join(cell).strip())
            cell = []
        elif ch == '\n' and not in_quotes:
            row.append(''.join(cell).rstrip('\r').strip())
            cell = []
            if any(v != '' for v in row):
                rows.append(row)
            row = []
        else:
            cell.append(ch)
        i += 1

    if cell or row:
        row.append(''.join(cell).rstrip('\r').strip())
        if any(v != '' for v in row):
            rows.append(row)
    return rows


def clean_cell(value: str) -> str:
    return value.strip().strip('"').strip()


def clean_series(value: str) -> str:
    return clean_cell(value).replace('\u201c', '').replace('\u201d', '')


def first_number(value: str):
    started = False
    number = []
    for ch in value:
        if ch.isascii() and ch.isdigit() or (ch == '.' and started):
            started = True
            number.append(ch)
        elif started:
            break
    if not number:
        return None
    try:
        return float(''.join(number))
    except Exception:
        return None


def now_string():
    return datetime.now().strftime('%Y-%m-%dT%H:%M:%S')


def seed_cornice_rates(conn):
    content = CORNICE_CSV.read_text(encoding='utf-8')
    rows = parse_csv(content)
    if not rows:
        print('No cornice rows found')
        return 0
    headers = rows[0]
    now = now_string()
    cur = conn.cursor()
    inserted = 0
    for row in rows[1:]:
        index = 0
        while index + 1 < len(headers):
            series = clean_series(headers[index] if index < len(headers) else '')
            model = clean_cell(row[index]) if index < len(row) else ''
            unit_text = row[index+1] if index+1 < len(row) else ''
            if series and model:
                unit_value = first_number(unit_text)
                cur.execute(
                    "INSERT OR IGNORE INTO cornice_rates (series, model, unit_text, unit_value, is_confidential, updated_at) VALUES (?, ?, ?, ?, 1, ?)",
                    (series, model, unit_text, unit_value, now),
                )
                if cur.rowcount:
                    inserted += 1
            index += 2
    conn.commit()
    return inserted


def seed_stock_items(conn):
    content = STOCK_CSV.read_text(encoding='utf-8')
    rows = parse_csv(content)
    if len(rows) <= 1:
        print('No stock rows found')
        return 0
    now = now_string()
    cur = conn.cursor()
    inserted = 0
    for row in rows[1:]:
        model = clean_cell(row[0]) if len(row) > 0 else ''
        if not model:
            continue
        stock = 0
        if len(row) > 1:
            try:
                stock = int(clean_cell(row[1]))
            except Exception:
                stock = 0
        location = clean_cell(row[2]) if len(row) > 2 else ''
        cur.execute(
            "INSERT OR IGNORE INTO stock_items (item_type, model, stock, location, updated_at) VALUES ('cornice', ?, ?, ?, ?)",
            (model, stock, location, now),
        )
        if cur.rowcount:
            inserted += 1
    conn.commit()
    return inserted


def main():
    if not DB_PATH.exists():
        print('Database not found at', DB_PATH)
        sys.exit(1)
    conn = sqlite3.connect(str(DB_PATH))
    # enable foreign keys
    conn.execute('PRAGMA foreign_keys = ON')
    conn.execute('PRAGMA journal_mode = WAL')

    cornice_added = seed_cornice_rates(conn)
    stock_added = seed_stock_items(conn)

    # mark seeded
    conn.execute("INSERT OR REPLACE INTO app_meta (key, value) VALUES ('seed_version', '1')")
    conn.commit()
    print(f'Inserted {cornice_added} cornice_rates and {stock_added} stock_items')

if __name__ == '__main__':
    main()
