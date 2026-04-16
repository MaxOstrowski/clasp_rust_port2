Your task is to port `original_clasp` to Rust.

The C++ code under `original_clasp/` is the behavioral reference. Preserve original behavior as closely as possible. Prefer exact parity unless Rust requires a structural change or an established Rust library is a clearly better fit for the same job.

## Source Of Truth

- Use `analysis/original_clasp/porting_order.json` as the primary source for porting order.
- Use `analysis/original_clasp/README.md` to understand what the analysis includes.
- Use `original_clasp/` as the implementation and behavior reference.
- Use original tests under `original_clasp/tests/` and `original_clasp/libpotassco/tests/` as the reference test suite to port.

## How To Read `porting_order.json`

The porting order is more detailed than file-level planning. It is an entity graph.

- `batches` are topological layers. Prefer lower layers before higher layers.
- Each entry in a batch is a port unit such as a class or function.
- `depends_on_internal` lists prerequisites inside the analyzed codebase.
- `depends_on_external_project` lists dependencies outside the current internal ordering. Treat these as external references that still need to exist on the Rust side if they are required.
- `ported` is the status field. When an entity is fully ported, set it to `true`.
- `cycles` contains strongly connected components. Members of one cycle should usually be ported together as a small unit.

Do not treat one JSON entry as meaning one Rust file. The order file is intentionally fine-grained. It tells you dependency order, not the final Rust module boundaries.

## Porting Workflow

For each task:

1. Find the target entity or entities in `analysis/original_clasp/porting_order.json`.
2. Confirm all internal prerequisites are already ported.
3. If prerequisites are not ported, stop and warn clearly.
4. Inspect the corresponding C++ header, source, and original tests.
5. Port the smallest coherent Rust unit that preserves behavior.
6. Add thorough Rust tests in a dedicated test file.
7. Port matching original tests when they exist.
8. Update `ported` flags for the completed entities.

When the order file is very detailed, work like this:

- If several helper functions and a class form one cohesive implementation unit, port them together.
- If a C++ file contains multiple unrelated entities, split them into multiple Rust modules.
- If one C++ class naturally maps to several Rust types or modules, that is acceptable.
- If an item in `cycles` is tightly coupled, port the whole cycle in one change.

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
- Warn when a missing prerequisite from `porting_order.json` would force guessing.
- Do not silently skip original tests.
- After completing a ported entity, mark it as ported in `analysis/original_clasp/porting_order.json`.

## Summary Rule

Use `porting_order.json` for order, use `original_clasp/` for behavior, and use a clean Rust module structure for the final code layout.