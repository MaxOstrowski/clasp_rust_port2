//! Port target for original_clasp/clasp/cli/clasp_app.h, original_clasp/src/clasp_app.cpp.

use core::ptr::NonNull;

use crate::clasp::clasp_facade::ClaspFacade;
use crate::clasp::cli::clasp_cli_options::{CliEnum, KeyVal};
use crate::clasp::cli::clasp_options::ClaspCliConfig;
use crate::clasp::cli::clasp_output::{CatAssign, CatAtom, CatCost, CatStep, ColorStyleSpec};
use crate::clasp::mt::ThreadSafe;
use crate::clasp::pod_vector::PodVectorT;
use crate::potassco::basic_types::Lit as PotasscoLit;
use crate::potassco::enums::EnumTag;
use crate::potassco::platform::{CFile, stdout_stream};

#[repr(i32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitCode {
    Unknown = 0,
    Interrupt = 1,
    Sat = 10,
    Exhaust = 20,
    Memory = 33,
    Error = 65,
    NoRun = 128,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WriteCnf {
    str_: Option<NonNull<CFile>>,
}

impl WriteCnf {
    pub fn new(_out_file: &str) -> Self {
        Self::default()
    }

    pub fn close(&mut self) {
        self.str_ = None;
    }

    pub fn is_open(&self) -> bool {
        self.str_.is_some()
    }

