#!/usr/bin/env python3
"""Sanity checks for a generated vehicle catalog.

Run after pbev_import.py; exits non-zero when a hard invariant breaks.

    python3 tools/ev/validate_catalog.py data/ev/vehicles-br-2026.json
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

MJ_PER_KWH = 3.6          # 1 kWh = 3.6 MJ

# The table reports MJ/km for every powertrain, so the plausible band depends on it:
# a BEV uses ~0.15-0.35 MJ/km, a petrol car ~2-4 MJ/km (energy content, not wheel work).
CONSUMPTION_BANDS = {
    "bev": (0.10, 1.30),      # MJ/km ~ 28-360 Wh/km (vans sit at the top)
    "phev": (0.10, 3.50),
    "other": (1.00, 8.00),    # petrol/diesel energy content per km
}
RANGE_BANDS = {"bev": (80.0, 1200.0), "phev": (5.0, 250.0)}


def wh_per_km(mj_per_km: float) -> float:
    return mj_per_km / MJ_PER_KWH * 1000.0


def powertrain(vehicle: dict) -> str:
    fuel = (vehicle.get("fuelType") or "").lower()
    if fuel.startswith("elétr"):
        return "bev"
    if "plug" in fuel or "plug" in (vehicle.get("engine") or "").lower():
        return "phev"
    return "other"


def validate(payload: dict) -> list[str]:
    errors: list[str] = []
    vehicles = payload.get("vehicles", [])
    if not vehicles:
        return ["catalog has no vehicles"]

    ids = set()
    for vehicle in vehicles:
        vid = vehicle.get("id")
        if not vid:
            errors.append("vehicle without id")
            continue
        if vid in ids:
            errors.append(f"duplicate id: {vid}")
        ids.add(vid)

        if not vehicle.get("make") or not vehicle.get("model"):
            errors.append(f"{vid}: missing make/model")

        kind = powertrain(vehicle)
        mj = vehicle.get("consumptionMjPerKm")
        if mj is not None:
            low, high = CONSUMPTION_BANDS[kind]
            if not (low <= mj <= high):
                errors.append(f"{vid}: implausible {kind} consumption {mj} MJ/km")

        rng = vehicle.get("rangeKm")
        if rng is not None and kind in RANGE_BANDS:
            low, high = RANGE_BANDS[kind]
            if not (low <= rng <= high):
                errors.append(f"{vid}: implausible {kind} range {rng:.0f} km")

    for vehicle in vehicles:
        if powertrain(vehicle) == "bev":
            if vehicle.get("consumptionMjPerKm") is None or vehicle.get("rangeKm") is None:
                errors.append(f"{vehicle.get('id')}: electric vehicle without consumption/range")

    counts = payload.get("counts", {})
    if counts.get("vehicles") != len(vehicles):
        errors.append(f"count mismatch: header {counts.get('vehicles')} vs {len(vehicles)} rows")
    return errors


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("catalog", type=Path)
    args = parser.parse_args(argv)

    payload = json.loads(args.catalog.read_text(encoding="utf-8"))
    errors = validate(payload)
    counts = payload.get("counts", {})
    print(f"{counts.get('vehicles', 0)} vehicles, {counts.get('electric', 0)} electric")
    if errors:
        for error in errors[:50]:
            print(f"ERROR: {error}", file=sys.stderr)
        print(f"{len(errors)} problem(s)", file=sys.stderr)
        return 1
    print("OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
