//! Port target for original_clasp/clasp/cli/clasp_app.h, original_clasp/src/clasp_app.cpp.

use crate::clasp::cli::clasp_cli_options::{CliEnum, KeyVal};
use crate::potassco::enums::EnumTag;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PreFormat {
    #[default]
    Aspif = 0,
    Smodels = 1,
    Reify = 2,
}

impl EnumTag for PreFormat {
    type Repr = u8;

    fn to_underlying(self) -> Self::Repr {
        self as u8
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Aspif),
            1 => Some(Self::Smodels),
            2 => Some(Self::Reify),
            _ => None,
        }
    }

    fn min_value() -> Self::Repr {
        0
    }

    fn max_value() -> Self::Repr {
        2
    }

    fn count() -> usize {
        3
    }
}

impl CliEnum for PreFormat {
    fn entries() -> &'static [KeyVal<Self>] {
        const ENTRIES: &[KeyVal<PreFormat>] = &[
            KeyVal {
                key: "aspif",
                value: PreFormat::Aspif,
            },
            KeyVal {
                key: "smodels",
                value: PreFormat::Smodels,
            },
            KeyVal {
                key: "reify",
                value: PreFormat::Reify,
            },
        ];
        ENTRIES
    }
}
