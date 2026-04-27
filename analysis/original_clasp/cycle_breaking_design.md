# Cycle-breaking design notes

This document records intentional dependency-boundary decisions made while porting `original_clasp` to Rust.

Goal: keep behavior parity while avoiding cyclic Rust module dependencies that would otherwise force large, all-at-once ports.

## Bundle A: Solver kernel (statistics / solver_types / clause / solver / shared_context)

### Decision A1: Type-erased `StatisticObject` lives in `clasp::statistics`

- **Why**: Upstream `clasp/statistics.h` is (mostly) solver-agnostic. In Rust, putting the statistics object model in `solver_types` forces an import direction `solver_types -> statistics` (or the reverse) that quickly becomes cyclic once other subsystems also want to export statistics.
- **Design**: Implement a type-erased `StatisticObject` backed by a small vtable (function pointers) and an `*const ()` payload, mirroring the upstream C++ layout.
- **Detail**: `StatisticObject` supports `InlineValue(f64)` to represent derived scalar statistics without allocating/boxing temporaries just to satisfy type-erasure.
- **Constraint**: `clasp::statistics` must not depend on solver/runtime types.
- **Compatibility**: `clasp::solver_types` may re-export `StatisticObject`/`StatisticType` to keep the current public API stable during the transition.
- **Implementation note**: `clasp::statistics` now stays as the stable public facade while the internals are split into `src/clasp/statistics/object.rs` for the type-erased object model and `src/clasp/statistics/store.rs` for `StatsVisitor` plus the writable/external `ClaspStatistics` store.

As of Bundle A refactor, `clasp::solver_types` re-exports `StatisticObject`/`StatisticType` from `clasp::statistics` and implements `StatisticMap` for `CoreStats`, `JumpStats`, `ExtendedStats`, and `SolverStats`.

### Decision A3: Extract only `ProblemStats` from `shared_context`

- **Why**: Upstream `clasp/statistics.h` mentions `ProblemStats`, but the full `SharedContext` runtime is still blocked on solver/clause integration. Treating all of `shared_context` as a prerequisite would keep the statistics port artificially blocked.
- **Design**: Port the standalone `ProblemStats` aggregate and its `StatisticObject` map interface into `clasp::shared_context` now, while leaving the rest of `SharedContext` as a later runtime task.
- **Constraint**: `clasp::statistics` depends only on the lightweight `ProblemStats` data type, not on `SharedContext` runtime behavior.
- **Outcome**: `ClaspStatistics` and `StatsVisitor` can be ported faithfully without pulling in solver attach/detach, clause lifecycle, or initialization flows.

### Decision A2: Incremental solver port keeps the current placeholder `constraint::Solver`

- **Why**: The repository currently contains a minimal `Solver` in `clasp::constraint` used by already-ported code/tests. Replacing it with the full upstream solver is a large change that is best done after the cycle-breaking seams are in place.
- **Implication**: Bundle A work is split into (1) cycle-breaking refactors and (2) later replacement of the placeholder solver with the full solver kernel.

### Decision A4: Bundle A is ported as a split runtime, not as one monolithic file-for-file translation

- **Why**: The upstream Bundle A files are a single SCC in practice, but they are too large to port honestly into single Rust files without recreating the same cycle pressure. The upstream units also mix several different responsibilities: statistics, assignment/reason packing, watch storage, clause runtime, solver search state, and shared runtime services.
- **Design**: Keep the public Rust module names aligned with upstream concepts (`clasp::solver_types`, `clasp::clause`, `clasp::solver`, `clasp::shared_context`), but split the implementation into smaller support modules under `src/clasp/` as needed.

- **Implemented split**:
	- `clasp::solver_types`: statistics aggregates, reason/data stores, `ValueSet`, `Assignment`, watch types, `ImpliedLiteral`, `ImpliedList`, and `ClauseHead` support types.
	  The public module now delegates to `src/clasp/solver_types/stats.rs` for `CoreStats`/`JumpStats`/`ExtendedStats`/`SolverStats` and to `src/clasp/solver_types/runtime.rs` for the assignment, reason-store, watch, implied-literal, and `SolverSet` runtime helpers.
	- `clasp::clause`: `SharedLiterals`, `ClauseCreator`, explicit clause runtime (`Clause`), and loop-formula/shared-clause adapters when needed.
	- `clasp::solver`: assignment/watch kernel, propagation queue, trail/backtrack/root-level handling, clause/constraint attachment, conflict storage, and solver-owned runtime state.
	- `clasp::shared_context`: `VarInfo`, short implication graph, problem/shared statistics plumbing, solver ownership/attachment, and the subset of context runtime directly required by Bundle A.
