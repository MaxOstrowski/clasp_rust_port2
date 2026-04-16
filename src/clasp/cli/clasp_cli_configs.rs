//! Rust port of `original_clasp/clasp/cli/clasp_cli_configs.inl`.

use crate::clasp::claspfwd::ProblemType;
use crate::clasp::cli::clasp_cli_options::{CliEnum, KeyVal};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigEntry {
    pub solver_id: u8,
    pub name: &'static str,
    pub common: &'static str,
    pub standalone: &'static str,
    pub portfolio: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ConfigView {
    name: &'static str,
    base: &'static str,
    args: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigIter {
    views: &'static [ConfigView],
    index: usize,
}

impl ConfigIter {
    pub fn name(&self) -> &'static str {
        self.current().name
    }

    pub fn base(&self) -> &'static str {
        self.current().base
    }

    pub fn args(&self) -> &'static str {
        self.current().args
    }

    pub fn valid(&self) -> bool {
        self.index < self.views.len()
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> bool {
        if self.valid() {
            self.index += 1;
        }
        self.valid()
    }

    fn current(&self) -> &'static ConfigView {
        self.views.get(self.index).unwrap_or(&INVALID_CONFIG_VIEW)
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigKey {
    Default = 0,
    Tweety = 1,
    Trendy = 2,
    Frumpy = 3,
    Crafty = 4,
    Jumpy = 5,
    Handy = 6,
    S6 = 8,
    S7 = 9,
    S8 = 10,
    S9 = 11,
    S10 = 12,
    S11 = 13,
    S12 = 14,
    S13 = 15,
    Nolearn = 16,
    Tester = 17,
    Many = 19,
}

impl ConfigKey {
    pub const DEFAULT_MAX_VALUE: u8 = 7;
    pub const AUX_MAX_VALUE: u8 = 18;
    pub const MAX_VALUE: u8 = 20;
    pub const ASP_DEFAULT: Self = Self::Tweety;
    pub const SAT_DEFAULT: Self = Self::Trendy;
    pub const TESTER_DEFAULT: Self = Self::Tester;

    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Default),
            1 => Some(Self::Tweety),
            2 => Some(Self::Trendy),
            3 => Some(Self::Frumpy),
            4 => Some(Self::Crafty),
            5 => Some(Self::Jumpy),
            6 => Some(Self::Handy),
            8 => Some(Self::S6),
            9 => Some(Self::S7),
            10 => Some(Self::S8),
            11 => Some(Self::S9),
            12 => Some(Self::S10),
            13 => Some(Self::S11),
            14 => Some(Self::S12),
            15 => Some(Self::S13),
            16 => Some(Self::Nolearn),
            17 => Some(Self::Tester),
            19 => Some(Self::Many),
            _ => None,
        }
    }

    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

impl CliEnum for ConfigKey {
    fn entries() -> &'static [KeyVal<Self>] {
        const ENTRIES: &[KeyVal<ConfigKey>] = &[
            KeyVal {
                key: "auto",
                value: ConfigKey::Default,
            },
            KeyVal {
                key: "tweety",
                value: ConfigKey::Tweety,
            },
            KeyVal {
                key: "trendy",
                value: ConfigKey::Trendy,
            },
            KeyVal {
                key: "frumpy",
                value: ConfigKey::Frumpy,
            },
            KeyVal {
                key: "crafty",
                value: ConfigKey::Crafty,
            },
            KeyVal {
                key: "jumpy",
                value: ConfigKey::Jumpy,
            },
            KeyVal {
                key: "handy",
                value: ConfigKey::Handy,
            },
            KeyVal {
                key: "s6",
                value: ConfigKey::S6,
            },
            KeyVal {
                key: "s7",
                value: ConfigKey::S7,
            },
            KeyVal {
                key: "s8",
                value: ConfigKey::S8,
            },
            KeyVal {
                key: "s9",
                value: ConfigKey::S9,
            },
            KeyVal {
                key: "s10",
                value: ConfigKey::S10,
            },
            KeyVal {
                key: "s11",
                value: ConfigKey::S11,
            },
            KeyVal {
                key: "s12",
                value: ConfigKey::S12,
            },
            KeyVal {
                key: "s13",
                value: ConfigKey::S13,
            },
            KeyVal {
                key: "nolearn",
                value: ConfigKey::Nolearn,
            },
            KeyVal {
                key: "tester",
                value: ConfigKey::Tester,
            },
            KeyVal {
                key: "many",
                value: ConfigKey::Many,
            },
        ];
        ENTRIES
    }
}

