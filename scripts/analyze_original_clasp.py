#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import shlex
from collections import defaultdict, deque
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from clang import cindex


OWNED_PATH_PREFIXES = (
    "clasp/",
    "src/",
    "app/",
    "examples/",
    "tests/",
    "libpotassco/potassco/",
    "libpotassco/src/",
    "libpotassco/app/",
    "libpotassco/tests/",
)
EXTERNAL_PROJECT_PATH_PREFIXES = (
    "libpotassco/third_party/",
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
GCC_INSTALL_DIR = "/usr/lib/gcc/x86_64-linux-gnu/13"
GCC_INCLUDE_DIR = "/usr/lib/gcc/x86_64-linux-gnu/13/include"
KEEP_WARNING_FLAGS = {"-Wno-deprecated-declarations"}


def cursor_kind(name: str) -> cindex.CursorKind | None:
    return getattr(cindex.CursorKind, name, None)

CLASS_KINDS = {
    cindex.CursorKind.CLASS_DECL,
    cindex.CursorKind.STRUCT_DECL,
    cindex.CursorKind.UNION_DECL,
    cindex.CursorKind.CLASS_TEMPLATE,
    cindex.CursorKind.CLASS_TEMPLATE_PARTIAL_SPECIALIZATION,
}
FUNCTION_KINDS = {
    cindex.CursorKind.FUNCTION_DECL,
    cindex.CursorKind.FUNCTION_TEMPLATE,
    cindex.CursorKind.CXX_METHOD,
    cindex.CursorKind.CONSTRUCTOR,
    cindex.CursorKind.DESTRUCTOR,
    cindex.CursorKind.CONVERSION_FUNCTION,
}
CALL_KINDS = {kind for kind in {cursor_kind("CALL_EXPR")} if kind is not None}
REFERENCE_RELATIONS = {
    kind: relation
    for kind, relation in {
        cursor_kind("TYPE_REF"): "type",
        cursor_kind("TEMPLATE_REF"): "template",
        cursor_kind("DECL_REF_EXPR"): "reference",
        cursor_kind("MEMBER_REF_EXPR"): "member-reference",
        cursor_kind("MEMBER_REF"): "member-reference",
    }.items()
    if kind is not None
}


@dataclass(frozen=True)
class SourceLocation:
    file: str
    line: int
    column: int

    def as_dict(self) -> dict[str, Any]:
        return {"file": self.file, "line": self.line, "column": self.column}


class Analyzer:
    def __init__(self, project_root: Path, build_dir: Path, output_dir: Path) -> None:
        self.project_root = project_root.resolve()
        self.build_dir = build_dir.resolve()
        self.output_dir = output_dir.resolve()
        self.compile_commands_path = self.build_dir / "compile_commands.json"
        self.generated_path_map = {
            str((self.build_dir / "clasp" / "config.h").resolve()): self.project_root / "clasp" / "config.h.in",
        }
        self.compile_entries = json.loads(self.compile_commands_path.read_text())
        self.compile_args_by_file: dict[Path, list[str]] = {}
        self.donor_args_by_scope: dict[str, list[str]] = {}
        self.generic_donor_args: list[str] | None = None
        self.entities: dict[str, dict[str, Any]] = {}
        self.entity_fingerprints: dict[tuple[str, str, str, str, int], str] = {}
        self.entity_dependencies: dict[str, dict[str, set[str]]] = defaultdict(lambda: defaultdict(set))
        self.entity_external_dependencies: dict[str, dict[str, dict[str, Any]]] = defaultdict(dict)
        self.file_usage: dict[str, dict[str, Any]] = defaultdict(self._new_file_usage)
        self.class_members: dict[str, set[str]] = defaultdict(set)
        self.member_owner: dict[str, str] = {}
        self.parse_results: list[dict[str, Any]] = []
        self.skipped_files: list[dict[str, Any]] = []
        self._prepare_compile_args()

    def _new_file_usage(self) -> dict[str, Any]:
        return {
            "declared_entities": set(),
            "defined_entities": set(),
            "used_entities": set(),
            "used_classes": set(),
            "used_top_level_functions": set(),
            "used_methods_or_constructors": set(),
            "external_project_symbols": {},
        }

    def _prepare_compile_args(self) -> None:
        for entry in self.compile_entries:
            source_file = Path(entry["file"]).resolve()
            args = self._normalize_compile_command(entry)
            self.compile_args_by_file[source_file] = args
            relative_path = self._rel(source_file)
            scope = self._owned_scope_for_rel(relative_path)
            if scope and scope not in self.donor_args_by_scope:
                self.donor_args_by_scope[scope] = list(args)
            if self.generic_donor_args is None and scope is not None:
                self.generic_donor_args = list(args)
        if self.generic_donor_args is None and self.compile_args_by_file:
            self.generic_donor_args = list(next(iter(self.compile_args_by_file.values())))
        if self.generic_donor_args is None:
            raise SystemExit(f"No compile commands found in {self.compile_commands_path}")

    def _normalize_compile_command(self, entry: dict[str, Any]) -> list[str]:
        args = shlex.split(entry["command"])
        source_file = entry["file"]
        normalized = [f"--gcc-install-dir={GCC_INSTALL_DIR}", f"-I{GCC_INCLUDE_DIR}"]
        skip_next = False
        for index, arg in enumerate(args):
            if index == 0:
                continue
            if skip_next:
                skip_next = False
                continue
            if arg == "-o":
                skip_next = True
                continue
            if arg == "-c" or arg == source_file:
                continue
            if arg.startswith("-W") and arg not in KEEP_WARNING_FLAGS:
                continue
            normalized.append(arg)
        return normalized

    def run(self) -> None:
        index = cindex.Index.create()
        for path in self._collect_compile_database_sources():
            print(f"[analyze] {self._rel(path)}", flush=True)
            args, synthetic = self._args_for_file(path)
            parse_options = cindex.TranslationUnit.PARSE_PRECOMPILED_PREAMBLE
            if self._is_header_like(path):
                parse_options |= cindex.TranslationUnit.PARSE_INCOMPLETE
            try:
                translation_unit = index.parse(str(path), args=args, options=parse_options)
            except cindex.TranslationUnitLoadError as exc:
                self.skipped_files.append({
                    "file": self._rel(path),
                    "reason": f"parse failed: {exc}",
                    "synthetic_command": synthetic,
                })
                continue
            diagnostics = [
                {
                    "severity": diag.severity,
                    "spelling": diag.spelling,
                    "file": self._rel(self._normalize_path(diag.location.file.name)) if diag.location.file else None,
                    "line": diag.location.line if diag.location.file else None,
                }
                for diag in translation_unit.diagnostics
                if diag.severity >= cindex.Diagnostic.Warning
            ]
            self.parse_results.append(
                {
                    "file": self._rel(path),
                    "synthetic_command": synthetic,
                    "diagnostic_count": len(diagnostics),
                    "diagnostics": diagnostics,
                }
            )
            self._walk_cursor(translation_unit.cursor, current_class_usr=None, current_function_usr=None)
        seen_files = set(self.file_usage.keys())
        for path in self._collect_unseen_owned_headers(seen_files):
            print(f"[header] {self._rel(path)}", flush=True)
            args, synthetic = self._args_for_file(path)
            try:
                translation_unit = index.parse(
                    str(path),
                    args=args,
                    options=cindex.TranslationUnit.PARSE_PRECOMPILED_PREAMBLE | cindex.TranslationUnit.PARSE_INCOMPLETE,
                )
            except cindex.TranslationUnitLoadError as exc:
                self.skipped_files.append(
                    {
                        "file": self._rel(path),
                        "reason": f"parse failed: {exc}",
                        "synthetic_command": synthetic,
                    }
                )
                continue
            diagnostics = [
                {
                    "severity": diag.severity,
                    "spelling": diag.spelling,
                    "file": self._rel(self._normalize_path(diag.location.file.name)) if diag.location.file else None,
                    "line": diag.location.line if diag.location.file else None,
                }
                for diag in translation_unit.diagnostics
                if diag.severity >= cindex.Diagnostic.Warning
            ]
            self.parse_results.append(
                {
                    "file": self._rel(path),
                    "synthetic_command": synthetic,
                    "diagnostic_count": len(diagnostics),
                    "diagnostics": diagnostics,
                }
            )
            self._walk_cursor(translation_unit.cursor, current_class_usr=None, current_function_usr=None)
        self._write_reports()

    def _collect_compile_database_sources(self) -> list[Path]:
        files = [
            path
            for path in self.compile_args_by_file
            if self._owned_scope_for_rel(self._rel(path)) is not None
        ]
        return sorted(files, key=lambda path: self._rel(path))

    def _collect_unseen_owned_headers(self, seen_files: set[str]) -> list[Path]:
        candidates: list[Path] = []
        for scope in OWNED_PATH_PREFIXES:
            root = self.project_root / scope.rstrip("/")
            if not root.exists():
                continue
            for path in root.rglob("*"):
                if path.is_dir() or not self._is_source_like(path):
                    continue
                resolved = path.resolve()
                relative_path = self._rel(resolved)
                if self._owned_scope_for_rel(relative_path) is None:
                    continue
                if not self._is_header_like(resolved):
                    continue
                if relative_path in seen_files:
                    continue
                candidates.append(resolved)
        return sorted(candidates, key=lambda path: self._rel(path))

    def _is_source_like(self, path: Path) -> bool:
        path_str = str(path)
        return any(path_str.endswith(suffix) for suffix in SOURCE_SUFFIXES)

    def _is_header_like(self, path: Path) -> bool:
        path_str = str(path)
        return not any(path_str.endswith(suffix) for suffix in (".c", ".cc", ".cpp", ".cxx"))

    def _args_for_file(self, path: Path) -> tuple[list[str], bool]:
        args = self.compile_args_by_file.get(path)
        if args is not None:
            return list(args), False
        relative_path = self._rel(path)
        scope = self._owned_scope_for_rel(relative_path)
        donor = self.donor_args_by_scope.get(scope or "", self.generic_donor_args)
        assert donor is not None
        synthetic_args = list(donor)
        if scope == "tests":
            synthetic_args.extend(
                [
                    f"-I{self.project_root / 'tests'}",
                    f"-I{self.project_root / 'libpotassco' / 'third_party' / 'Catch2' / 'src'}",
                ]
            )
        elif scope == "libpotassco/tests":
            synthetic_args.extend(
                [
                    f"-I{self.project_root / 'libpotassco' / 'tests'}",
                    f"-I{self.project_root / 'libpotassco' / 'third_party' / 'Catch2' / 'src'}",
                ]
            )
        if self._is_header_like(path):
            synthetic_args.extend(["-x", "c++-header"])
        return synthetic_args, True

    def _walk_cursor(
        self,
        cursor: cindex.Cursor,
        *,
        current_class_usr: str | None,
        current_function_usr: str | None,
    ) -> None:
        cursor_loc = self._cursor_location(cursor)
        if cursor.kind != cindex.CursorKind.TRANSLATION_UNIT and cursor_loc is None:
            return
        next_class_usr = current_class_usr
        next_function_usr = current_function_usr

        if self._is_entity_cursor(cursor) and cursor_loc is not None and self._classify_path(cursor_loc.file) == "owned":
            entity_usr = self._ensure_entity(cursor)
            if entity_usr is not None:
                usage = self.file_usage[cursor_loc.file]
                usage["declared_entities"].add(entity_usr)
                if cursor.is_definition():
                    usage["defined_entities"].add(entity_usr)
                if self._is_class_cursor(cursor):
                    next_class_usr = entity_usr
                    next_function_usr = None
                elif self._is_function_cursor(cursor):
                    next_function_usr = entity_usr
                    next_class_usr = self.entities[entity_usr]["owner_class_usr"]

        if cursor_loc is not None and self._classify_path(cursor_loc.file) == "owned":
            self._record_reference(cursor, cursor_loc.file, next_class_usr, next_function_usr)

        for child in cursor.get_children():
            self._walk_cursor(child, current_class_usr=next_class_usr, current_function_usr=next_function_usr)

    def _record_reference(
        self,
        cursor: cindex.Cursor,
        source_file: str,
        current_class_usr: str | None,
        current_function_usr: str | None,
    ) -> None:
        relation = None
        if cursor.kind in CALL_KINDS:
            relation = "call"
        elif cursor.kind in REFERENCE_RELATIONS:
            relation = REFERENCE_RELATIONS[cursor.kind]
        else:
            return
        target = self._resolve_reference(cursor.referenced)
        if target is None:
            return
        usage = self.file_usage[source_file]
        if target["scope"] == "owned":
            target_usr = target["usr"]
            usage["used_entities"].add(target_usr)
            target_entity = self.entities[target_usr]
            if target_entity["category"] == "class":
                usage["used_classes"].add(target_usr)
            elif target_entity["owner_class_usr"] is None:
                usage["used_top_level_functions"].add(target_usr)
            else:
                usage["used_methods_or_constructors"].add(target_usr)
            source_usr = current_function_usr or current_class_usr
            if source_usr is not None and source_usr != target_usr:
                self.entity_dependencies[source_usr][target_usr].add(relation)
        elif target["scope"] == "external-project":
            key = target["key"]
            usage["external_project_symbols"][key] = target["payload"]
            source_usr = current_function_usr or current_class_usr
            if source_usr is not None:
                bucket = self.entity_external_dependencies[source_usr].setdefault(key, dict(target["payload"]))
                bucket.setdefault("relations", [])
                if relation not in bucket["relations"]:
                    bucket["relations"].append(relation)

    def _resolve_reference(self, referenced: cindex.Cursor | None) -> dict[str, Any] | None:
        if referenced is None or referenced.kind == cindex.CursorKind.NO_DECL_FOUND:
            return None
        canonical = referenced.canonical
        if canonical.kind == cindex.CursorKind.TRANSLATION_UNIT:
            return None
        target_cursor = canonical
        target_location = self._cursor_location(target_cursor)
        owner_cursor = self._nearest_supported_owner(target_cursor)
        if target_location is None and owner_cursor is not None:
            target_cursor = owner_cursor
            target_location = self._cursor_location(owner_cursor)
        if target_location is None:
            return None
        scope = self._classify_path(target_location.file)
        if scope == "owned":
            if not self._is_entity_cursor(target_cursor) and owner_cursor is not None:
                target_cursor = owner_cursor
            target_usr = self._ensure_entity(target_cursor)
            if target_usr is None:
                return None
            return {"scope": "owned", "usr": target_usr}
        if scope == "external-project":
            payload = {
                "name": self._qualified_name(target_cursor),
                "kind": target_cursor.kind.name,
                "file": target_location.file,
                "line": target_location.line,
            }
            key = f"{payload['kind']}:{payload['name']}@{payload['file']}:{payload['line']}"
            return {"scope": "external-project", "key": key, "payload": payload}
        return None

    def _nearest_supported_owner(self, cursor: cindex.Cursor | None) -> cindex.Cursor | None:
        current = cursor
        while current is not None and current.kind != cindex.CursorKind.TRANSLATION_UNIT:
            if self._is_entity_cursor(current):
                return current
            current = current.semantic_parent
        return None

    def _ensure_entity(self, cursor: cindex.Cursor) -> str | None:
        if not self._is_entity_cursor(cursor):
            return None
        canonical = cursor.canonical
        location = self._cursor_location(canonical) or self._cursor_location(cursor)
        if location is None or self._classify_path(location.file) != "owned":
            return None
        fingerprint = self._entity_fingerprint(canonical, location)
        usr = canonical.get_usr() or self.entity_fingerprints.get(fingerprint)
        if not usr:
            usr = f"synthetic::{fingerprint[0]}::{fingerprint[1]}::{fingerprint[3]}:{fingerprint[4]}"
        existing_usr = self.entity_fingerprints.get(fingerprint)
        if existing_usr is not None:
            usr = existing_usr
        else:
            self.entity_fingerprints[fingerprint] = usr
        owner_class_usr = self._owner_class_usr(canonical)
        entity = self.entities.setdefault(
            usr,
            {
                "usr": usr,
                "kind": canonical.kind.name,
                "category": self._entity_category(canonical),
                "name": self._cursor_name(canonical),
                "qualified_name": self._qualified_name(canonical),
                "display_name": canonical.displayname or self._cursor_name(canonical),
                "signature": getattr(canonical.type, "spelling", "") or canonical.displayname or "",
                "owner_class_usr": owner_class_usr,
                "top_level": owner_class_usr is None and self._is_function_cursor(canonical),
                "files": set(),
                "declarations": set(),
                "definitions": set(),
                "scope": self._scope_label(location.file),
            },
        )
        entity["kind"] = self._prefer_kind(entity["kind"], canonical.kind.name)
        entity["files"].add(location.file)
        if cursor.is_definition():
            entity["definitions"].add(location)
        else:
            entity["declarations"].add(location)
        if owner_class_usr is not None and self._is_function_cursor(canonical):
            self.class_members[owner_class_usr].add(usr)
            self.member_owner[usr] = owner_class_usr
        return usr

    def _prefer_kind(self, current_kind: str, new_kind: str) -> str:
        rank = {
            "CLASS_TEMPLATE": 4,
            "FUNCTION_TEMPLATE": 4,
            "CLASS_DECL": 3,
            "STRUCT_DECL": 3,
            "UNION_DECL": 3,
            "CXX_METHOD": 3,
            "CONSTRUCTOR": 3,
            "DESTRUCTOR": 3,
            "FUNCTION_DECL": 2,
        }
        return new_kind if rank.get(new_kind, 0) > rank.get(current_kind, 0) else current_kind

    def _entity_fingerprint(self, cursor: cindex.Cursor, location: SourceLocation) -> tuple[str, str, str, str, int]:
        return (
            self._entity_category(cursor),
            self._qualified_name(cursor),
            getattr(cursor.type, "spelling", "") or cursor.displayname or "",
            location.file,
            location.line,
        )

    def _entity_category(self, cursor: cindex.Cursor) -> str:
        if self._is_class_cursor(cursor):
            return "class"
        if self._is_function_cursor(cursor):
            return "function"
        raise ValueError(f"Unsupported entity cursor: {cursor.kind}")

    def _is_entity_cursor(self, cursor: cindex.Cursor) -> bool:
        if cursor.kind in CLASS_KINDS:
            return True
        if cursor.kind in FUNCTION_KINDS:
            return True
        return False

    def _is_class_cursor(self, cursor: cindex.Cursor) -> bool:
        return cursor.kind in CLASS_KINDS

    def _is_function_cursor(self, cursor: cindex.Cursor) -> bool:
        return cursor.kind in FUNCTION_KINDS

    def _owner_class_usr(self, cursor: cindex.Cursor) -> str | None:
        current = cursor.semantic_parent
        while current is not None and current.kind != cindex.CursorKind.TRANSLATION_UNIT:
            if self._is_class_cursor(current):
                return self._ensure_entity(current)
            current = current.semantic_parent
        return None

    def _cursor_location(self, cursor: cindex.Cursor) -> SourceLocation | None:
        file_obj = cursor.location.file
        if file_obj is None:
            return None
        normalized_path = self._normalize_path(file_obj.name)
        if normalized_path is None:
            return None
        return SourceLocation(file=self._rel(normalized_path), line=cursor.location.line, column=cursor.location.column)

    def _normalize_path(self, path: str | Path | None) -> Path | None:
        if path is None:
            return None
        candidate = Path(path)
        if not candidate.is_absolute():
            candidate = (self.project_root / candidate).resolve()
        else:
            candidate = candidate.resolve()
        remapped = self.generated_path_map.get(str(candidate))
        if remapped is not None:
            return remapped.resolve()
        try:
            candidate.relative_to(self.project_root)
        except ValueError:
            return None
        return candidate

    def _classify_path(self, relative_path: str) -> str:
        if self._owned_scope_for_rel(relative_path) is not None:
            return "owned"
        if self._external_scope_for_rel(relative_path) is not None:
            return "external-project"
        return "other"

    def _owned_scope_for_rel(self, relative_path: str) -> str | None:
        for prefix in OWNED_PATH_PREFIXES:
            if relative_path.startswith(prefix):
                return prefix.rstrip("/")
        return None

    def _external_scope_for_rel(self, relative_path: str) -> str | None:
        for prefix in EXTERNAL_PROJECT_PATH_PREFIXES:
            if relative_path.startswith(prefix):
                return prefix.rstrip("/")
        return None

    def _scope_label(self, relative_path: str) -> str:
        owned_scope = self._owned_scope_for_rel(relative_path)
        if owned_scope is not None:
            return owned_scope
        external_scope = self._external_scope_for_rel(relative_path)
        if external_scope is not None:
            return external_scope
        parts = Path(relative_path).parts
        return parts[0] if parts else ""

    def _top_dir_for_path(self, path: Path) -> str | None:
        try:
            return path.resolve().relative_to(self.project_root).parts[0]
        except ValueError:
            return None

    def _top_dir_for_rel(self, relative_path: str) -> str | None:
        parts = Path(relative_path).parts
        return parts[0] if parts else None

    def _rel(self, path: Path | None) -> str:
        if path is None:
            return ""
        return path.resolve().relative_to(self.project_root).as_posix()

    def _cursor_name(self, cursor: cindex.Cursor) -> str:
        if cursor.spelling:
            return cursor.spelling
        location = self._cursor_location(cursor)
        if cursor.kind == cindex.CursorKind.NAMESPACE:
            return "<anonymous_namespace>"
        if location is not None:
            return f"<anonymous@{location.file}:{location.line}>"
        return "<anonymous>"

    def _qualified_name(self, cursor: cindex.Cursor) -> str:
        parts: list[str] = []
        current = cursor.semantic_parent
        while current is not None and current.kind != cindex.CursorKind.TRANSLATION_UNIT:
            if current.kind == cindex.CursorKind.NAMESPACE:
                parts.append(current.spelling or "<anonymous_namespace>")
            elif self._is_entity_cursor(current):
                parts.append(self._cursor_name(current))
            current = current.semantic_parent
        parts.reverse()
        parts.append(self._cursor_name(cursor))
        return "::".join(part for part in parts if part)

    def _sorted_locations(self, locations: set[SourceLocation]) -> list[dict[str, Any]]:
        return [
            location.as_dict()
            for location in sorted(locations, key=lambda loc: (loc.file, loc.line, loc.column))
        ]

    def _primary_location(self, entity: dict[str, Any]) -> dict[str, Any] | None:
        locations = self._sorted_locations(entity["definitions"] or entity["declarations"])
        return locations[0] if locations else None

    def _serialize_entity(self, entity: dict[str, Any]) -> dict[str, Any]:
        return {
            "usr": entity["usr"],
            "kind": entity["kind"],
            "category": entity["category"],
            "name": entity["name"],
            "qualified_name": entity["qualified_name"],
            "display_name": entity["display_name"],
            "signature": entity["signature"],
            "owner_class_usr": entity["owner_class_usr"],
            "top_level": entity["top_level"],
            "scope": entity["scope"],
            "files": sorted(entity["files"]),
            "declarations": self._sorted_locations(entity["declarations"]),
            "definitions": self._sorted_locations(entity["definitions"]),
            "primary_location": self._primary_location(entity),
            "member_usrs": sorted(self.class_members.get(entity["usr"], set())),
        }

    def _build_port_units(self) -> tuple[dict[str, dict[str, Any]], list[list[str]]]:
        units: dict[str, dict[str, Any]] = {}
        for usr, entity in self.entities.items():
            if entity["category"] == "class":
                if not self._is_port_unit_entity(entity):
                    continue
                units[usr] = {
                    "usr": usr,
                    "unit_type": "class",
                    "name": entity["qualified_name"],
                    "entity_usrs": {usr, *self.class_members.get(usr, set())},
                }
            elif entity["category"] == "function" and entity["owner_class_usr"] is None:
                if not self._is_port_unit_entity(entity):
                    continue
                units[usr] = {
                    "usr": usr,
                    "unit_type": "function",
                    "name": entity["qualified_name"],
                    "entity_usrs": {usr},
                }

        def port_unit_for_entity(entity_usr: str) -> str | None:
            entity = self.entities[entity_usr]
            if entity["category"] == "class":
                return entity_usr if entity_usr in units else None
            owner = entity["owner_class_usr"]
            if owner is not None:
                return owner if owner in units else None
            if entity_usr in units:
                return entity_usr
            return None

        for unit_usr, unit in units.items():
            internal_dependencies: dict[str, set[str]] = defaultdict(set)
            external_dependencies: dict[str, dict[str, Any]] = {}
            for entity_usr in unit["entity_usrs"]:
                for target_usr, relations in self.entity_dependencies.get(entity_usr, {}).items():
                    target_unit = port_unit_for_entity(target_usr)
                    if target_unit is None or target_unit == unit_usr:
                        continue
                    internal_dependencies[target_unit].update(relations)
                for key, payload in self.entity_external_dependencies.get(entity_usr, {}).items():
                    bucket = external_dependencies.setdefault(key, dict(payload))
                    bucket.setdefault("relations", [])
                    for relation in payload.get("relations", []):
                        if relation not in bucket["relations"]:
                            bucket["relations"].append(relation)
            unit["internal_dependencies"] = internal_dependencies
            unit["external_dependencies"] = external_dependencies
            unit["primary_location"] = self._primary_location(self.entities[unit_usr])
            unit["files"] = sorted({file for entity_usr in unit["entity_usrs"] for file in self.entities[entity_usr]["files"]})

        sccs = self._strongly_connected_components({usr: set(data["internal_dependencies"].keys()) for usr, data in units.items()})
        return units, sccs

    def _is_port_unit_entity(self, entity: dict[str, Any]) -> bool:
        qualified_name = entity["qualified_name"]
        if "(lambda at " in qualified_name:
            return False
        if "(anonymous " in qualified_name:
            return False
        if "<anonymous@" in qualified_name:
            return False
        return True

    def _strongly_connected_components(self, graph: dict[str, set[str]]) -> list[list[str]]:
        index = 0
        stack: list[str] = []
        on_stack: set[str] = set()
        indices: dict[str, int] = {}
        lowlinks: dict[str, int] = {}
        components: list[list[str]] = []

        def strongconnect(node: str) -> None:
            nonlocal index
            indices[node] = index
            lowlinks[node] = index
            index += 1
            stack.append(node)
            on_stack.add(node)
            for neighbor in graph.get(node, set()):
                if neighbor not in indices:
                    strongconnect(neighbor)
                    lowlinks[node] = min(lowlinks[node], lowlinks[neighbor])
                elif neighbor in on_stack:
                    lowlinks[node] = min(lowlinks[node], indices[neighbor])
            if lowlinks[node] == indices[node]:
                component: list[str] = []
                while True:
                    member = stack.pop()
                    on_stack.remove(member)
                    component.append(member)
                    if member == node:
                        break
                components.append(sorted(component, key=lambda usr: self.entities[usr]["qualified_name"]))

        for node in sorted(graph, key=lambda usr: self.entities[usr]["qualified_name"]):
            if node not in indices:
                strongconnect(node)
        return components

    def _topological_layers(self, units: dict[str, dict[str, Any]], sccs: list[list[str]]) -> list[list[str]]:
        component_index: dict[str, int] = {}
        for index, component in enumerate(sccs):
            for member in component:
                component_index[member] = index
        condensed_edges: dict[int, set[int]] = defaultdict(set)
        indegree: dict[int, int] = {index: 0 for index in range(len(sccs))}
        for member, unit in units.items():
            source_component = component_index[member]
            for dependency in unit["internal_dependencies"].keys():
                target_component = component_index[dependency]
                if source_component == target_component or target_component in condensed_edges[source_component]:
                    continue
                condensed_edges[source_component].add(target_component)
                indegree[target_component] += 1
        ready = deque(sorted((index for index, degree in indegree.items() if degree == 0), key=lambda idx: sccs[idx]))
        layers: list[list[str]] = []
        while ready:
            current_layer_components = list(ready)
            ready.clear()
            layer: list[str] = []
            for component_id in current_layer_components:
                layer.extend(sccs[component_id])
            layers.append(sorted(layer, key=lambda usr: self.entities[usr]["qualified_name"]))
            for component_id in current_layer_components:
                for neighbor in condensed_edges.get(component_id, set()):
                    indegree[neighbor] -= 1
                    if indegree[neighbor] == 0:
                        ready.append(neighbor)
            ready = deque(sorted(ready))
        return layers

    def _write_reports(self) -> None:
        self.output_dir.mkdir(parents=True, exist_ok=True)
        generated_at = datetime.now(UTC).isoformat()
        serialized_entities = sorted(
            (self._serialize_entity(entity) for entity in self.entities.values()),
            key=lambda entity: (entity["primary_location"] or {"file": "", "line": 0})["file"],
        )
        inventory = {
            "generated_at": generated_at,
            "analysis_method": {
                "parser": "libclang",
                "compile_database": self._rel(self.compile_commands_path),
                "compiler_normalization": {
                    "gcc_install_dir": GCC_INSTALL_DIR,
                    "extra_include_dir": GCC_INCLUDE_DIR,
                    "removed_warning_flags": "all -W* except -Wno-deprecated-declarations",
                },
                "scope": {
                    "owned_path_prefixes": list(OWNED_PATH_PREFIXES),
                    "external_project_path_prefixes": list(EXTERNAL_PROJECT_PATH_PREFIXES),
                },
            },
            "parse_results": {
                "successful_files": len(self.parse_results),
                "skipped_files": self.skipped_files,
                "per_file": self.parse_results,
            },
            "entities": sorted(
                serialized_entities,
                key=lambda entity: (
                    (entity["primary_location"] or {"file": "", "line": 0, "column": 0})["file"],
                    (entity["primary_location"] or {"file": "", "line": 0, "column": 0})["line"],
                    entity["qualified_name"],
                    entity["signature"],
                ),
            ),
        }
        file_usage_report = {
            "generated_at": generated_at,
            "files": [self._serialize_file_usage(path, usage) for path, usage in sorted(self.file_usage.items())],
        }
        units, sccs = self._build_port_units()
        dependency_report = {
            "generated_at": generated_at,
            "entity_dependencies": self._serialize_entity_dependencies(),
            "port_units": self._serialize_port_units(units),
            "strongly_connected_components": [self._serialize_component(component, units) for component in sccs],
        }
        layers = self._topological_layers(units, sccs)
        porting_order = {
            "generated_at": generated_at,
            "method": "Topological layers over SCC-compressed internal dependencies. Class methods are folded into their owning class. External project dependencies are reported but excluded from layer computation.",
            "batches": [self._serialize_batch(index, batch, units) for index, batch in enumerate(layers)],
            "cycles": [
                self._serialize_component(component, units, include_ported=True)
                for component in sccs
                if len(component) > 1
            ],
        }
        summary = self._render_summary(inventory, units, porting_order)

        (self.output_dir / "symbol_inventory.json").write_text(json.dumps(inventory, indent=2))
        (self.output_dir / "file_usage.json").write_text(json.dumps(file_usage_report, indent=2))
        (self.output_dir / "dependencies.json").write_text(json.dumps(dependency_report, indent=2))
        (self.output_dir / "porting_order.json").write_text(json.dumps(porting_order, indent=2))
        (self.output_dir / "README.md").write_text(summary)

    def _serialize_entity_dependencies(self) -> list[dict[str, Any]]:
        rows: list[dict[str, Any]] = []
        for usr, entity in sorted(self.entities.items(), key=lambda item: item[1]["qualified_name"]):
            internal = [
                {
                    "target_usr": target_usr,
                    "target_name": self.entities[target_usr]["qualified_name"],
                    "target_kind": self.entities[target_usr]["kind"],
                    "relations": sorted(relations),
                }
                for target_usr, relations in sorted(self.entity_dependencies.get(usr, {}).items(), key=lambda item: self.entities[item[0]]["qualified_name"])
                if target_usr in self.entities
            ]
            external = sorted(self.entity_external_dependencies.get(usr, {}).values(), key=lambda item: (item["file"], item["line"], item["name"]))
            rows.append(
                {
                    "source_usr": usr,
                    "source_name": entity["qualified_name"],
                    "source_kind": entity["kind"],
                    "internal_dependencies": internal,
                    "external_project_dependencies": external,
                }
            )
        return rows

    def _serialize_port_units(self, units: dict[str, dict[str, Any]]) -> list[dict[str, Any]]:
        rows: list[dict[str, Any]] = []
        for usr, unit in sorted(units.items(), key=lambda item: self.entities[item[0]]["qualified_name"]):
            rows.append(
                {
                    "usr": usr,
                    "unit_type": unit["unit_type"],
                    "name": unit["name"],
                    "primary_location": unit["primary_location"],
                    "files": unit["files"],
                    "entity_usrs": sorted(unit["entity_usrs"]),
                    "internal_dependencies": [
                        {
                            "target_usr": target_usr,
                            "target_name": self.entities[target_usr]["qualified_name"],
                            "relations": sorted(relations),
                        }
                        for target_usr, relations in sorted(unit["internal_dependencies"].items(), key=lambda item: self.entities[item[0]]["qualified_name"])
                    ],
                    "external_project_dependencies": sorted(unit["external_dependencies"].values(), key=lambda item: (item["file"], item["line"], item["name"])),
                }
            )
        return rows

    def _serialize_component(
        self,
        component: list[str],
        units: dict[str, dict[str, Any]],
        include_ported: bool = False,
    ) -> dict[str, Any]:
        return {
            "members": [
                {
                    "usr": usr,
                    "name": units[usr]["name"],
                    "unit_type": units[usr]["unit_type"],
                    "primary_location": units[usr]["primary_location"],
                    **({"ported": False} if include_ported else {}),
                }
                for usr in component
            ]
        }

    def _serialize_batch(self, layer_index: int, batch: list[str], units: dict[str, dict[str, Any]]) -> dict[str, Any]:
        return {
            "layer": layer_index,
            "entities": [
                {
                    "usr": usr,
                    "name": units[usr]["name"],
                    "unit_type": units[usr]["unit_type"],
                    "ported": False,
                    "primary_location": units[usr]["primary_location"],
                    "files": units[usr]["files"],
                    "depends_on_internal": [
                        {
                            "usr": target_usr,
                            "name": self.entities[target_usr]["qualified_name"],
                            "relations": sorted(relations),
                        }
                        for target_usr, relations in sorted(units[usr]["internal_dependencies"].items(), key=lambda item: self.entities[item[0]]["qualified_name"])
                    ],
                    "depends_on_external_project": sorted(units[usr]["external_dependencies"].values(), key=lambda item: (item["file"], item["line"], item["name"])),
                }
                for usr in batch
            ],
        }

    def _serialize_file_usage(self, relative_path: str, usage: dict[str, Any]) -> dict[str, Any]:
        def summarize(usrs: set[str]) -> list[dict[str, Any]]:
            return [
                {
                    "usr": usr,
                    "name": self.entities[usr]["qualified_name"],
                    "kind": self.entities[usr]["kind"],
                    "primary_location": self._primary_location(self.entities[usr]),
                }
                for usr in sorted(usrs, key=lambda value: self.entities[value]["qualified_name"])
            ]

        return {
            "file": relative_path,
            "scope": self._scope_label(relative_path),
            "declared_entities": summarize(usage["declared_entities"]),
            "defined_entities": summarize(usage["defined_entities"]),
            "used_entities": summarize(usage["used_entities"]),
            "used_classes": summarize(usage["used_classes"]),
            "used_top_level_functions": summarize(usage["used_top_level_functions"]),
            "used_methods_or_constructors": summarize(usage["used_methods_or_constructors"]),
            "external_project_symbols": sorted(usage["external_project_symbols"].values(), key=lambda item: (item["file"], item["line"], item["name"])),
        }

    def _render_summary(self, inventory: dict[str, Any], units: dict[str, dict[str, Any]], porting_order: dict[str, Any]) -> str:
        entity_count = len(inventory["entities"])
        class_count = sum(1 for entity in inventory["entities"] if entity["category"] == "class")
        function_count = entity_count - class_count
        scope_counts: dict[str, int] = defaultdict(int)
        for entity in inventory["entities"]:
            scope_counts[entity["scope"]] += 1
        lines = [
            "# Original Clasp Analysis",
            "",
            "## Method",
            "",
            "Parser: libclang",
            f"Compile database: {self._rel(self.compile_commands_path)}",
            f"Included scopes: {', '.join(prefix.rstrip('/') for prefix in OWNED_PATH_PREFIXES)}",
            f"Excluded from inventory and port-order graph: {', '.join(prefix.rstrip('/') for prefix in EXTERNAL_PROJECT_PATH_PREFIXES)}",
            "Generated build/config header is remapped back to clasp/config.h.in for reporting.",
            "",
            "## Coverage",
            "",
            f"Entities captured: {entity_count}",
            f"Class-like entities: {class_count}",
            f"Function-like entities: {function_count}",
            f"Parsed files: {inventory['parse_results']['successful_files']}",
            f"Skipped files: {len(inventory['parse_results']['skipped_files'])}",
            f"File-usage entries: {len(self.file_usage)}",
            f"Port units: {len(units)}",
            f"Port-order layers: {len(porting_order['batches'])}",
            f"Dependency cycles: {len(porting_order['cycles'])}",
            "",
            "## Entity Counts By Scope",
            "",
        ]
        for scope, count in sorted(scope_counts.items()):
            lines.append(f"{scope}: {count}")
        lines.extend(
            [
                "",
                "## Notes",
                "",
                "The symbol inventory and dependency graph include app, examples, original tests, and libpotassco.",
                "Test scopes include test-local helpers and Catch2-generated internal symbols because the request was to avoid missing classes and functions.",
                "The port-order graph excludes lambda and anonymous implementation artifacts, but it intentionally keeps named test symbols.",
                "",
            ]
        )
        lines.extend(
            [
                "## Files",
                "",
                "symbol_inventory.json: every discovered class/function-like entity with file and line information.",
                "file_usage.json: per-file declarations, definitions, and symbol usage.",
                "dependencies.json: entity-level dependencies and port-unit dependency edges.",
                "porting_order.json: dependency-layered porting batches for the widened scope.",
                "",
            ]
        )
        return "\n".join(lines)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Analyze original_clasp symbols and dependencies with libclang.")
    parser.add_argument(
        "--project-root",
        default="original_clasp",
        help="Path to the original clasp C++ source tree.",
    )
    parser.add_argument(
        "--build-dir",
        default="original_clasp/build-analysis-full",
        help="Path to the configured CMake build directory containing compile_commands.json.",
    )
    parser.add_argument(
        "--output-dir",
        default="analysis/original_clasp",
        help="Where to write the generated reports.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    analyzer = Analyzer(
        project_root=Path(args.project_root),
        build_dir=Path(args.build_dir),
        output_dir=Path(args.output_dir),
    )
    analyzer.run()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())