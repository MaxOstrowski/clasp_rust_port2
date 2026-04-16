//! Rust port of the enum conversion surface from
//! `original_clasp/clasp/cli/clasp_cli_options.inl`.

use crate::clasp::util::misc_types::MovingAvgType;
use crate::potassco::enums::EnumTag;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyVal<E> {
    pub key: &'static str,
    pub value: E,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseError;

pub trait CliEnum: Copy + Eq + 'static {
    fn entries() -> &'static [KeyVal<Self>];

    fn canonical_key(self) -> Option<&'static str> {
        Self::entries()
            .iter()
            .find(|entry| entry.value == self)
            .map(|entry| entry.key)
    }
}

#[must_use]
pub fn enum_map<E: CliEnum>() -> &'static [KeyVal<E>] {
    E::entries()
}

pub fn from_chars<E: CliEnum>(input: &str) -> Result<(E, usize), ParseError> {
    let key = input.split_once(',').map_or(input, |(head, _)| head);
    enum_map::<E>()
        .iter()
        .find(|entry| entry.key.eq_ignore_ascii_case(key))
        .map(|entry| (entry.value, entry.key.len()))
        .ok_or(ParseError)
}

pub fn parse_exact<E: CliEnum>(input: &str) -> Result<E, ParseError> {
    let (value, consumed) = from_chars::<E>(input)?;
    if consumed == input.len() {
        Ok(value)
    } else {
        Err(ParseError)
    }
}

pub fn to_chars<E: CliEnum>(out: &mut String, value: E) -> &mut String {
    if let Some(key) = value.canonical_key() {
        out.push_str(key);
    }
    out
}

macro_rules! define_cli_enum {
	(
		$(#[$meta:meta])*
		$vis:vis enum $name:ident : $repr:ty {
			$($variant:ident = $value:expr => $key:literal $(| $alias:literal)*),+ $(,)?
		}
	) => {
		$(#[$meta])*
		#[repr($repr)]
		#[derive(Copy, Clone, Debug, Eq, PartialEq)]
		$vis enum $name {
			$($variant = $value),+
		}

		impl EnumTag for $name {
			type Repr = $repr;

			fn to_underlying(self) -> Self::Repr {
				self as $repr
			}

			fn from_underlying(value: Self::Repr) -> Option<Self> {
				match value {
					$($value => Some(Self::$variant),)+
					_ => None,
				}
			}

			fn min_value() -> Self::Repr {
				[$($value),+].into_iter().min().unwrap_or(0)
			}

			fn max_value() -> Self::Repr {
				[$($value),+].into_iter().max().unwrap_or(0)
			}

			fn count() -> usize {
				[$(stringify!($variant)),+].len()
			}
		}

		impl CliEnum for $name {
			fn entries() -> &'static [KeyVal<Self>] {
				const ENTRIES: &[KeyVal<$name>] = &[
					$(
						KeyVal { key: $key, value: $name::$variant },
						$(KeyVal { key: $alias, value: $name::$variant },)*
					)+
				];
				ENTRIES
			}
		}
	};
}

impl EnumTag for MovingAvgType {
    type Repr = u32;

    fn to_underlying(self) -> Self::Repr {
        self as u32
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::AvgSma),
            1 => Some(Self::AvgEma),
            2 => Some(Self::AvgEmaLog),
            3 => Some(Self::AvgEmaSmooth),
            4 => Some(Self::AvgEmaLogSmooth),
            _ => None,
        }
    }

    fn min_value() -> Self::Repr {
        0
    }

    fn max_value() -> Self::Repr {
        4
    }

    fn count() -> usize {
        5
    }
}

impl CliEnum for MovingAvgType {
    fn entries() -> &'static [KeyVal<Self>] {
        const ENTRIES: &[KeyVal<MovingAvgType>] = &[
            KeyVal {
                key: "d",
                value: MovingAvgType::AvgSma,
            },
            KeyVal {
                key: "e",
                value: MovingAvgType::AvgEma,
            },
            KeyVal {
                key: "l",
                value: MovingAvgType::AvgEmaLog,
            },
            KeyVal {
                key: "es",
                value: MovingAvgType::AvgEmaSmooth,
            },
            KeyVal {
                key: "ls",
                value: MovingAvgType::AvgEmaLogSmooth,
            },
        ];
        ENTRIES
    }
}