const INVALID_CONFIG_VIEW: ConfigView = ConfigView {
    name: "",
    base: "",
    args: "",
};

const DEFAULT_CONFIG_VIEW: ConfigView = ConfigView {
    name: "default",
    base: "",
    args: "",
};

pub const DEFAULT_CONFIGS: &[ConfigEntry] = &[
    ConfigEntry {
        solver_id: 0,
        name: "tweety",
        common: concat!(
            "--heuristic=Vsids,92 --restarts=L,60 --deletion=basic,50 --del-max=2000000",
            " --del-estimate=1 --del-cfl=+,2000,100,20 --del-grow=0 --del-glue=2,0",
            " --strengthen=recursive,all --otfs=2 --init-moms --score-other=all",
            " --update-lbd=less --save-progress=160 --init-watches=least",
            " --local-restarts --loops=shared"
        ),
        standalone: "--eq=3 --trans-ext=dynamic",
        portfolio: "--opt-strat=bb,hier",
    },
    ConfigEntry {
        solver_id: 1,
        name: "trendy",
        common: concat!(
            "--heuristic=Vsids --restarts=D,100,0.7 --deletion=basic,50",
            " --del-init=3.0,500,19500 --del-grow=1.1,20.0,x,100,1.5",
            " --del-cfl=+,10000,2000 --del-glue=2 --strengthen=recursive",
            " --update-lbd=less --otfs=2 --save-p=75 --counter-restarts=3,1023",
            " --reverse-arcs=2 --contraction=250 --loops=common"
        ),
        standalone: "--sat-p=2,iter=20,occ=25,time=240 --trans-ext=dynamic",
        portfolio: "--opt-heu=sign --opt-strat=usc,disjoint",
    },
    ConfigEntry {
        solver_id: 2,
        name: "frumpy",
        common: concat!(
            "--heuristic=Berkmin --restarts=x,100,1.5 --deletion=basic,75",
            " --del-init=3.0,200,40000 --del-max=400000 --contraction=250",
            " --loops=common --save-p=180 --del-grow=1.1 --strengthen=local",
            " --sign-def-disj=pos"
        ),
        standalone: "--eq=5",
        portfolio: "--restart-on-model --opt-heu=model",
    },
    ConfigEntry {
        solver_id: 3,
        name: "crafty",
        common: concat!(
            "--restarts=x,128,1.5 --deletion=basic,75 --del-init=10.0,1000,9000",
            " --del-grow=1.1,20.0 --del-cfl=+,10000,1000 --del-glue=2 --otfs=2",
            " --reverse-arcs=1 --counter-restarts=3,9973 --contraction=250"
        ),
        standalone: concat!(
            "--sat-p=2,iter=10,occ=25,time=240 --trans-ext=dynamic --backprop",
            " --heuristic=Vsids --save-p=180"
        ),
        portfolio: "--heuristic=domain --dom-mod=neg,opt --opt-strat=bb,hier",
    },
    ConfigEntry {
        solver_id: 4,
        name: "jumpy",
        common: concat!(
            "--heuristic=Vsids --restarts=L,100 --deletion=basic,75,mixed",
            " --del-init=3.0,1000,20000 --del-grow=1.1,25,x,100,1.5",
            " --del-cfl=x,10000,1.1 --del-glue=2 --update-lbd=glucose",
            " --strengthen=recursive --otfs=2 --save-p=70"
        ),
        standalone: "--sat-p=2,iter=20,occ=25,time=240 --trans-ext=dynamic",
        portfolio: "--restart-on-model --opt-heu=sign,model --opt-strat=bb,inc",
    },
    ConfigEntry {
        solver_id: 5,
        name: "handy",
        common: concat!(
            "--heuristic=Vsids --restarts=D,100,0.7 --deletion=sort,50,mixed",
            " --del-max=200000 --del-init=20.0,1000,14000 --del-cfl=+,4000,600",
            " --del-glue=2 --update-lbd=less --strengthen=recursive --otfs=2",
            " --save-p=20 --contraction=600 --loops=distinct --counter-restarts=7,1023",
            " --reverse-arcs=2"
        ),
        standalone: "--sat-p=2,iter=10,occ=25,time=240 --trans-ext=dynamic --backprop",
        portfolio: "",
    },
];

