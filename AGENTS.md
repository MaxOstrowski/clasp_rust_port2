Your task is to port `original_clasp` to Rust.

The C++ code under `original_clasp/` is the behavioral reference. Preserve original behavior as closely as possible. Prefer exact parity unless Rust requires a structural change or an established Rust library is a clearly better fit for the same job.

## Source Of Truth

- Use `analysis/original_clasp/source_tree_by_import_order.txt` as the primary worklist and progress tracker.
- Use `analysis/original_clasp/porting_order.json` only as supplemental dependency detail when the file-level order is not sufficient.
- Use `analysis/original_clasp/README.md` to understand what the analysis includes.
- Use `original_clasp/` as the implementation and behavior reference.
- Use original tests under `original_clasp/tests/` and `original_clasp/libpotassco/tests/` as the reference test suite to port.

## How To Read `source_tree_by_import_order.txt`

The source tree file is a file-level worklist ordered by import dependencies.

- Each non-indented line names one original C++ header, source, or test file in dependency order.
- A suffix like `: ported` or `: partially ported, ...` records progress for that original file.
- A suffix like `: blocked by x, y` records a file that cannot yet be ported honestly because prerequisite source-tree entries are still not ported enough.
- Indented lines below an entry list the Rust files and tests that currently implement it.
- If one original C++ file is split across several Rust modules, list all corresponding Rust paths under the same entry.

Do not treat one source-tree entry as meaning one Rust file. It is acceptable to map one original C++ file to several Rust files when that keeps the Rust port readable and maintainable.

## Porting Workflow

For each task:

1. Find the next target file or coherent file group in `analysis/original_clasp/source_tree_by_import_order.txt`.
2. Confirm earlier dependency entries that the target relies on are already ported enough for the change. If the file-level order is ambiguous, consult `analysis/original_clasp/porting_order.json` as a secondary reference.
3. If prerequisites are not ported, update the source-tree entry to `: blocked by x, y` using the prerequisite source-tree entries, warn clearly, and then continue with the next unported entry unless the user explicitly asks you to stop at blockers.
4. Inspect the corresponding C++ header, source, and original tests.
5. Port the smallest coherent Rust unit that preserves behavior.
6. Add thorough Rust tests in a dedicated test file.
7. Port matching original tests when they exist.
8. Update the matching entry in `analysis/original_clasp/source_tree_by_import_order.txt` with its current status and the Rust files that now cover it.

When a source entry expands during porting, work like this:

- If several helper functions and a class form one cohesive implementation unit, port them together.
- If a C++ file contains multiple unrelated entities, split them into multiple Rust modules.
- If one original C++ file naturally maps to several Rust types or modules, that is acceptable.
- If one target Rust file becomes too large, split it into several Rust files by responsibility and list all of them under the same source-tree entry.

## Rules For Parity

- Match original behavior exactly, ask for permission if this should not be possible.
- Keep API semantics, edge cases, invariants, and algorithmic behavior aligned with C++.
- If Rust requires a different expression of the design, keep observable behavior the same.
- You may replace hand-written low-level infrastructure with established Rust libraries when the replacement fits the intent of the original design.
- When choosing a library, prefer one with similar semantics to the original implementation. For example, preserve lock-free or low-allocation characteristics when those properties matter. Also cache-efficiency is an important factor.

## Rust Project Structure

There is no need to mirror the C++ file layout 1:1. The Rust port should be organized for readability while keeping traceability back to the original code.

Use this structure as the default description of the port:

- Organize the Rust code by subsystem, not by translation unit.
- Keep public surface areas close to the original conceptual modules such as parser, solver, logic program, constraints, dependency graph, and utilities.
- Within a subsystem, split large C++ files into several Rust modules by responsibility.
- Keep each Rust source file below roughly 800 lines when practical. The only acceptable exception is a file centered on one large type where splitting would make the code harder to understand.
- Put reusable helpers into dedicated support modules instead of hiding them in oversized files.
- Keep a clear mapping in comments from Rust code back to the original C++ source being ported.

## Test Structure

- Never place tests next to source, use a separate file.
- Port original tests whenever the original code has them.
- Keep ported tests separate from handwritten Rust-only tests, e.g.
tests/ 
- Test all implemented methods thoroughly, including edge cases and behavior-sensitive scenarios.

Recommended layout once the Rust crate exists:

- production code under `src/`
- handwritten Rust integration tests under `tests/`
- ported original tests under `tests/ported/`

Current bootstrap layout in this repository:

- `Cargo.toml` defines the root Rust crate `rust_clasp`
- `src/lib.rs` is the current Rust library entry point
- `tests/ported/` is reserved for translated upstream tests

## Rust Tooling

Run all Rust tooling from the repository root.

- Format in place with `cargo fmt --all`
- Check formatting without changing files with `cargo fmt --all -- --check`
- Run linting with `cargo clippy --workspace --all-targets -- -D warnings`
- Run the Rust test suite with `cargo test --workspace`

Before finishing a Rust change, run formatting, clippy, and the relevant tests unless the environment prevents it. If a command cannot be run, say so clearly.

## Practical Expectations

- Be thorough.
- Prefer small, dependency-respecting increments.
- Warn when a requested port violates the dependency order.
- Warn when the source-tree order and supplemental dependency data still leave prerequisites unclear enough that continuing would require guessing.
- Do not silently skip original tests.
- After completing work on an original file, update its status and Rust file list in `analysis/original_clasp/source_tree_by_import_order.txt`.

## Summary Rule

Use `source_tree_by_import_order.txt` for work order and progress tracking, use `original_clasp/` for behavior, and use a clean Rust module structure for the final code layout.