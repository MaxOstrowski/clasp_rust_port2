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

As of Bundle A refactor, `clasp::solver_types` re-exports `StatisticObject`/`StatisticType` from `clasp::statistics` and implements `StatisticMap` for `CoreStats`, `JumpStats`, `ExtendedStats`, and `SolverStats`.

### Decision A3: Extract only `ProblemStats` from `shared_context`

- **Why**: Upstream `clasp/statistics.h` mentions `ProblemStats`, but the full `SharedContext` runtime is still blocked on solver/clause integration. Treating all of `shared_context` as a prerequisite would keep the statistics port artificially blocked.
- **Design**: Port the standalone `ProblemStats` aggregate and its `StatisticObject` map interface into `clasp::shared_context` now, while leaving the rest of `SharedContext` as a later runtime task.
- **Constraint**: `clasp::statistics` depends only on the lightweight `ProblemStats` data type, not on `SharedContext` runtime behavior.
- **Outcome**: `ClaspStatistics` and `StatsVisitor` can be ported faithfully without pulling in solver attach/detach, clause lifecycle, or initialization flows.

### Decision A2: Incremental solver port keeps the current placeholder `constraint::Solver`

- **Why**: The repository currently contains a minimal `Solver` in `clasp::constraint` used by already-ported code/tests. Replacing it with the full upstream solver is a large change that is best done after the cycle-breaking seams are in place.
- **Implication**: Bundle A work is split into (1) cycle-breaking refactors and (2) later replacement of the placeholder solver with the full solver kernel.

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