- **Constraint**: `clasp::constraint` remains the home of generic `Constraint`, `PostPropagator`, `Antecedent`, and score/info types, but `Solver` is treated as a re-exported runtime type once the real solver kernel lands.

### Decision A5: Preserve downstream compatibility with temporary re-exports while the kernel moves

- **Why**: Many already-ported modules and tests currently import `Solver` and related types from `clasp::constraint` or `claspfwd`. A hard cutover would turn Bundle A into a repo-wide rename instead of a faithful kernel port.
- **Design**: Keep downstream call sites stable by re-exporting the real solver runtime through existing paths during the transition.
- **Compatibility rules**:
	- `clasp::constraint::Solver` re-exports `clasp::solver::Solver` after the runtime lands.
	- `claspfwd` continues to re-export `Constraint`, `ConstraintInfo`, and `Solver` without changing its public shape.
	- New helper modules stay internal unless an upstream-visible type requires direct exposure.

### Decision A6: Bundle A implementation order follows the low-level runtime seam

- **Why**: Porting clause and solver behavior directly without first porting assignment/reason/watch storage would either duplicate logic or force placeholder APIs that diverge from upstream.
- **Implementation order**:
	1. Port `solver_types` low-level runtime pieces: `ReasonStore32/64`, `ValueSet`, `Assignment`, watch types, `ImpliedLiteral`, and `ImpliedList`.
	2. Add the minimal `shared_context` runtime pieces required by the solver kernel: `VarInfo`, solver ownership hooks, and short implication storage.
	3. Replace the placeholder solver with a real `clasp::solver` kernel and keep compatibility re-exports for existing modules.
	4. Finish `ClauseCreator` creation/integration paths and the explicit clause runtime on top of the new solver kernel.
	5. Port the upstream Bundle A tests that exercise these runtime paths and update the tracker entries for the original Bundle A files.
- **Status**: The Bundle A runtime slice defined by steps 1-5 is now landed. Original Bundle A source-tree entries remain marked `partially ported` where the upstream files still contain non-Bundle-A responsibilities that are intentionally deferred to later bundles.

## Bundle B: Program stack (parser / program_builder / logic_program / dependency_graph)

### Decision B1: `detect_problem_type` and `ProgramParser` are parser-framework only

- **Why**: The upstream `clasp/parser.h` provides generic parsing infrastructure (`ProgramParser`, `ParserOptions`, `detectProblemType`) that should not depend on `LogicProgram`/`SharedContext`.
- **Design**: Implement the framework pieces in `clasp::parser` using libpotassco’s `ProgramReader` as the underlying strategy, but keep concrete readers (ASP/SAT/PB) behind trait objects so `clasp::parser` does not import `clasp::logic_program`.

Implementation notes:
- Upstream incremental behavior (`parse()` + `more()`) is mapped to libpotassco `ProgramReader::parse(ReadMode::Incremental)` so each `parse()` consumes one step.
- `clasp::parser` depends only on potassco readers and `claspfwd::ProblemType`; it does not depend on `clasp::logic_program`.

### Decision B2: `ProgramBuilder` is introduced as an API boundary

- **Why**: Upstream keeps the builder/parser split with virtual hooks (`doStartProgram`, `doCreateParser`, ...). In Rust we preserve this as a trait boundary to avoid `parser <-> logic_program` cycles.
- **Design**: `clasp::program_builder` defines the trait surface and stores minimal state; concrete builders (e.g., ASP `LogicProgram`) live in their own modules and depend on the program_builder API, not the other way around.

## Notes

- These decisions are intended to be revisited once the solver kernel is ported far enough that cyclic dependencies can be eliminated naturally by stable module layering.