pub mod context_params {
    use super::*;

    define_cli_enum! {
        pub enum ShareMode: u8 {
            ShareNo = 0 => "no",
            ShareAll = 3 => "all",
            ShareAuto = 4 => "auto",
            ShareProblem = 1 => "problem",
            ShareLearnt = 2 => "learnt"
        }
    }

    define_cli_enum! {
        pub enum ShortSimpMode: u8 {
            SimpNo = 0 => "no",
            SimpLearnt = 1 => "learnt",
            SimpAll = 2 => "all"
        }
    }
}

pub mod opt_params {
    use super::*;

    define_cli_enum! {
        pub enum Type: u8 {
            TypeBb = 0 => "bb",
            TypeUsc = 1 => "usc"
        }
    }

    define_cli_enum! {
        pub enum BBAlgo: u8 {
            BbLin = 0 => "lin",
            BbHier = 1 => "hier",
            BbInc = 2 => "inc",
            BbDec = 3 => "dec"
        }
    }

    define_cli_enum! {
        pub enum UscAlgo: u8 {
            UscOll = 0 => "oll",
            UscOne = 1 => "one",
            UscK = 2 => "k",
            UscPmr = 3 => "pmres"
        }
    }

    define_cli_enum! {
        pub enum UscOption: u8 {
            UscDisjoint = 1 => "disjoint",
            UscSuccinct = 2 => "succinct",
            UscStratify = 4 => "stratify"
        }
    }

    define_cli_enum! {
        pub enum UscTrim: u8 {
            UscTrimLin = 1 => "lin",
            UscTrimRgs = 4 => "rgs",
            UscTrimMin = 6 => "min",
            UscTrimExp = 5 => "exp",
            UscTrimInv = 2 => "inv",
            UscTrimBin = 3 => "bin"
        }
    }

    define_cli_enum! {
        pub enum Heuristic: u8 {
            HeuSign = 1 => "sign",
            HeuModel = 2 => "model"
        }
    }
}

define_cli_enum! {
    pub enum VarType: u32 {
        Atom = 1 => "atom",
        Body = 2 => "body",
        Hybrid = 3 => "hybrid"
    }
}

#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum HeuristicType {
    Def = 0,
    Berkmin = 1,
    Vsids = 2,
    Vmtf = 3,
    Domain = 4,
    Unit = 5,
    None = 6,
    User = 7,
}

impl EnumTag for HeuristicType {
    type Repr = u32;

    fn to_underlying(self) -> Self::Repr {
        self as u32
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Def),
            1 => Some(Self::Berkmin),
            2 => Some(Self::Vsids),
            3 => Some(Self::Vmtf),
            4 => Some(Self::Domain),
            5 => Some(Self::Unit),
            6 => Some(Self::None),
            7 => Some(Self::User),
            _ => None,
        }
    }

    fn min_value() -> Self::Repr {
        0
    }

    fn max_value() -> Self::Repr {
        7
    }

    fn count() -> usize {
        8
    }
}

impl CliEnum for HeuristicType {
    fn entries() -> &'static [KeyVal<Self>] {
        const ENTRIES: &[KeyVal<HeuristicType>] = &[
            KeyVal {
                key: "berkmin",
                value: HeuristicType::Berkmin,
            },
            KeyVal {
                key: "vmtf",
                value: HeuristicType::Vmtf,
            },
            KeyVal {
                key: "vsids",
                value: HeuristicType::Vsids,
            },
            KeyVal {
                key: "domain",
                value: HeuristicType::Domain,
            },
            KeyVal {
                key: "unit",
                value: HeuristicType::Unit,
            },
            KeyVal {
                key: "auto",
                value: HeuristicType::Def,
            },
            KeyVal {
                key: "none",
                value: HeuristicType::None,
            },
        ];
        ENTRIES
    }
}

pub mod heu_params {
    use super::*;

    define_cli_enum! {
        pub enum Score: u8 {
            ScoreAuto = 0 => "auto",
            ScoreMin = 1 => "min",
            ScoreSet = 2 => "set",
            ScoreMultiSet = 3 => "multiset"
        }
    }

