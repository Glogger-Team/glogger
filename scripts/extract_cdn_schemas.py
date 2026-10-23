"""
Extract field schemas from Project Gorgon CDN JSON files.

For each CDN file, produces:
  - Total entry count
  - Structure type (map of objects, map of arrays, etc.)
  - Per-field: name, frequency (count + percentage), types observed, sample values

Usage:
    python scripts/extract_cdn_schemas.py
    python scripts/extract_cdn_schemas.py --input path/to/cdn/files --output path/to/output.json
"""

import argparse
import json
import os
import sys
from collections import defaultdict


def classify_value(v):
    """Classify a JSON value into a human-readable type string."""
    if v is None:
        return "null"
    if isinstance(v, bool):
        return "boolean"
    if isinstance(v, int):
        return "integer"
    if isinstance(v, float):
        return "number"
    if isinstance(v, str):
        return "string"
    if isinstance(v, list):
        if len(v) == 0:
            return "array<empty>"
        inner = classify_value(v[0])
        return f"array<{inner}>"
    if isinstance(v, dict):
        return "object"
    return type(v).__name__


def truncate_sample(value, max_len=120):
    """Truncate a sample value for display."""
    if isinstance(value, (list, dict)):
        s = json.dumps(value)
        if len(s) > max_len:
            return s[:max_len] + "..."
        return s
    if isinstance(value, str) and len(value) > 80:
        return value[:80] + "..."
    return value


def extract_schema(filepath):
    """Extract field schema from a single CDN JSON file."""
    with open(filepath, "r", encoding="utf-8") as f:
        data = json.load(f)

    if not isinstance(data, dict):
        return None, 0, "top-level is not object"

    first_key = next(iter(data), None)
    if first_key is None:
        return None, 0, "empty"

    first_val = data[first_key]

    if isinstance(first_val, dict):
        total_entries = len(data)
        field_info = defaultdict(lambda: {"count": 0, "types": set(), "samples": []})

        for entry in data.values():
            if not isinstance(entry, dict):
                continue
            for field, value in entry.items():
                info = field_info[field]
                info["count"] += 1
                info["types"].add(classify_value(value))
                if len(info["samples"]) < 2:
                    info["samples"].append(truncate_sample(value))

        return field_info, total_entries, "map"

    if isinstance(first_val, list):
        return None, len(data), f"map of arrays (sample key: {first_key})"

    return None, len(data), f"map of {classify_value(first_val)}"


def main():
    parser = argparse.ArgumentParser(description="Extract CDN JSON field schemas")
    parser.add_argument(
        "--input",
        default="docs/samples/CDN-full-examples",
        help="Directory containing CDN JSON files",
    )
    parser.add_argument(
        "--output",
        default="docs/reference/cdn-field-schemas.json",
        help="Output JSON file path",
    )
    args = parser.parse_args()

    if not os.path.isdir(args.input):
        print(f"Error: input directory not found: {args.input}")
        return 1

    results = {}
    for fname in sorted(os.listdir(args.input)):
        if not fname.endswith(".json"):
            continue
        filepath = os.path.join(args.input, fname)
        field_info, total, structure = extract_schema(filepath)
        results[fname] = {
            "total_entries": total,
            "structure": structure,
            "fields": {},
        }
        if field_info:
            for field, info in sorted(field_info.items()):
                results[fname]["fields"][field] = {
                    "count": info["count"],
                    "pct": round(100 * info["count"] / total, 1),
                    "types": sorted(info["types"]),
                    "samples": info["samples"],
                }

    os.makedirs(os.path.dirname(args.output) or ".", exist_ok=True)
    with open(args.output, "w", encoding="utf-8") as f:
        json.dump(results, f, indent=2, default=str)

    # Print summary
    total_files = len(results)
    total_fields = sum(len(r["fields"]) for r in results.values())
    print(
        f"Extracted schemas from {total_files} CDN files ({total_fields} total fields)"
    )
    print(f"Output: {args.output}")

    for fname, info in sorted(results.items()):
        field_count = len(info["fields"])
        print(
            f"  {fname}: {info['total_entries']} entries, {field_count} fields ({info['structure']})"
        )

    return 0


if __name__ == "__main__":
    sys.exit(main())