pub const AUX_CONFIGS: &[ConfigEntry] = &[
    ConfigEntry {
        solver_id: 6,
        name: "s6",
        common: "--heuristic=Berkmin,512 --restarts=x,100,1.5 --deletion=basic,75 --del-init=3.0,200,40000 --del-max=400000 --contraction=250 --loops=common --del-grow=1.1,25 --otfs=2 --reverse-arcs=2 --strengthen=recursive --init-w=least --lookahead=atom,10",
        standalone: "",
        portfolio: "",
    },
    ConfigEntry {
        solver_id: 7,
        name: "s7",
        common: "--heuristic=Vsids --reverse-arcs=1 --otfs=1 --local-restarts --save-progress=0 --contraction=250 --counter-restart=7,200 --restarts=x,100,1.5 --del-init=3.0,800,-1 --deletion=basic,60 --strengthen=local --del-grow=1.0,1.0 --del-glue=4 --del-cfl=+,4000,300,100",
        standalone: "",
        portfolio: "",
    },
    ConfigEntry {
        solver_id: 8,
        name: "s8",
        common: "--heuristic=Vsids --restarts=L,256 --counter-restart=3,9973 --strengthen=recursive --update-lbd=less --del-glue=2 --otfs=2 --deletion=ipSort,75,mixed --del-init=20.0,1000,19000",
        standalone: "",
        portfolio: "",
    },
    ConfigEntry {
        solver_id: 9,
        name: "s9",
        common: "--heuristic=Berkmin,512 --restarts=F,16000 --lookahead=atom,50",
        standalone: "",
        portfolio: "",
    },
    ConfigEntry {
        solver_id: 10,
        name: "s10",
        common: "--heuristic=Vmtf --strengthen=no --contr=0 --restarts=x,100,1.3 --del-init=3.0,800,9200",
        standalone: "",
        portfolio: "",
    },
    ConfigEntry {
        solver_id: 11,
        name: "s11",
        common: "--heuristic=Vsids --strengthen=recursive --restarts=x,100,1.5,15 --contraction=0",
        standalone: "",
        portfolio: "",
    },
    ConfigEntry {
        solver_id: 12,
        name: "s12",
        common: "--heuristic=Vsids --restarts=L,128 --save-p --otfs=1 --init-w=least --contr=0 --opt-heu=sign,model",
        standalone: "",
        portfolio: "",
    },
    ConfigEntry {
        solver_id: 13,
        name: "s13",
        common: "--heuristic=Berkmin,512 --restarts=x,100,1.5,6 --local-restarts --init-w=least --contr=0",
        standalone: "",
        portfolio: "",
    },
    ConfigEntry {
        solver_id: 14,
        name: "nolearn",
        common: "--no-lookback --heuristic=Unit --lookahead=atom --deletion=no --restarts=no",
        standalone: "",
        portfolio: "",
    },
    ConfigEntry {
        solver_id: 15,
        name: "tester",
        common: concat!(
            "--heuristic=Vsids --restarts=D,100,0.7 --deletion=sort,50,mixed",
            " --del-max=200000 --del-init=20.0,1000,14000 --del-cfl=+,4000,600",
            " --del-glue=2 --update-lbd=less --strengthen=recursive --otfs=2",
            " --save-p=20 --contraction=600 --counter-restarts=7,1023 --reverse-arcs=2"
        ),
        standalone: "--sat-p=2,iter=10,occ=25,time=240",
        portfolio: "",
    },
];

