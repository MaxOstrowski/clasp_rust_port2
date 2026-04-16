#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
from collections import defaultdict, deque
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
ORIGINAL_ROOT = REPO_ROOT / "original_clasp"
ANALYSIS_FILE = REPO_ROOT / "analysis" / "original_clasp" / "file_usage.json"
DEFAULT_OUTPUT = REPO_ROOT / "analysis" / "original_clasp" / "source_tree_by_import_order.txt"
OWNED_PREFIXES = (
    "app/",
    "clasp/",
    "src/",
    "tests/",
    "libpotassco/app/",
    "libpotassco/potassco/",
    "libpotassco/src/",
    "libpotassco/tests/",
)
SOURCE_SUFFIXES = (
    ".c",
    ".cc",
    ".cpp",
    ".cxx",
    ".h",
    ".hh",
    ".hpp",
    ".hxx",
    ".inl",
    ".h.in",
)


def is_owned_source(rel_path: str) -> bool:
    return rel_path.endswith(SOURCE_SUFFIXES) and rel_path.startswith(OWNED_PREFIXES)


def collect_target_files() -> list[str]:
    files: list[str] = []
    for prefix in OWNED_PREFIXES:
        scope_root = ORIGINAL_ROOT / prefix
        if not scope_root.exists():
            continue
        for path in scope_root.rglob("*"):
            if not path.is_file():
                continue
            rel_path = path.relative_to(ORIGINAL_ROOT).as_posix()
            if is_owned_source(rel_path):
                files.append(rel_path)
    return sorted(set(files))


def load_file_usage() -> list[dict[str, object]]:
    data = json.loads(ANALYSIS_FILE.read_text())
    return data["files"]


def build_dependency_graph(target_files: set[str]) -> dict[str, set[str]]:
    graph = {path: set() for path in target_files}
    for entry in load_file_usage():
        source = entry["file"]
        if source not in target_files:
            continue
        for used in entry.get("used_entities", []):
            location = used.get("primary_location") or {}
            target = location.get("file")
            if target in target_files and target != source:
                graph[source].add(target)
    return graph


def resolve_include(source: str, include: str, target_files: set[str]) -> str | None:
    candidates = []
    normalized = include.replace("\\", "/")
    source_dir = Path(source).parent
    candidates.append((source_dir / normalized).as_posix())
    candidates.append(normalized)
    if normalized.startswith("potassco/"):
        candidates.append(f"libpotassco/{normalized}")

    seen: set[str] = set()
    for candidate in candidates:
        if candidate in seen:
            continue
        seen.add(candidate)
        if candidate in target_files:
            return candidate
    return None


def build_include_graph(target_files: set[str]) -> dict[str, set[str]]:
    graph = {path: set() for path in target_files}
    for source in sorted(target_files):
        source_path = ORIGINAL_ROOT / source
        for line in source_path.read_text().splitlines():
            stripped = line.strip()
            if not stripped.startswith("#include"):
                continue
            include = None
            if '"' in stripped:
                parts = stripped.split('"')
                if len(parts) >= 3:
                    include = parts[1]
            elif "<" in stripped and ">" in stripped:
                include = stripped.split("<", 1)[1].split(">", 1)[0]
            if include is None:
                continue
            resolved = resolve_include(source, include, target_files)
            if resolved is not None and resolved != source:
                graph[source].add(resolved)
    return graph


def strongly_connected_components(graph: dict[str, set[str]]) -> list[list[str]]:
    index = 0
    stack: list[str] = []
    on_stack: set[str] = set()
    indices: dict[str, int] = {}
    lowlinks: dict[str, int] = {}
    components: list[list[str]] = []

    def visit(node: str) -> None:
        nonlocal index
        indices[node] = index
        lowlinks[node] = index
        index += 1
        stack.append(node)
        on_stack.add(node)

        for dependency in sorted(graph[node]):
            if dependency not in indices:
                visit(dependency)
                lowlinks[node] = min(lowlinks[node], lowlinks[dependency])
            elif dependency in on_stack:
                lowlinks[node] = min(lowlinks[node], indices[dependency])

        if lowlinks[node] != indices[node]:
            return

        component: list[str] = []
        while True:
            current = stack.pop()
            on_stack.remove(current)
            component.append(current)
            if current == node:
                break
        components.append(sorted(component))

    for node in sorted(graph):
        if node not in indices:
            visit(node)

    return components


def order_component(component: list[str], secondary_graph: dict[str, set[str]]) -> list[str]:
    if len(component) <= 1:
        return component

    members = set(component)
    adjacency: dict[str, set[str]] = {path: set() for path in component}
    indegree = {path: 0 for path in component}
    for source in component:
        for dependency in secondary_graph.get(source, set()):
            if dependency not in members:
                continue
            if source not in adjacency[dependency]:
                adjacency[dependency].add(source)
                indegree[source] += 1

    ready = deque(sorted(path for path, degree in indegree.items() if degree == 0))
    ordered: list[str] = []
    while ready:
        path = ready.popleft()
        ordered.append(path)
        for dependent in sorted(adjacency[path]):
            indegree[dependent] -= 1
            if indegree[dependent] == 0:
                ready.append(dependent)

    if len(ordered) == len(component):
        return ordered

    remaining = sorted(members.difference(ordered))
    return ordered + remaining


def topo_sort_files(graph: dict[str, set[str]], secondary_graph: dict[str, set[str]]) -> list[str]:
    components = strongly_connected_components(graph)
    component_of: dict[str, int] = {}
    for component_id, component in enumerate(components):
        for node in component:
            component_of[node] = component_id

    component_graph: dict[int, set[int]] = defaultdict(set)
    indegree = {component_id: 0 for component_id in range(len(components))}
    for source, dependencies in graph.items():
        source_component = component_of[source]
        for dependency in dependencies:
            dependency_component = component_of[dependency]
            if source_component == dependency_component:
                continue
            if source_component not in component_graph[dependency_component]:
                component_graph[dependency_component].add(source_component)
                indegree[source_component] += 1

    ready = deque(sorted(component_id for component_id, degree in indegree.items() if degree == 0))
    ordered_components: list[int] = []
    while ready:
        component_id = ready.popleft()
        ordered_components.append(component_id)
        for dependent in sorted(component_graph[component_id]):
            indegree[dependent] -= 1
            if indegree[dependent] == 0:
                ready.append(dependent)

    if len(ordered_components) != len(components):
        raise RuntimeError("component ordering did not cover all files")

    ordered_files: list[str] = []
    for component_id in ordered_components:
        ordered_files.extend(order_component(components[component_id], secondary_graph))
    return ordered_files


def write_output(output_path: Path, ordered_files: list[str]) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("".join(f"{path}:\n" for path in ordered_files))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Create an import-ordered original_clasp source-file tree from analysis/original_clasp/file_usage.json."
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=DEFAULT_OUTPUT,
        help=f"Path for the generated tree file (default: {DEFAULT_OUTPUT.relative_to(REPO_ROOT)})",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    target_files = set(collect_target_files())
    include_graph = build_include_graph(target_files)
    dependency_graph = build_dependency_graph(target_files)
    ordered_files = topo_sort_files(include_graph, dependency_graph)
    write_output(args.output, ordered_files)
    print(f"wrote {len(ordered_files)} files to {args.output.relative_to(REPO_ROOT)}")


if __name__ == "__main__":
    main()