    define_cli_enum! {
        pub enum ScoreOther: u8 {
            OtherAuto = 0 => "auto",
            OtherNo = 1 => "no",
            OtherLoop = 2 => "loop",
            OtherAll = 3 => "all"
        }
    }

    #[repr(u8)]
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum DomMod {
        ModNone = 0,
        ModLevel = 1,
        ModSPos = 2,
        ModTrue = 3,
        ModSNeg = 4,
        ModFalse = 5,
        ModInit = 6,
        ModFactor = 7,
    }

    impl EnumTag for DomMod {
        type Repr = u8;

        fn to_underlying(self) -> Self::Repr {
            self as u8
        }

        fn from_underlying(value: Self::Repr) -> Option<Self> {
            match value {
                0 => Some(Self::ModNone),
                1 => Some(Self::ModLevel),
                2 => Some(Self::ModSPos),
                3 => Some(Self::ModTrue),
                4 => Some(Self::ModSNeg),
                5 => Some(Self::ModFalse),
                6 => Some(Self::ModInit),
                7 => Some(Self::ModFactor),
                _ => None,
            }
        }

        fn min_value() -> Self::Repr {
            0
        }

        fn max_value() -> Self::Repr {
            7
        }

        fn count() -> usize {
            8
        }
    }

    impl CliEnum for DomMod {
        fn entries() -> &'static [KeyVal<Self>] {
            const ENTRIES: &[KeyVal<DomMod>] = &[
                KeyVal {
                    key: "level",
                    value: DomMod::ModLevel,
                },
                KeyVal {
                    key: "pos",
                    value: DomMod::ModSPos,
                },
                KeyVal {
                    key: "true",
                    value: DomMod::ModTrue,
                },
                KeyVal {
                    key: "neg",
                    value: DomMod::ModSNeg,
                },
                KeyVal {
                    key: "false",
                    value: DomMod::ModFalse,
                },
                KeyVal {
                    key: "init",
                    value: DomMod::ModInit,
                },
                KeyVal {
                    key: "factor",
                    value: DomMod::ModFactor,
                },
            ];
            ENTRIES
        }
    }

    define_cli_enum! {
        pub enum DomPref: u8 {
            PrefAtom = 0 => "all",
            PrefScc = 1 => "scc",
            PrefHcc = 2 => "hcc",
            PrefDisj = 4 => "disj",
            PrefMin = 8 => "opt",
            PrefShow = 16 => "show"
        }
    }
}

pub mod solver_strategies {
    use super::*;

    define_cli_enum! {
        pub enum SignHeu: u8 {
            SignAtom = 0 => "asp",
            SignPos = 1 => "pos",
            SignNeg = 2 => "neg",
            SignRnd = 3 => "rnd"
        }
    }

    define_cli_enum! {
        pub enum WatchInit: u8 {
            WatchRand = 0 => "rnd",
            WatchFirst = 1 => "first",
            WatchLeast = 2 => "least"
        }
    }

    define_cli_enum! {
        pub enum UpdateMode: u8 {
            UpdateOnPropagate = 0 => "propagate",
            UpdateOnConflict = 1 => "conflict"
        }
    }

    define_cli_enum! {
        pub enum CCMinType: u8 {
            CcLocal = 0 => "local",
            CcRecursive = 1 => "recursive"
        }
    }

    #[repr(u8)]
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum CCMinAntes {
        AllAntes = 0,
        ShortAntes = 1,
        BinaryAntes = 2,
        NoAntes = 3,
    }

    impl EnumTag for CCMinAntes {
        type Repr = u8;

        fn to_underlying(self) -> Self::Repr {
            self as u8
        }