    pub fn attach_raw_stream_for_test(&mut self, stream: *mut CFile) {
        self.str_ = NonNull::new(stream);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LemmaLoggerOptions {
    pub log_max: u32,
    pub lbd_max: u32,
    pub dom_out: bool,
    pub log_text: bool,
}

impl Default for LemmaLoggerOptions {
    fn default() -> Self {
        Self {
            log_max: u32::MAX,
            lbd_max: u32::MAX,
            dom_out: false,
            log_text: false,
        }
    }
}

pub struct LemmaLogger {
    str_: Option<NonNull<CFile>>,
    solver2_asp_: Vec<PotasscoLit>,
    solver2_name_idx_: PodVectorT<u32>,
    asp_: bool,
    options_: LemmaLoggerOptions,
    step_: i32,
    logged_: ThreadSafe<u32>,
}

impl LemmaLogger {
    pub fn new(out_file: &str, options: LemmaLoggerOptions) -> Self {
        let stream = if out_file == "-" || out_file == "stdout" {
            NonNull::new(stdout_stream())
        } else {
            None
        };
        Self {
            str_: stream,
            solver2_asp_: Vec::new(),
            solver2_name_idx_: PodVectorT::new(),
            asp_: false,
            options_: options,
            step_: 0,
            logged_: ThreadSafe::new(0),
        }
    }

    pub fn close(&mut self) {
        self.str_ = None;
        self.solver2_asp_.clear();
        self.solver2_name_idx_.clear();
    }

    pub fn is_open(&self) -> bool {
        self.str_.is_some()
    }

    pub fn is_asp(&self) -> bool {
        self.asp_
    }

    pub fn step(&self) -> i32 {
        self.step_
    }

    pub fn logged_count(&self) -> u32 {
        self.logged_.load(crate::clasp::mt::memory_order_relaxed)
    }

    pub fn options(&self) -> LemmaLoggerOptions {
        self.options_
    }

    pub fn solver2_asp(&self) -> &[PotasscoLit] {
        &self.solver2_asp_
    }

    pub fn solver2_name_idx(&self) -> &[u32] {
        self.solver2_name_idx_.as_slice()
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OutputFormat {
    #[default]
    Def = 0,
    Comp = 1,
    Json = 2,
    None = 3,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReifyFlag(u8);

impl ReifyFlag {
    pub const NONE: Self = Self(0);
    pub const SCC: Self = Self(1u8);
    pub const STEP: Self = Self(2u8);

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl core::ops::BitOr for ReifyFlag {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitOrAssign for ReifyFlag {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ColorMode {
    No = 0,
    #[default]
    Auto = 1,
    Yes = 2,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaspAppOptions {
    pub input: Vec<String>,
    pub lemma_log: String,
    pub lemma_in: String,
    pub hcc_out: String,
    pub out_atom: CatAtom,
    pub out_assign: CatAssign,
    pub out_cost: CatCost,
    pub out_step: CatStep,
    pub col_style: ColorStyleSpec,
    pub outf: OutputFormat,
    pub compute: i32,
    pub lemma: LemmaLoggerOptions,
    pub quiet: [u8; 3],
    pub pre: PreFormat,
    pub reify: ReifyFlag,
    pub ifs: char,
    pub pred_sep: char,
    pub hide_aux: bool,
    pub print_port: bool,
    pub color: ColorMode,
}

impl ClaspAppOptions {
    pub const Q_DEF: u8 = u8::MAX;

    pub fn is_text_output(format: OutputFormat) -> bool {
        matches!(format, OutputFormat::Def | OutputFormat::Comp)
    }
}

impl Default for ClaspAppOptions {
    fn default() -> Self {
        Self {
            input: Vec::new(),
            lemma_log: String::new(),
            lemma_in: String::new(),
            hcc_out: String::new(),
            out_atom: CatAtom::new(),
            out_assign: CatAssign::new(),
            out_cost: CatCost::new(),
            out_step: CatStep::new(),
            col_style: ColorStyleSpec::default(),
            outf: OutputFormat::Def,
            compute: 0,
            lemma: LemmaLoggerOptions::default(),
            quiet: [Self::Q_DEF, Self::Q_DEF, Self::Q_DEF],
            pre: PreFormat::No,
            reify: ReifyFlag::NONE,
            ifs: ' ',
            pred_sep: '\0',
            hide_aux: false,
            print_port: false,
            color: ColorMode::Auto,
        }
    }
}

#[derive(Debug, Default)]
pub struct AppOutputShell;

#[derive(Debug, Default)]
pub struct LemmaReader;

#[derive(Clone, Copy, Debug, Default)]
pub struct InputPtr {
    stream_: Option<NonNull<()>>,
    deleter_: Option<fn(*mut ())>,
}

impl InputPtr {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stream(&self) -> Option<NonNull<()>> {
        self.stream_
    }

    pub fn deleter(&self) -> Option<fn(*mut ())> {
        self.deleter_
    }
}

#[derive(Default)]
pub struct ClaspAppBase {
    pub clasp_config: ClaspCliConfig,
    pub clasp_app_opts: ClaspAppOptions,
    pub clasp: Option<Box<ClaspFacade>>,
    pub out: Option<Box<AppOutputShell>>,
    pub logger: Option<Box<LemmaLogger>>,
    pub lemma_in: Option<Box<LemmaReader>>,
    pub input: InputPtr,
    pub fpu_mode: u32,
}

impl ClaspAppBase {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Default)]
pub struct ClaspApp {
    pub base: ClaspAppBase,
}

impl ClaspApp {
    pub fn new() -> Self {
        Self::default()
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PreFormat {
    #[default]
    No = 0,
    Aspif = 1,
    Smodels = 2,
    Reify = 3,
}

impl EnumTag for PreFormat {
    type Repr = u8;

    fn to_underlying(self) -> Self::Repr {
        self as u8
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::No),
            1 => Some(Self::Aspif),
            2 => Some(Self::Smodels),
            3 => Some(Self::Reify),
            _ => None,
        }
    }

    fn min_value() -> Self::Repr {
        0
    }

    fn max_value() -> Self::Repr {
        3
    }

    fn count() -> usize {
        4
    }
}

impl CliEnum for PreFormat {
    fn entries() -> &'static [KeyVal<Self>] {
        const ENTRIES: &[KeyVal<PreFormat>] = &[
            KeyVal {
                key: "no",
                value: PreFormat::No,
            },
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
