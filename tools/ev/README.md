# Vehicle catalog tools (Brazil / PBEV)

Builds `data/ev/vehicles-br-2026.json` from the official Inmetro **PBEV** table. The table is
published only as a PDF, so the pipeline is: extract rows → normalize → validate.

```
PBEV PDF ──pdf_cells.swift──▶ rows.jsonl ──pbev_import.py──▶ vehicles-br-2026.json ──validate_catalog.py──▶ OK
           (macOS, PDFKit)                 (pure Python)                              (sanity checks)
```

## Run it

```bash
# 1. download the current table (18th cycle, updated 2026-08-14)
curl -sSL -o tools/ev/.cache/pbev-2026.pdf \
  "https://www.gov.br/inmetro/pt-br/assuntos/regulamentacao/avaliacao-da-conformidade/\
programa-brasileiro-de-etiquetagem/tabelas-de-eficiencia-energetica/\
veiculos-automotivos-pbe-veicular/mascara-pbev-2026_19_jan-rev01.pdf/@@download/file"

# 2. extract one JSON line per vehicle row (macOS; PDFKit)
swift tools/ev/pdf_cells.swift tools/ev/.cache/pbev-2026.pdf > tools/ev/.cache/rows.jsonl

# 3. normalize
python3 tools/ev/pbev_import.py --cells tools/ev/.cache/rows.jsonl \
    --year 2026 --cycle 18 --out data/ev/vehicles-br-2026.json

# 4. validate
python3 tools/ev/validate_catalog.py data/ev/vehicles-br-2026.json
```

Cross-platform alternative for step 2: the importer also accepts `--pdf`, using `pdfminer.six`
(`pip install -r tools/ev/requirements.txt`). It is much slower on this watermark-heavy PDF; the
`pdf_cells.swift` path exists precisely because of that. On Linux, `pdftotext -bbox` output can be
adapted into the same rows JSONL.

## Output

```json
{
  "schemaVersion": 1,
  "catalogVersion": "2026.18.1",
  "source": { "name": "INMETRO PBEV", "year": 2026, "cycle": 18, "license": "unknown" },
  "counts": { "vehicles": 939, "electric": 181 },
  "vehicles": [{
    "id": "br-byd-dolphin-mini-gs-ev-eletrico-n-a",
    "market": "BR",
    "category": "Sub Compacto",
    "make": "BYD", "model": "DOLPHIN MINI", "version": "GS EV",
    "engine": "Elétrico", "fuelType": "Elétrico", "transmission": "N.A.",
    "consumptionMjPerKm": 0.41,
    "consumptionWhPerKm": 113.9,
    "rangeKm": 280,
    "energyRating": "A", "ratingCategory": "A", "ratingGeneral": "A"
  }]
}
```

`consumptionMjPerKm` is the official number; `consumptionWhPerKm` is derived for electric vehicles
(`MJ/km ÷ 3.6 × 1000`) because the planner works in Wh/km. Battery capacity is **not** in PBEV — the
app asks the user or uses a learned value.

## Coverage and known gaps

The 2026 table announces 976 versions; the extractor finds 968 row candidates and the catalog keeps
**939 unique** rows (**181 electric**). The gap is expected and tracked:

* a handful of rows have the category cell merged with its neighbour and are skipped;
* repeated rows (same make/model/version/engine/transmission) collapse to one id;
* PHEV `rangeKm` is the **electric-only** range (often 10–60 km), not total range.

## Licence

The Inmetro page publishes the table without an explicit reuse licence (open question **O1** in the
EV plan). Until Inmetro confirms, treat this catalog as *curated factual data with attribution*
(`INMETRO / Conpet, PBEV 2026`), not as a wholesale redistribution of the PDF. The PDF itself is
cached under `tools/ev/.cache/` and must never be committed.

## Tests

```bash
python3 -m unittest discover -s tools/ev/tests
```