        fn from_underlying(value: Self::Repr) -> Option<Self> {
            match value {
                0 => Some(Self::AllAntes),
                1 => Some(Self::ShortAntes),
                2 => Some(Self::BinaryAntes),
                3 => Some(Self::NoAntes),
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

    impl CliEnum for CCMinAntes {
        fn entries() -> &'static [KeyVal<Self>] {
            const ENTRIES: &[KeyVal<CCMinAntes>] = &[
                KeyVal {
                    key: "all",
                    value: CCMinAntes::AllAntes,
                },
                KeyVal {
                    key: "short",
                    value: CCMinAntes::ShortAntes,
                },
                KeyVal {
                    key: "binary",
                    value: CCMinAntes::BinaryAntes,
                },
            ];
            ENTRIES
        }
    }

    #[repr(u8)]
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum LbdMode {
        LbdFixed = 0,
        LbdUpdatedLess = 1,
        LbdUpdateGlucose = 2,
        LbdUpdatePseudo = 3,
    }

    impl EnumTag for LbdMode {
        type Repr = u8;

        fn to_underlying(self) -> Self::Repr {
            self as u8
        }

        fn from_underlying(value: Self::Repr) -> Option<Self> {
            match value {
                0 => Some(Self::LbdFixed),
                1 => Some(Self::LbdUpdatedLess),
                2 => Some(Self::LbdUpdateGlucose),
                3 => Some(Self::LbdUpdatePseudo),
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

    impl CliEnum for LbdMode {
        fn entries() -> &'static [KeyVal<Self>] {
            const ENTRIES: &[KeyVal<LbdMode>] = &[
                KeyVal {
                    key: "less",
                    value: LbdMode::LbdUpdatedLess,
                },
                KeyVal {
                    key: "glucose",
                    value: LbdMode::LbdUpdateGlucose,
                },
                KeyVal {
                    key: "pseudo",
                    value: LbdMode::LbdUpdatePseudo,
                },
            ];
            ENTRIES
        }
    }

    define_cli_enum! {
        pub enum CCRepMode: u8 {
            CcNoReplace = 0 => "no",
            CcRepDecision = 1 => "decisionSeq",
            CcRepUip = 2 => "allUIP",
            CcRepDynamic = 3 => "dynamic"
        }
    }
}

pub mod solver_params {
    use super::*;

    define_cli_enum! {
        pub enum Forget: u8 {
            ForgetHeuristic = 1 => "varScores",
            ForgetSigns = 2 => "signs",
            ForgetActivities = 4 => "lemmaScores",
            ForgetLearnts = 8 => "lemmas"
        }
    }
}

pub mod default_unfounded_check {
    use super::*;

    #[repr(u8)]
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum ReasonStrategy {
        CommonReason = 0,
        OnlyReason = 1,
        DistinctReason = 2,
        SharedReason = 3,
        NoReason = 4,
    }

    impl EnumTag for ReasonStrategy {
        type Repr = u8;

        fn to_underlying(self) -> Self::Repr {
            self as u8
        }

        fn from_underlying(value: Self::Repr) -> Option<Self> {
            match value {
                0 => Some(Self::CommonReason),
                1 => Some(Self::OnlyReason),
                2 => Some(Self::DistinctReason),
                3 => Some(Self::SharedReason),
                4 => Some(Self::NoReason),
                _ => None,
            }
        }

        fn min_value() -> Self::Repr {
            0
        }

        fn max_value() -> Self::Repr {
            4
        }

        fn count() -> usize {
            5
        }
    }

    impl CliEnum for ReasonStrategy {
        fn entries() -> &'static [KeyVal<Self>] {
            const ENTRIES: &[KeyVal<ReasonStrategy>] = &[
                KeyVal {
                    key: "common",
                    value: ReasonStrategy::CommonReason,
                },
                KeyVal {
                    key: "shared",
                    value: ReasonStrategy::SharedReason,
                },
                KeyVal {
                    key: "distinct",
                    value: ReasonStrategy::DistinctReason,
                },
                KeyVal {
                    key: "no",
                    value: ReasonStrategy::OnlyReason,
                },
            ];
            ENTRIES
        }
    }
}

pub mod restart_schedule {
    use super::*;

    define_cli_enum! {
        pub enum Keep: u8 {
            KeepNever = 0 => "n",
            KeepRestart = 1 => "r",
            KeepBlock = 2 => "b",
            KeepAlways = 3 => "br" | "rb"
        }
    }
}

pub mod restart_params {
    use super::*;

    define_cli_enum! {
        pub enum SeqUpdate: u8 {
            SeqContinue = 0 => "no",
            SeqRepeat = 1 => "repeat",
            SeqDisable = 2 => "disable"
        }
    }
}

pub mod reduce_strategy {
    use super::*;

    define_cli_enum! {
        pub enum Algorithm: u8 {
            ReduceLinear = 0 => "basic",
            ReduceStable = 1 => "sort",
            ReduceSort = 2 => "ipSort",
            ReduceHeap = 3 => "ipHeap"
        }
    }

    define_cli_enum! {
        pub enum Score: u8 {
            ScoreAct = 0 => "activity",
            ScoreLbd = 1 => "lbd",
            ScoreBoth = 2 => "mixed"
        }
    }
}

pub mod asp_logic_program {
    use super::*;

    #[repr(u8)]
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum ExtendedRuleMode {
        ModeNative = 0,
        ModeTransform = 1,
        ModeTransformChoice = 2,
        ModeTransformCard = 3,
        ModeTransformWeight = 4,
        ModeTransformScc = 5,
        ModeTransformNhcf = 6,
        ModeTransformInteg = 7,
        ModeTransformDynamic = 8,
    }

    impl EnumTag for ExtendedRuleMode {
        type Repr = u8;

        fn to_underlying(self) -> Self::Repr {
            self as u8
        }

        fn from_underlying(value: Self::Repr) -> Option<Self> {
            match value {
                0 => Some(Self::ModeNative),
                1 => Some(Self::ModeTransform),
                2 => Some(Self::ModeTransformChoice),
                3 => Some(Self::ModeTransformCard),
                4 => Some(Self::ModeTransformWeight),
                5 => Some(Self::ModeTransformScc),
                6 => Some(Self::ModeTransformNhcf),
                7 => Some(Self::ModeTransformInteg),
                8 => Some(Self::ModeTransformDynamic),
                _ => None,
            }
        }

        fn min_value() -> Self::Repr {
            0
        }

        fn max_value() -> Self::Repr {
            8
        }

        fn count() -> usize {
            9
        }
    }

    impl CliEnum for ExtendedRuleMode {
        fn entries() -> &'static [KeyVal<Self>] {
            const ENTRIES: &[KeyVal<ExtendedRuleMode>] = &[
                KeyVal {
                    key: "no",
                    value: ExtendedRuleMode::ModeNative,
                },
                KeyVal {
                    key: "all",
                    value: ExtendedRuleMode::ModeTransform,
                },
                KeyVal {
                    key: "choice",
                    value: ExtendedRuleMode::ModeTransformChoice,
                },
                KeyVal {
                    key: "card",
                    value: ExtendedRuleMode::ModeTransformCard,
                },
                KeyVal {
                    key: "weight",
                    value: ExtendedRuleMode::ModeTransformWeight,
                },
                KeyVal {
                    key: "scc",
                    value: ExtendedRuleMode::ModeTransformScc,
                },
                KeyVal {
                    key: "integ",
                    value: ExtendedRuleMode::ModeTransformInteg,
                },
                KeyVal {
                    key: "dynamic",
                    value: ExtendedRuleMode::ModeTransformDynamic,
                },
            ];
            ENTRIES
        }
    }

    define_cli_enum! {
        pub enum AtomSorting: u8 {
            SortNo = 1 => "no",
            SortAuto = 0 => "auto",
            SortNumber = 2 => "number",
            SortName = 3 => "name",
            SortNatural = 4 => "natural",
            SortArity = 5 => "arity",
            SortArityNatural = 6 => "full"
        }
    }
}

pub mod solve_options {
    use super::*;

