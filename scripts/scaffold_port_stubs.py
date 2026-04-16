from __future__ import annotations

import argparse
import json
from collections import defaultdict
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
ANALYSIS_FILE = REPO_ROOT / "analysis/original_clasp/file_usage.json"


def base_name(path: Path) -> str:
    name = path.name
    if name.endswith(".h.in"):
        return name[:-5]
    return path.stem


def is_header_like(path: Path) -> bool:
    name = path.name
    return name.endswith(".h") or name.endswith(".h.in") or name.endswith(".inl")


def header_target_for_path(source: Path) -> Path:
    parts = source.parts
    stem = base_name(source)
    if parts[0] == "clasp":
        return REPO_ROOT / "src" / "clasp" / Path(*parts[1:-1]) / f"{stem}.rs"
    if parts[:2] == ("libpotassco", "potassco"):
        file_name = "enums.rs" if stem == "enum" else f"{stem}.rs"
        return REPO_ROOT / "src" / "potassco" / Path(*parts[2:-1]) / file_name
    raise ValueError(f"Unsupported header path: {source}")


def build_header_index(files: list[str]) -> dict[str, dict[str, list[Path]]]:
    index: dict[str, dict[str, list[Path]]] = {
        "clasp": defaultdict(list),
        "potassco": defaultdict(list),
    }
    for original_file in files:
        source = Path(original_file)
        if not is_header_like(source):
            continue
        stem = base_name(source)
        if source.parts[0] == "clasp":
            index["clasp"][stem].append(header_target_for_path(source))
        elif source.parts[:2] == ("libpotassco", "potassco"):
            index["potassco"][stem].append(header_target_for_path(source))
    return index


def unique_header_target(index: dict[str, dict[str, list[Path]]], domain: str, stem: str) -> Path | None:
    matches = index[domain].get(stem, [])
    unique_matches = sorted(set(matches))
    return unique_matches[0] if len(unique_matches) == 1 else None


def map_original_to_target(original_file: str, header_index: dict[str, dict[str, list[Path]]]) -> tuple[str, Path]:
    source = Path(original_file)
    parts = source.parts
    stem = base_name(source)

    if parts[0] == "clasp":
        return "clasp", header_target_for_path(source)
    if parts[0] == "src":
        matched = unique_header_target(header_index, "clasp", stem)
        return "clasp", matched or (REPO_ROOT / "src" / "clasp" / f"{stem}.rs")
    if parts[:2] == ("libpotassco", "potassco"):
        return "potassco", header_target_for_path(source)
    if parts[:2] == ("libpotassco", "src"):
        matched = unique_header_target(header_index, "potassco", stem)
        return "potassco", matched or (REPO_ROOT / "src" / "potassco" / f"{stem}.rs")
    if parts[:2] == ("libpotassco", "app"):
        return "potassco", REPO_ROOT / "src" / "potassco" / "app" / Path(*parts[2:-1]) / f"{stem}.rs"
    if parts[0] == "app":
        return "app", REPO_ROOT / "src" / "app" / Path(*parts[1:-1]) / f"{stem}.rs"
    if parts[0] == "examples":
        return "examples", REPO_ROOT / "examples" / "ported" / Path(*parts[1:-1]) / f"{stem}.rs"
    if parts[:2] == ("libpotassco", "tests"):
        return "tests", REPO_ROOT / "tests" / "ported" / "potassco" / Path(*parts[2:-1]) / f"{stem}.rs"
    if parts[0] == "tests":
        return "tests", REPO_ROOT / "tests" / "ported" / "clasp" / Path(*parts[1:-1]) / f"{stem}.rs"
    raise ValueError(f"Unsupported original file: {original_file}")


def load_targets() -> dict[str, dict[Path, list[str]]]:
    data = json.loads(ANALYSIS_FILE.read_text())
    original_files = [entry["file"] for entry in data["files"]]
    header_index = build_header_index(original_files)
    grouped: dict[str, dict[Path, list[str]]] = defaultdict(lambda: defaultdict(list))
    for original_file in original_files:
        group, target = map_original_to_target(original_file, header_index)
        grouped[group][target].append(original_file)
    return grouped


def stub_line(originals: list[str]) -> str:
    refs = ", ".join(f"original_clasp/{path}" for path in sorted(originals))
    return f"//! Port target for {refs}.\n"


def print_plan(grouped: dict[str, dict[Path, list[str]]], group: str | None) -> None:
    groups = [group] if group else sorted(grouped)
    for name in groups:
        targets = grouped[name]
        print(f"[{name}] {len(targets)} rust target files")
        for target in sorted(targets):
            originals = ", ".join(sorted(targets[target]))
            print(f"  {target.relative_to(REPO_ROOT)} <= {originals}")


def apply_group(grouped: dict[str, dict[Path, list[str]]], group: str) -> None:
    created = 0
    skipped = 0
    for target, originals in sorted(grouped[group].items()):
        if target.exists():
            skipped += 1
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(stub_line(originals))
        created += 1
    print(f"[{group}] created={created} skipped_existing={skipped}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Create one-line Rust stub files from analyzed original_clasp files.")
    parser.add_argument("command", choices=("plan", "apply"))
    parser.add_argument("--group", choices=("app", "clasp", "examples", "potassco", "tests"))
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    grouped = load_targets()
    if args.command == "plan":
        print_plan(grouped, args.group)
        return
    if not args.group:
        raise SystemExit("--group is required for apply")
    apply_group(grouped, args.group)


if __name__ == "__main__":
    main()