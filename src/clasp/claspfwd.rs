//! Rust port of `original_clasp/clasp/claspfwd.h`.

macro_rules! opaque_type {
    ($name:ident) => {
        #[derive(Debug)]
        pub struct $name {
            _private: (),
        }
    };
}

opaque_type!(SharedContext);
opaque_type!(MinimizeBuilder);
opaque_type!(SharedMinimizeData);
opaque_type!(Configuration);
opaque_type!(Constraint);
opaque_type!(ConstraintInfo);
opaque_type!(Solver);
opaque_type!(Model);
opaque_type!(ProgramBuilder);
opaque_type!(ProgramParser);
opaque_type!(SatBuilder);
opaque_type!(PBBuilder);
opaque_type!(ExtDepGraph);

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProblemType {
    Sat = 0,
    Pb = 1,
    Asp = 2,
}

impl ProblemType {
    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Sat),
            1 => Some(Self::Pb),
            2 => Some(Self::Asp),
            _ => None,
        }
    }

    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

pub mod asp {
    macro_rules! opaque_type {
        ($name:ident) => {
            #[derive(Debug)]
            pub struct $name {
                _private: (),
            }
        };
    }

    opaque_type!(LogicProgram);
    opaque_type!(Preprocessor);
    opaque_type!(LpStats);
    opaque_type!(PrgAtom);
    opaque_type!(PrgBody);
    opaque_type!(PrgDisj);
    opaque_type!(PrgHead);
    opaque_type!(PrgNode);
    opaque_type!(PrgDepGraph);
    opaque_type!(PrgEdge);
}

pub use self::asp as Asp;