    pub mod algorithm {
        use super::*;

        define_cli_enum! {
            pub enum SearchMode: u8 {
                ModeCompete = 1 => "compete",
                ModeSplit = 0 => "split"
            }
        }
    }

    pub mod distribution {
        use super::*;

        define_cli_enum! {
            pub enum Mode: u8 {
                ModeGlobal = 0 => "global",
                ModeLocal = 1 => "local"
            }
        }
    }

    pub mod integration {
        use super::*;

        define_cli_enum! {
            pub enum Filter: u8 {
                FilterNo = 0 => "all",
                FilterGp = 1 => "gp",
                FilterSat = 2 => "unsat",
                FilterHeuristic = 3 => "active"
            }
        }

        define_cli_enum! {
            pub enum Topology: u8 {
                TopoAll = 0 => "all",
                TopoRing = 1 => "ring",
                TopoCube = 2 => "cube",
                TopoCubex = 3 => "cubex"
            }
        }
    }

    #[repr(u8)]
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum EnumType {
        EnumAuto = 0,
        EnumBt = 1,
        EnumRecord = 2,
        EnumDomRecord = 3,
        EnumConsequences = 4,
        EnumBrave = 5,
        EnumCautious = 6,
        EnumQuery = 7,
        EnumUser = 8,
    }

