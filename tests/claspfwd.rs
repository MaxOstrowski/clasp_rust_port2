use std::any::TypeId;

use rust_clasp::clasp::claspfwd::{
    Asp, Configuration, Constraint, ConstraintInfo, ExtDepGraph, MinimizeBuilder, Model, PBBuilder,
    ProblemType, ProgramBuilder, ProgramParser, SatBuilder, SharedContext, SharedMinimizeData,
    Solver,
};

#[test]
fn exposes_core_forward_declared_types() {
    let ids = [
        TypeId::of::<SharedContext>(),
        TypeId::of::<MinimizeBuilder>(),
        TypeId::of::<SharedMinimizeData>(),
        TypeId::of::<Configuration>(),
        TypeId::of::<Constraint>(),
        TypeId::of::<ConstraintInfo>(),
        TypeId::of::<Solver>(),
        TypeId::of::<Model>(),
        TypeId::of::<ProgramBuilder>(),
        TypeId::of::<ProgramParser>(),
        TypeId::of::<SatBuilder>(),
        TypeId::of::<PBBuilder>(),
        TypeId::of::<ExtDepGraph>(),
    ];

    assert_eq!(ids.len(), 13);
}

#[test]
fn exposes_asp_forward_declared_types() {
    let ids = [
        TypeId::of::<Asp::LogicProgram>(),
        TypeId::of::<Asp::Preprocessor>(),
        TypeId::of::<Asp::LpStats>(),
        TypeId::of::<Asp::PrgAtom>(),
        TypeId::of::<Asp::PrgBody>(),
        TypeId::of::<Asp::PrgDisj>(),
        TypeId::of::<Asp::PrgHead>(),
        TypeId::of::<Asp::PrgNode>(),
        TypeId::of::<Asp::PrgDepGraph>(),
        TypeId::of::<Asp::PrgEdge>(),
    ];

    assert_eq!(ids.len(), 10);
}

#[test]
fn preserves_problem_type_discriminants() {
    assert_eq!(ProblemType::Sat.as_u32(), 0);
    assert_eq!(ProblemType::Pb.as_u32(), 1);
    assert_eq!(ProblemType::Asp.as_u32(), 2);

    assert_eq!(ProblemType::from_u32(0), Some(ProblemType::Sat));
    assert_eq!(ProblemType::from_u32(1), Some(ProblemType::Pb));
    assert_eq!(ProblemType::from_u32(2), Some(ProblemType::Asp));
    assert_eq!(ProblemType::from_u32(3), None);
}
