import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import pbev_import as m


def cells_for(values):
    """Build fake (x_center, y, text) cells from {column_index: text}."""
    return [(m.COLUMN_CENTERS[i], 100.0, text) for i, text in values.items()]


class RowsTest(unittest.TestCase):
    def test_groups_cells_by_baseline(self):
        cells = [(185.0, 100.0, "Compacto"), (218.0, 100.4, "FIAT"), (185.0, 96.8, "SUV")]
        rows = m.group_rows(cells)
        self.assertEqual(len(rows), 2)
        self.assertEqual(len(rows[0][1]), 2)

    def test_assigns_nearest_column(self):
        row = m.to_columns([(m.COLUMN_CENTERS[1] + 1.5, 100.0, "BYD")])
        self.assertEqual(row[1], "BYD")
        self.assertIsNone(row[2])

    def test_ignores_far_outliers(self):
        row = m.to_columns([(760.0, 100.0, "junk")])
        self.assertTrue(all(v is None for v in row))


class NumbersTest(unittest.TestCase):
    def test_brazilian_decimal(self):
        self.assertAlmostEqual(m.parse_number("0,41"), 0.41)
        self.assertAlmostEqual(m.parse_number("1.234,5"), 1234.5)

    def test_placeholders(self):
        for placeholder in ["\\", "-", "ND", "N.A.", "", None]:
            self.assertIsNone(m.parse_number(placeholder))


class BuildTest(unittest.TestCase):
    def test_builds_electric_record(self):
        row = [None] * len(m.COLUMN_CENTERS)
        row[0] = "Sub Compacto"
        row[1] = "BYD"
        row[2] = "DOLPHIN MINI"
        row[3] = "GS EV"
        row[5] = "Elétrico"
        row[28] = "0,41"
        row[29] = "280"
        record = m.build_record(row, "BR")
        self.assertEqual(record["id"], "br-byd-dolphin-mini-gs-ev")
        self.assertAlmostEqual(record["consumptionMjPerKm"], 0.41)
        self.assertEqual(record["rangeKm"], 280.0)

    def test_rejects_row_without_make_or_model(self):
        row = [None] * len(m.COLUMN_CENTERS)
        row[2] = "ORPHAN"
        self.assertIsNone(m.build_record(row, "BR"))


if __name__ == "__main__":
    unittest.main()


class ImportRowsTest(unittest.TestCase):
    def _rows_file(self, rows):
        import json
        import tempfile

        handle = tempfile.NamedTemporaryFile("w", suffix=".jsonl", delete=False, encoding="utf-8")
        with handle:
            for cells in rows:
                handle.write(json.dumps({"p": 0, "y": 1.0, "cells": cells}) + "\n")
        self.addCleanup(Path(handle.name).unlink)
        return handle.name

    def test_reads_rows_and_derives_wh_per_km(self):
        cells = [""] * len(m.COLUMN_CENTERS)
        cells[1], cells[2], cells[3] = "BYD", "DOLPHIN MINI", "GS EV"
        cells[5] = "Elétrico"
        cells[28], cells[29] = "0,41", "280"
        vehicles, _pages = m.import_rows(self._rows_file([cells]))
        self.assertEqual(len(vehicles), 1)
        self.assertAlmostEqual(vehicles[0]["consumptionWhPerKm"], 113.9)

    def test_deduplicates_ids(self):
        cells = [""] * len(m.COLUMN_CENTERS)
        cells[1], cells[2] = "BYD", "DOLPHIN MINI"
        vehicles, _pages = m.import_rows(self._rows_file([cells, list(cells)]))
        self.assertEqual(len(vehicles), 1)


class ValidateCatalogTest(unittest.TestCase):
    def setUp(self):
        import validate_catalog

        self.v = validate_catalog

    def _payload(self, vehicles):
        return {"counts": {"vehicles": len(vehicles)}, "vehicles": vehicles}

    def test_accepts_valid_ev(self):
        payload = self._payload([{
            "id": "br-x", "make": "BYD", "model": "DOLPHIN", "fuelType": "Elétrico",
            "consumptionMjPerKm": 0.41, "rangeKm": 280,
        }])
        self.assertEqual(self.v.validate(payload), [])

    def test_flags_duplicate_and_missing_fields(self):
        payload = self._payload([
            {"id": "br-x", "make": "A", "model": "B", "fuelType": "Combustão"},
            {"id": "br-x", "make": "", "model": "B", "fuelType": "Combustão"},
        ])
        errors = self.v.validate(payload)
        self.assertTrue(any("duplicate" in e for e in errors))
        self.assertTrue(any("missing make/model" in e for e in errors))

    def test_flags_ev_without_range(self):
        payload = self._payload([{"id": "br-y", "make": "A", "model": "B",
                                  "fuelType": "Elétrico", "consumptionMjPerKm": 0.4}])
        self.assertTrue(any("without consumption/range" in e for e in self.v.validate(payload)))
