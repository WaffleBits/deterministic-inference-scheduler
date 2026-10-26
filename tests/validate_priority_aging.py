#!/usr/bin/env python3
"""Assert the priority-aging replay contract from real CLI JSON output."""

import json
import sys
from pathlib import Path


def read_json(path: str) -> dict:
    with Path(path).open(encoding="utf-8") as source:
        return json.load(source)


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit("usage: validate_priority_aging.py <input.json> <report.json>")

    replay_input = read_json(sys.argv[1])
    report = read_json(sys.argv[2])
    requests = replay_input["requests"]
    low_priority = next(request for request in requests if request["id"] == "low-priority")
    later_higher_arrivals = [
        request["arrival_tick"]
        for request in requests
        if request["priority"] > low_priority["priority"]
        and request["arrival_tick"] > low_priority["arrival_tick"]
    ]
    assert later_higher_arrivals, "fixture must keep higher-priority arrivals active"

    admissions = [
        tick["tick"]
        for tick in report["ticks"]
        if "low-priority" in tick["admitted"]
    ]
    assert len(admissions) == 1, f"expected one low-priority admission, got {admissions}"
    assert admissions[0] < max(later_higher_arrivals), (
        "low-priority work must be admitted before higher-priority arrivals stop"
    )
    assert report["request_count"] == len(requests)
    assert report["completed_requests"] == len(requests)
    assert report["final_kv_pages"] == 0

    print(
        "priority-aging CLI fixture passed: low-priority request admitted at "
        f"tick {admissions[0]} before the last higher-priority arrival at "
        f"tick {max(later_higher_arrivals)}; {len(requests)}/{len(requests)} completed"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