const SINGLE_CONFIGS: &[ConfigView] = &[
    ConfigView {
        name: "[tweety]",
        base: "",
        args: concat!(
            "--eq=3 --trans-ext=dynamic ",
            "--heuristic=Vsids,92 --restarts=L,60 --deletion=basic,50 --del-max=2000000",
            " --del-estimate=1 --del-cfl=+,2000,100,20 --del-grow=0 --del-glue=2,0",
            " --strengthen=recursive,all --otfs=2 --init-moms --score-other=all",
            " --update-lbd=less --save-progress=160 --init-watches=least",
            " --local-restarts --loops=shared"
        ),
    },
    ConfigView {
        name: "[trendy]",
        base: "",
        args: concat!(
            "--sat-p=2,iter=20,occ=25,time=240 --trans-ext=dynamic ",
            "--heuristic=Vsids --restarts=D,100,0.7 --deletion=basic,50",
            " --del-init=3.0,500,19500 --del-grow=1.1,20.0,x,100,1.5",
            " --del-cfl=+,10000,2000 --del-glue=2 --strengthen=recursive",
            " --update-lbd=less --otfs=2 --save-p=75 --counter-restarts=3,1023",
            " --reverse-arcs=2 --contraction=250 --loops=common"
        ),
    },
    ConfigView {
        name: "[frumpy]",
        base: "",
        args: concat!(
            "--eq=5 ",
            "--heuristic=Berkmin --restarts=x,100,1.5 --deletion=basic,75",
            " --del-init=3.0,200,40000 --del-max=400000 --contraction=250",
            " --loops=common --save-p=180 --del-grow=1.1 --strengthen=local",
            " --sign-def-disj=pos"
        ),
    },
    ConfigView {
        name: "[crafty]",
        base: "",
        args: concat!(
            "--sat-p=2,iter=10,occ=25,time=240 --trans-ext=dynamic --backprop",
            " --heuristic=Vsids --save-p=180 ",
            "--restarts=x,128,1.5 --deletion=basic,75 --del-init=10.0,1000,9000",
            " --del-grow=1.1,20.0 --del-cfl=+,10000,1000 --del-glue=2 --otfs=2",
            " --reverse-arcs=1 --counter-restarts=3,9973 --contraction=250"
        ),
    },
    ConfigView {
        name: "[jumpy]",
        base: "",
        args: concat!(
            "--sat-p=2,iter=20,occ=25,time=240 --trans-ext=dynamic ",
            "--heuristic=Vsids --restarts=L,100 --deletion=basic,75,mixed",
            " --del-init=3.0,1000,20000 --del-grow=1.1,25,x,100,1.5",
            " --del-cfl=x,10000,1.1 --del-glue=2 --update-lbd=glucose",
            " --strengthen=recursive --otfs=2 --save-p=70"
        ),
    },
    ConfigView {
        name: "[handy]",
        base: "",
        args: concat!(
            "--sat-p=2,iter=10,occ=25,time=240 --trans-ext=dynamic --backprop ",
            "--heuristic=Vsids --restarts=D,100,0.7 --deletion=sort,50,mixed",
            " --del-max=200000 --del-init=20.0,1000,14000 --del-cfl=+,4000,600",
            " --del-glue=2 --update-lbd=less --strengthen=recursive --otfs=2",
            " --save-p=20 --contraction=600 --loops=distinct --counter-restarts=7,1023",
            " --reverse-arcs=2"
        ),
    },
    ConfigView {
        name: "[s6]",
        base: "",
        args: concat!(
            " ",
            "--heuristic=Berkmin,512 --restarts=x,100,1.5 --deletion=basic,75",
            " --del-init=3.0,200,40000 --del-max=400000 --contraction=250",
            " --loops=common --del-grow=1.1,25 --otfs=2 --reverse-arcs=2",
            " --strengthen=recursive --init-w=least --lookahead=atom,10"
        ),
    },
    ConfigView {
        name: "[s7]",
        base: "",
        args: concat!(
            " ",
            "--heuristic=Vsids --reverse-arcs=1 --otfs=1 --local-restarts",
            " --save-progress=0 --contraction=250 --counter-restart=7,200",
            " --restarts=x,100,1.5 --del-init=3.0,800,-1 --deletion=basic,60",
            " --strengthen=local --del-grow=1.0,1.0 --del-glue=4 --del-cfl=+,4000,300,100"
        ),
    },
    ConfigView {
        name: "[s8]",
        base: "",
        args: concat!(
            " ",
            "--heuristic=Vsids --restarts=L,256 --counter-restart=3,9973",
            " --strengthen=recursive --update-lbd=less --del-glue=2 --otfs=2",
            " --deletion=ipSort,75,mixed --del-init=20.0,1000,19000"
        ),
    },
    ConfigView {
        name: "[s9]",
        base: "",
        args: " --heuristic=Berkmin,512 --restarts=F,16000 --lookahead=atom,50",
    },
    ConfigView {
        name: "[s10]",
        base: "",
        args: " --heuristic=Vmtf --strengthen=no --contr=0 --restarts=x,100,1.3 --del-init=3.0,800,9200",
    },
    ConfigView {
        name: "[s11]",
        base: "",
        args: " --heuristic=Vsids --strengthen=recursive --restarts=x,100,1.5,15 --contraction=0",
    },
    ConfigView {
        name: "[s12]",
        base: "",
        args: " --heuristic=Vsids --restarts=L,128 --save-p --otfs=1 --init-w=least --contr=0 --opt-heu=sign,model",
    },
    ConfigView {
        name: "[s13]",
        base: "",
        args: " --heuristic=Berkmin,512 --restarts=x,100,1.5,6 --local-restarts --init-w=least --contr=0",
    },
    ConfigView {
        name: "[nolearn]",
        base: "",
        args: " --no-lookback --heuristic=Unit --lookahead=atom --deletion=no --restarts=no",
    },
    ConfigView {
        name: "[tester]",
        base: "",
        args: concat!(
            "--sat-p=2,iter=10,occ=25,time=240 ",
            "--heuristic=Vsids --restarts=D,100,0.7 --deletion=sort,50,mixed",
            " --del-max=200000 --del-init=20.0,1000,14000 --del-cfl=+,4000,600",
            " --del-glue=2 --update-lbd=less --strengthen=recursive --otfs=2",
            " --save-p=20 --contraction=600 --counter-restarts=7,1023 --reverse-arcs=2"
        ),
    },
];

