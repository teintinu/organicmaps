# Vehicle catalog data

`vehicles-br-2026.sample.json` is a 30-row sample of the Inmetro **PBEV** 2026 catalog, committed so
the schema and the tooling are visible and testable.

The **full** catalog (939 unique versions, 181 electric) is generated locally:

```bash
python3 tools/ev/pbev_import.py --cells tools/ev/.cache/rows.jsonl \
    --year 2026 --cycle 18 --out data/ev/vehicles-br-2026.json
```

It is git-ignored on purpose: Inmetro publishes the table without an explicit reuse licence, so
redistribution terms are still open (decision **O1** in `plans/ev/DECISIONS.md`). Once confirmed,
committing the full file is a one-line change plus removing the ignore rule.

Attribution when the data is shown: **INMETRO / Conpet, PBEV 2026**.
