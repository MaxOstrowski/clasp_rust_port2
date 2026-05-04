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

CONFIG_HEADER_ALIASES = {
    "build-analysis/clasp/config.h": "clasp/config.h.in",
    "build-analysis-full/clasp/config.h": "clasp/config.h.in",
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


def load_source_tree_entries(path: Path) -> dict[str, dict[str, object]]:
    entries: dict[str, dict[str, object]] = {}
    current: str | None = None
    for raw_line in path.read_text().splitlines():
        line = raw_line.rstrip()
        if not line:
            continue
        stripped = line.lstrip()
        if stripped.startswith("#"):
            continue
        if line == stripped:
            file_name, separator, status = stripped.partition(":")
            current = file_name.strip()
            entries[current] = {
                "status": status.strip() if separator else "",
                "paths": [],
            }
            continue
        if current is None:
            continue
        if stripped.startswith("#"):
            continue
        cast_paths = entries[current]["paths"]
        assert isinstance(cast_paths, list)
        cast_paths.append(stripped)
    return entries


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


def owner_names(entities: list[dict]) -> dict[str, str]:
    names: dict[str, str] = {}
    for entity in entities:
        usr = entity.get("usr")
        qualified_name = entity.get("qualified_name")
        if not usr or not qualified_name:
            continue
        names[usr] = qualified_name
    return names


def normalize_tracker_header(header: str) -> str:
    return CONFIG_HEADER_ALIASES.get(header, header)


def fallback_owner_name(entity: dict) -> str:
    qualified_name = entity["qualified_name"]
    method_name = entity.get("name") or qualified_name.rsplit("::", 1)[-1]
    suffix = f"::{method_name}"
    if qualified_name.endswith(suffix):
        return qualified_name[: -len(suffix)]
    owner, _, _ = qualified_name.rpartition("::")
    return owner


def group_methods_by_header(entities: list[dict], headers: set[str]) -> dict[str, list[dict[str, str]]]:
    methods_by_header: dict[str, dict[str, dict[str, str]]] = defaultdict(dict)
    owners = owner_names(entities)
    for header in headers:
        methods_by_header[header]
    for entity in entities:
        if not is_method_entity(entity):
            continue
        primary_location = entity.get("primary_location") or {}
        header = primary_location.get("file")
        if header not in headers:
            continue
        qualified_name = entity["qualified_name"]
        methods_by_header[header][qualified_name] = {
            "qualified_name": qualified_name,
            "method_name": entity.get("name") or qualified_name.rsplit("::", 1)[-1],
            "owner_name": owners.get(entity.get("owner_class_usr") or "") or fallback_owner_name(entity),
        }
    return {
        header: sorted(methods.values(), key=lambda item: (item["owner_name"], item["qualified_name"]))
        for header, methods in methods_by_header.items()
    }


def output_path(output_root: Path, header: str) -> Path:
    return output_root / Path(f"{header}.txt")


def split_annotation_paths(paths: list[str]) -> tuple[list[str], list[str]]:
    impl_paths = sorted({path for path in paths if path.startswith(("src/", "examples/"))})
    test_paths = sorted({path for path in paths if path.startswith("tests/")})
    return impl_paths, test_paths


def parse_path_list(value: str) -> list[str]:
    if value in {"missing", "pending", ""}:
        return []
    return [part.strip() for part in value.split(",") if part.strip()]


def parse_existing_method_file(path: Path) -> dict[str, dict[str, list[str]]]:
    if not path.is_file():
        return {}
    annotations: dict[str, dict[str, list[str]]] = {}
    for raw_line in path.read_text().splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or line.startswith("[Class]"):
            continue
        if line.startswith("- "):
            payload = line[2:]
            if " | impl: " in payload:
                qualified_name, rest = payload.split(" | impl: ", 1)
                impl_value, tests_sep, tests_value = rest.partition(" | tests: ")
                annotations[qualified_name] = {
                    "impl": parse_path_list(impl_value.strip()),
                    "tests": parse_path_list(tests_value.strip()) if tests_sep else [],
                }
                continue
            line = payload
        qualified_name = line
        annotated_paths: list[str] = []
        if ": " in line:
            maybe_name, maybe_paths = line.rsplit(": ", 1)
            if "/" in maybe_paths and maybe_paths.endswith(".rs"):
                qualified_name = maybe_name
                annotated_paths = parse_path_list(maybe_paths)
        impl_paths, test_paths = split_annotation_paths(annotated_paths)
        annotations[qualified_name] = {
            "impl": impl_paths,
            "tests": test_paths,
        }
    return annotations


def load_existing_annotations(
    output_root: Path, headers: list[str]
) -> dict[str, dict[str, dict[str, list[str]]]]:
    return {
        header: parse_existing_method_file(output_path(output_root, header))
        for header in headers
    }


def format_path_list(paths: list[str], empty: str) -> str:
    return ", ".join(paths) if paths else empty


def render_method_file(
    header: str,
    methods: list[dict[str, str]],
    source_tree_entries: dict[str, dict[str, object]],
    existing_annotations: dict[str, dict[str, list[str]]],
) -> str:
    tracker_key = normalize_tracker_header(header)
    entry = source_tree_entries.get(tracker_key, {"status": "not tracked", "paths": []})
    raw_paths = entry.get("paths", [])
    assert isinstance(raw_paths, list)
    impl_files, test_files = split_annotation_paths([path for path in raw_paths if isinstance(path, str)])
    status = entry.get("status", "")
    assert isinstance(status, str)
    fully_ported = status == "ported"

    lines = [
        f"# original: {header}",
        f"# tracker: {tracker_key}",
        f"# status: {status or 'untracked'}",
        f"# impl_files: {format_path_list(impl_files, 'none')}",
        f"# test_files: {format_path_list(test_files, 'none')}",
    ]
    if methods:
        lines.append("")

    current_owner: str | None = None
    for method in methods:
        owner_name = method["owner_name"]
        if owner_name != current_owner:
            if current_owner is not None:
                lines.append("")
            lines.append(f"[Class] {owner_name}")
            current_owner = owner_name

        annotation = existing_annotations.get(method["qualified_name"], {"impl": [], "tests": []})
        impl_paths = sorted(set(annotation.get("impl", [])))
        test_paths = sorted(set(annotation.get("tests", [])))
        if fully_ported and not impl_paths:
            impl_paths = impl_files
        if fully_ported and not test_paths:
            test_paths = test_files
        lines.append(
            f"- {method['qualified_name']} | impl: {format_path_list(impl_paths, 'missing')} | tests: {format_path_list(test_paths, 'pending')}"
        )
    if lines and lines[-1] != "":
        lines.append("")
    return "\n".join(lines)


def write_method_files(
    output_root: Path,
    methods_by_header: dict[str, list[dict[str, str]]],
    source_tree_entries: dict[str, dict[str, object]],
    existing_annotations_by_header: dict[str, dict[str, dict[str, list[str]]]],
) -> tuple[int, int, int]:
    header_count = 0
    non_empty_count = 0
    method_count = 0
    for header, methods in sorted(methods_by_header.items()):
        target = output_path(output_root, header)
        target.parent.mkdir(parents=True, exist_ok=True)
        existing_annotations = existing_annotations_by_header.get(header, {})
        target.write_text(render_method_file(header, methods, source_tree_entries, existing_annotations))
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
    source_tree_path = analysis_dir / "source_tree_by_import_order.txt"

    headers = collect_headers(original_root, args.headers)
    methods_by_header = group_methods_by_header(load_symbol_inventory(symbol_inventory_path), set(headers))
    source_tree_entries = load_source_tree_entries(source_tree_path)
    existing_annotations_by_header = (
        load_existing_annotations(output_dir, headers) if output_dir.exists() else {}
    )

    if output_dir.exists() and not args.no_clean:
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    header_count, non_empty_count, method_count = write_method_files(
        output_dir,
        methods_by_header,
        source_tree_entries,
        existing_annotations_by_header,
    )
    print(f"Generated {header_count} files in {output_dir}")
    print(f"Headers with at least one method: {non_empty_count}")
    print(f"Distinct qualified method names written: {method_count}")


if __name__ == "__main__":
    main()