    impl EnumTag for EnumType {
        type Repr = u8;

        fn to_underlying(self) -> Self::Repr {
            self as u8
        }

        fn from_underlying(value: Self::Repr) -> Option<Self> {
            match value {
                0 => Some(Self::EnumAuto),
                1 => Some(Self::EnumBt),
                2 => Some(Self::EnumRecord),
                3 => Some(Self::EnumDomRecord),
                4 => Some(Self::EnumConsequences),
                5 => Some(Self::EnumBrave),
                6 => Some(Self::EnumCautious),
                7 => Some(Self::EnumQuery),
                8 => Some(Self::EnumUser),
                _ => None,
            }
        }

        fn min_value() -> Self::Repr {
            0
        }

        fn max_value() -> Self::Repr {
            8
        }

        fn count() -> usize {
            9
        }
    }

    impl CliEnum for EnumType {
        fn entries() -> &'static [KeyVal<Self>] {
            const ENTRIES: &[KeyVal<EnumType>] = &[
                KeyVal {
                    key: "bt",
                    value: EnumType::EnumBt,
                },
                KeyVal {
                    key: "record",
                    value: EnumType::EnumRecord,
                },
                KeyVal {
                    key: "domRec",
                    value: EnumType::EnumDomRecord,
                },
                KeyVal {
                    key: "brave",
                    value: EnumType::EnumBrave,
                },
                KeyVal {
                    key: "cautious",
                    value: EnumType::EnumCautious,
                },
                KeyVal {
                    key: "query",
                    value: EnumType::EnumQuery,
                },
                KeyVal {
                    key: "auto",
                    value: EnumType::EnumAuto,
                },
                KeyVal {
                    key: "user",
                    value: EnumType::EnumUser,
                },
            ];
            ENTRIES
        }
    }
}

pub mod distributor_policy {
    use super::*;

    #[repr(u8)]
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum Types {
        No = 0,
        Conflict = 1,
        Loop = 2,
        All = 3,
        Implicit = 4,
    }

    impl EnumTag for Types {
        type Repr = u8;

        fn to_underlying(self) -> Self::Repr {
            self as u8
        }

        fn from_underlying(value: Self::Repr) -> Option<Self> {
            match value {
                0 => Some(Self::No),
                1 => Some(Self::Conflict),
                2 => Some(Self::Loop),
                3 => Some(Self::All),
                4 => Some(Self::Implicit),
                _ => None,
            }
        }

        fn min_value() -> Self::Repr {
            0
        }

        fn max_value() -> Self::Repr {
            4
        }

        fn count() -> usize {
            5
        }
    }

    impl CliEnum for Types {
        fn entries() -> &'static [KeyVal<Self>] {
            const ENTRIES: &[KeyVal<Types>] = &[
                KeyVal {
                    key: "all",
                    value: Types::All,
                },
                KeyVal {
                    key: "short",
                    value: Types::Implicit,
                },
                KeyVal {
                    key: "conflict",
                    value: Types::Conflict,
                },
                KeyVal {
                    key: "loop",
                    value: Types::Loop,
                },
            ];
            ENTRIES
        }
    }
}

define_cli_enum! {
    pub enum ProjectMode: u8 {
        Implicit = 0 => "auto",
        Output = 1 => "show",
        Project = 2 => "project"
    }
}

define_cli_enum! {
    pub enum MinimizeMode: u8 {
        Optimize = 1 => "opt",
        Enumerate = 2 => "enum",
        EnumOpt = 3 => "optN",
        Ignore = 0 => "ignore"
    }
}