const MANY_CONFIGS: &[ConfigView] = &[
    ConfigView {
        name: "[solver.0]",
        base: "",
        args: concat!(
            "--heuristic=Vsids,92 --restarts=L,60 --deletion=basic,50 --del-max=2000000",
            " --del-estimate=1 --del-cfl=+,2000,100,20 --del-grow=0 --del-glue=2,0",
            " --strengthen=recursive,all --otfs=2 --init-moms --score-other=all",
            " --update-lbd=less --save-progress=160 --init-watches=least",
            " --local-restarts --loops=shared --opt-strat=bb,hier"
        ),
    },
    ConfigView {
        name: "[solver.1]",
        base: "",
        args: concat!(
            "--heuristic=Vsids --restarts=D,100,0.7 --deletion=basic,50",
            " --del-init=3.0,500,19500 --del-grow=1.1,20.0,x,100,1.5",
            " --del-cfl=+,10000,2000 --del-glue=2 --strengthen=recursive",
            " --update-lbd=less --otfs=2 --save-p=75 --counter-restarts=3,1023",
            " --reverse-arcs=2 --contraction=250 --loops=common",
            " --opt-heu=sign --opt-strat=usc,disjoint"
        ),
    },
    ConfigView {
        name: "[solver.2]",
        base: "",
        args: concat!(
            "--heuristic=Berkmin --restarts=x,100,1.5 --deletion=basic,75",
            " --del-init=3.0,200,40000 --del-max=400000 --contraction=250",
            " --loops=common --save-p=180 --del-grow=1.1 --strengthen=local",
            " --sign-def-disj=pos --restart-on-model --opt-heu=model"
        ),
    },
    ConfigView {
        name: "[solver.3]",
        base: "",
        args: concat!(
            "--restarts=x,128,1.5 --deletion=basic,75 --del-init=10.0,1000,9000",
            " --del-grow=1.1,20.0 --del-cfl=+,10000,1000 --del-glue=2 --otfs=2",
            " --reverse-arcs=1 --counter-restarts=3,9973 --contraction=250",
            " --heuristic=domain --dom-mod=neg,opt --opt-strat=bb,hier"
        ),
    },
    ConfigView {
        name: "[solver.4]",
        base: "",
        args: concat!(
            "--heuristic=Vsids --restarts=L,100 --deletion=basic,75,mixed",
            " --del-init=3.0,1000,20000 --del-grow=1.1,25,x,100,1.5",
            " --del-cfl=x,10000,1.1 --del-glue=2 --update-lbd=glucose",
            " --strengthen=recursive --otfs=2 --save-p=70",
            " --restart-on-model --opt-heu=sign,model --opt-strat=bb,inc"
        ),
    },
    ConfigView {
        name: "[solver.5]",
        base: "",
        args: concat!(
            "--heuristic=Vsids --restarts=D,100,0.7 --deletion=sort,50,mixed",
            " --del-max=200000 --del-init=20.0,1000,14000 --del-cfl=+,4000,600",
            " --del-glue=2 --update-lbd=less --strengthen=recursive --otfs=2",
            " --save-p=20 --contraction=600 --loops=distinct --counter-restarts=7,1023",
            " --reverse-arcs=2 "
        ),
    },
    ConfigView {
        name: "[solver.6]",
        base: "",
        args: "--heuristic=Berkmin,512 --restarts=x,100,1.5 --deletion=basic,75 --del-init=3.0,200,40000 --del-max=400000 --contraction=250 --loops=common --del-grow=1.1,25 --otfs=2 --reverse-arcs=2 --strengthen=recursive --init-w=least --lookahead=atom,10 ",
    },
    ConfigView {
        name: "[solver.7]",
        base: "",
        args: "--heuristic=Vsids --reverse-arcs=1 --otfs=1 --local-restarts --save-progress=0 --contraction=250 --counter-restart=7,200 --restarts=x,100,1.5 --del-init=3.0,800,-1 --deletion=basic,60 --strengthen=local --del-grow=1.0,1.0 --del-glue=4 --del-cfl=+,4000,300,100 ",
    },
    ConfigView {
        name: "[solver.8]",
        base: "",
        args: "--heuristic=Vsids --restarts=L,256 --counter-restart=3,9973 --strengthen=recursive --update-lbd=less --del-glue=2 --otfs=2 --deletion=ipSort,75,mixed --del-init=20.0,1000,19000 ",
    },
    ConfigView {
        name: "[solver.9]",
        base: "",
        args: "--heuristic=Berkmin,512 --restarts=F,16000 --lookahead=atom,50 ",
    },
    ConfigView {
        name: "[solver.10]",
        base: "",
        args: "--heuristic=Vmtf --strengthen=no --contr=0 --restarts=x,100,1.3 --del-init=3.0,800,9200 ",
    },
    ConfigView {
        name: "[solver.11]",
        base: "",
        args: "--heuristic=Vsids --strengthen=recursive --restarts=x,100,1.5,15 --contraction=0 ",
    },
    ConfigView {
        name: "[solver.12]",
        base: "",
        args: "--heuristic=Vsids --restarts=L,128 --save-p --otfs=1 --init-w=least --contr=0 --opt-heu=sign,model ",
    },
    ConfigView {
        name: "[solver.13]",
        base: "",
        args: "--heuristic=Berkmin,512 --restarts=x,100,1.5,6 --local-restarts --init-w=least --contr=0 ",
    },
    ConfigView {
        name: "[solver.14]",
        base: "",
        args: "--no-lookback --heuristic=Unit --lookahead=atom --deletion=no --restarts=no ",
    },
    ConfigView {
        name: "[solver.15]",
        base: "",
        args: concat!(
            "--heuristic=Vsids --restarts=D,100,0.7 --deletion=sort,50,mixed",
            " --del-max=200000 --del-init=20.0,1000,14000 --del-cfl=+,4000,600",
            " --del-glue=2 --update-lbd=less --strengthen=recursive --otfs=2",
            " --save-p=20 --contraction=600 --counter-restarts=7,1023 --reverse-arcs=2 "
        ),
    },
];

