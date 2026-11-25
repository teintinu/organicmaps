#!/usr/bin/env python3
"""Import the Inmetro PBEV vehicle table (PDF) into a normalized JSON catalog.

Why a PDF parser: Inmetro publishes the PBEV table only as a PDF (no CSV/API).
The table is a fixed-layout grid, so we reconstruct columns from the x-center of
each text cell and rows from the y baseline.

Usage:
    python3 tools/ev/pbev_import.py \
        --pdf tools/ev/.cache/pbev-2026.pdf \
        --year 2026 --cycle 18 \
        --out data/ev/vehicles-br-2026.json

Requires: pdfminer.six (see tools/ev/requirements.txt).
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import sys
import unicodedata
from pathlib import Path


# Column centers (x0 + width/2) measured from the 2026/18th-cycle table.
# Order matches the printed grid. Keep in sync with COLUMNS below.
COLUMN_CENTERS = [
    185.0, 218.7, 249.7, 283.0, 305.0, 319.7, 336.5, 351.3, 366.2, 382.8,
    396.4, 409.7, 420.4, 431.1, 440.3, 448.3, 457.7, 467.7, 476.7, 486.6,
    499.3, 511.1, 520.9, 533.4, 546.4, 557.7, 568.6, 579.7, 593.6, 610.6,
    629.0, 648.9, 667.5,
]

# Only the columns we trust semantically; everything else is kept in `rawCells`.
COLUMNS = {
    0: "category",
    1: "make",
    2: "model",
    3: "version",
    4: "engine",
    5: "fuelType",
    6: "transmission",
    8: "steering",
    13: "energyRating",
    28: "consumptionMjPerKm",
    29: "rangeKm",
    30: "ratingCategory",
    31: "ratingGeneral",
    32: "notes",
}

# Columns whose values are numbers (Brazilian decimal comma).
NUMERIC = {"consumptionMjPerKm", "rangeKm"}

# Rows above/below this band are header/footer, not vehicles.
Y_MIN, Y_MAX = 25.0, 476.0


def iter_cells(page):
    """Yield (x_center, y_baseline, text) for real table cells.

    Vertical watermarks ("www.inmetro.gov.br") are 1 pt tall and sit at x<172;
    they are not cells.
    """
    from pdfminer.layout import LTContainer, LTTextLine

    def walk(el):
        if isinstance(el, LTTextLine):
            yield el
        elif isinstance(el, LTContainer):
            for child in el:
                yield from walk(child)

    for line in walk(page):
        text = line.get_text().strip()
        if not text or line.x0 <= 172 or line.height < 1.2:
            continue
        yield line.x0 + line.width / 2.0, line.y0, text


def iter_rows_jsonl(rows_path):
    """Yield one cell list per vehicle row, from `pdf_cells.swift` output."""
    with open(rows_path, encoding="utf-8") as handle:
        for raw in handle:
            row = json.loads(raw)
            cells = [cell.strip() or None for cell in row["cells"]]
            yield cells


def group_rows(cells, tolerance=1.2):
    """Group cells by baseline; returns rows sorted top-to-bottom."""
    rows = []
    for cell in sorted(cells, key=lambda c: -c[1]):
        if rows and abs(rows[-1][0] - cell[1]) <= tolerance:
            rows[-1][1].append(cell)
        else:
            rows.append([cell[1], [cell]])
    return rows


def to_columns(cells):
    """Assign cells to the nearest column center."""
    row = [None] * len(COLUMN_CENTERS)
    for cx, _y, text in cells:
        idx = min(range(len(COLUMN_CENTERS)), key=lambda i: abs(COLUMN_CENTERS[i] - cx))
        if abs(COLUMN_CENTERS[idx] - cx) > 7.0:
            continue  # e.g. a stray annotation
        row[idx] = text
    return row


def parse_number(text):
    if text is None or text in {"", "\\", "-", "ND", "N.A."}:
        return None
    cleaned = text.replace(".", "").replace(",", ".").strip()
    try:
        return float(cleaned)
    except ValueError:
        return None


def slug(*parts):
    text = "-".join(p for p in parts if p)
    # Fold accents so ids stay ASCII (pt-BR names are full of them).
    text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode("ascii")
    text = text.lower()
    text = re.sub(r"[^a-z0-9]+", "-", text)
    return re.sub(r"-{2,}", "-", text).strip("-")


def build_record(row, market):
    record = {"id": "", "market": market}
    for idx, name in COLUMNS.items():
        value = row[idx]
        record[name] = parse_number(value) if name in NUMERIC else (value or None)

    if not record.get("make") or not record.get("model"):
        return None
    if (record.get("fuelType") or "").startswith("Elétr") and record.get("consumptionMjPerKm"):
        # PBEV reports MJ/km; the planner works in Wh/km (1 kWh = 3.6 MJ).
        record["consumptionWhPerKm"] = round(record["consumptionMjPerKm"] / 3.6 * 1000.0, 1)
    record["id"] = f"{market.lower()}-" + slug(
        record["make"], record["model"], record.get("version"),
        record.get("engine"), record.get("transmission"))
    return record


def _records_from_cells(cells, market):
    vehicles = []
    seen = set()
    for _y, row_cells in group_rows([c for c in cells if Y_MIN < c[1] < Y_MAX]):
        record = build_record(to_columns(row_cells), market)
        if record is None or record["id"] in seen:
            continue
        seen.add(record["id"])
        vehicles.append(record)
    return vehicles


def import_rows(rows_path, market="BR"):
    """Fast path: rows already split into cells by pdf_cells.swift."""
    vehicles = []
    seen = set()
    for cells in iter_rows_jsonl(rows_path):
        record = build_record(cells, market)
        if record is None or record["id"] in seen:
            continue
        seen.add(record["id"])
        vehicles.append(record)
    return vehicles, 0


def import_pdf(pdf_path, year, cycle, market="BR"):
    """Slow, cross-platform path: pdfminer layout analysis."""
    from pdfminer.high_level import extract_pages

    vehicles = []
    pages = 0
    for page in extract_pages(str(pdf_path)):
        pages += 1
        for _y, row_cells in group_rows([c for c in iter_cells(page) if Y_MIN < c[1] < Y_MAX]):
            record = build_record(to_columns(row_cells), market)
            if record and record["id"] not in {v["id"] for v in vehicles}:
                vehicles.append(record)
    return vehicles, pages


def main(argv=None):
    parser = argparse.ArgumentParser(description="Import the Inmetro PBEV PDF table")
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--pdf", type=Path, help="PDF (slow pdfminer path)")
    source.add_argument("--cells", type=Path, help="rows JSONL from tools/ev/pdf_cells.swift (fast)")
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--year", type=int, required=True)
    parser.add_argument("--cycle", type=int, required=True)
    parser.add_argument("--market", default="BR")
    parser.add_argument("--source-url", default=(
        "https://www.gov.br/inmetro/pt-br/assuntos/regulamentacao/avaliacao-da-conformidade/"
        "programa-brasileiro-de-etiquetagem/tabelas-de-eficiencia-energetica/"
        "veiculos-automotivos-pbe-veicular"))
    args = parser.parse_args(argv)

    if args.cells is not None:
        if not args.cells.exists():
            parser.error(f"cells file not found: {args.cells}")
        vehicles, pages = import_rows(args.cells, args.market)
    else:
        if not args.pdf.exists() or args.pdf is None:
            parser.error(f"PDF not found: {args.pdf}")
        vehicles, pages = import_pdf(args.pdf, args.year, args.cycle, args.market)
    evs = [v for v in vehicles if (v.get("fuelType") or "").lower().startswith("elétr")]
    payload = {
        "schemaVersion": 1,
        "catalogVersion": f"{args.year}.{args.cycle}.1",
        "generatedAt": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
        "source": {
            "name": "INMETRO PBEV",
            "year": args.year,
            "cycle": args.cycle,
            "pages": pages,
            "url": args.source_url,
            # Inmetro publishes no explicit licence; see docs/EV plan, open question O1.
            "license": "unknown",
        },
        "counts": {"vehicles": len(vehicles), "electric": len(evs)},
        "vehicles": vehicles,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"{len(vehicles)} vehicles ({len(evs)} electric) from {pages} pages -> {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
