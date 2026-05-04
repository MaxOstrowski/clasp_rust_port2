#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import shutil
from collections import defaultdict
from pathlib import Path


METHOD_KINDS = {
    "CXX_METHOD",
    "CONSTRUCTOR",
    "DESTRUCTOR",
    "CONVERSION_FUNCTION",
    "FUNCTION_TEMPLATE",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Generate one method-name list per original header file from analysis/original_clasp/symbol_inventory.json."
        )
    )
    parser.add_argument(
        "--project-root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="Repository root containing original_clasp/ and analysis/original_clasp/.",
    )
    parser.add_argument(
        "--analysis-dir",
        type=Path,
        help="Override the analysis/original_clasp directory.",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        help="Override the output directory. Defaults to <analysis-dir>/method_collection.",
    )
    parser.add_argument(
        "--headers",
        nargs="+",
        help="Optional list of header paths relative to original_clasp/ to generate.",
    )
    parser.add_argument(
        "--no-clean",
        action="store_true",
        help="Keep the existing output directory contents instead of recreating it.",
    )
    return parser.parse_args()


def load_symbol_inventory(path: Path) -> list[dict]:
    data = json.loads(path.read_text())
    return data["entities"]


def collect_headers(original_root: Path, requested_headers: list[str] | None) -> list[str]:
    if requested_headers:
        headers = sorted({Path(header).as_posix() for header in requested_headers})
        missing = [header for header in headers if not (original_root / header).is_file()]
        if missing:
            raise SystemExit(f"Unknown header path(s): {', '.join(missing)}")
        return headers
    return sorted(path.relative_to(original_root).as_posix() for path in original_root.rglob("*.h"))


def is_method_entity(entity: dict) -> bool:
    if entity.get("kind") not in METHOD_KINDS:
        return False
    if entity.get("owner_class_usr") is None:
        return False
    qualified_name = entity.get("qualified_name", "")
    if "(lambda at " in qualified_name or "(anonymous" in qualified_name:
        return False
    return True


def group_methods_by_header(entities: list[dict], headers: set[str]) -> dict[str, list[str]]:
    methods_by_header: dict[str, set[str]] = defaultdict(set)
    for header in headers:
        methods_by_header[header]
    for entity in entities:
        if not is_method_entity(entity):
            continue
        primary_location = entity.get("primary_location") or {}
        header = primary_location.get("file")
        if header not in headers:
            continue
        methods_by_header[header].add(entity["qualified_name"])
    return {header: sorted(methods) for header, methods in methods_by_header.items()}


def output_path(output_root: Path, header: str) -> Path:
    return output_root / Path(f"{header}.txt")


def write_method_files(output_root: Path, methods_by_header: dict[str, list[str]]) -> tuple[int, int, int]:
    header_count = 0
    non_empty_count = 0
    method_count = 0
    for header, methods in sorted(methods_by_header.items()):
        target = output_path(output_root, header)
        target.parent.mkdir(parents=True, exist_ok=True)
        content = "\n".join(methods)
        if content:
            content += "\n"
        target.write_text(content)
        header_count += 1
        if methods:
            non_empty_count += 1
            method_count += len(methods)
    return header_count, non_empty_count, method_count


def main() -> None:
    args = parse_args()
    project_root = args.project_root.resolve()
    analysis_dir = (args.analysis_dir or (project_root / "analysis" / "original_clasp")).resolve()
    output_dir = (args.output_dir or (analysis_dir / "method_collection")).resolve()
    original_root = (project_root / "original_clasp").resolve()
    symbol_inventory_path = analysis_dir / "symbol_inventory.json"

    headers = collect_headers(original_root, args.headers)
    methods_by_header = group_methods_by_header(load_symbol_inventory(symbol_inventory_path), set(headers))

    if output_dir.exists() and not args.no_clean:
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    header_count, non_empty_count, method_count = write_method_files(output_dir, methods_by_header)
    print(f"Generated {header_count} files in {output_dir}")
    print(f"Headers with at least one method: {non_empty_count}")
    print(f"Distinct qualified method names written: {method_count}")


if __name__ == "__main__":
    main()