pub fn config_entry(key: ConfigKey) -> Option<&'static ConfigEntry> {
    let mut entries = DEFAULT_CONFIGS.iter().chain(AUX_CONFIGS.iter());
    entries.find(|entry| key_from_name(entry.name).is_some_and(|candidate| candidate == key))
}

pub fn get_config(key: ConfigKey) -> ConfigIter {
    match key {
        ConfigKey::Default => ConfigIter {
            views: core::slice::from_ref(&DEFAULT_CONFIG_VIEW),
            index: 0,
        },
        ConfigKey::Tweety => single_config(0),
        ConfigKey::Trendy => single_config(1),
        ConfigKey::Frumpy => single_config(2),
        ConfigKey::Crafty => single_config(3),
        ConfigKey::Jumpy => single_config(4),
        ConfigKey::Handy => single_config(5),
        ConfigKey::S6 => single_config(6),
        ConfigKey::S7 => single_config(7),
        ConfigKey::S8 => single_config(8),
        ConfigKey::S9 => single_config(9),
        ConfigKey::S10 => single_config(10),
        ConfigKey::S11 => single_config(11),
        ConfigKey::S12 => single_config(12),
        ConfigKey::S13 => single_config(13),
        ConfigKey::Nolearn => single_config(14),
        ConfigKey::Tester => single_config(15),
        ConfigKey::Many => ConfigIter {
            views: MANY_CONFIGS,
            index: 0,
        },
    }
}

pub const fn get_defaults(problem_type: ProblemType) -> &'static str {
    match problem_type {
        ProblemType::Asp => "--configuration=tweety",
        ProblemType::Sat | ProblemType::Pb => "--configuration=trendy",
    }
}

fn single_config(index: usize) -> ConfigIter {
    ConfigIter {
        views: &SINGLE_CONFIGS[index..index + 1],
        index: 0,
    }
}

fn key_from_name(name: &str) -> Option<ConfigKey> {
    match name {
        "tweety" => Some(ConfigKey::Tweety),
        "trendy" => Some(ConfigKey::Trendy),
        "frumpy" => Some(ConfigKey::Frumpy),
        "crafty" => Some(ConfigKey::Crafty),
        "jumpy" => Some(ConfigKey::Jumpy),
        "handy" => Some(ConfigKey::Handy),
        "s6" => Some(ConfigKey::S6),
        "s7" => Some(ConfigKey::S7),
        "s8" => Some(ConfigKey::S8),
        "s9" => Some(ConfigKey::S9),
        "s10" => Some(ConfigKey::S10),
        "s11" => Some(ConfigKey::S11),
        "s12" => Some(ConfigKey::S12),
        "s13" => Some(ConfigKey::S13),
        "nolearn" => Some(ConfigKey::Nolearn),
        "tester" => Some(ConfigKey::Tester),
        _ => None,
    }
}
