//! Partial Rust port of the base `SatPreprocessor` implementation from
//! `original_clasp/clasp/shared_context.h` and `original_clasp/src/shared_context.cpp`.
//!
//! The shared base-layer clause/unit handling is ported here. The concrete
//! SatElite preprocessing algorithm still remains blocked on the larger
//! satelite runtime and is therefore modeled as the current identity backend.

use crate::clasp::literal::{LitVec, Literal, ValueVec};
use crate::clasp::satelite::SatPreClause;
use crate::clasp::shared_context::SharedContext;
use crate::clasp::solver_strategies::{Configuration, SatPreParams};
use crate::clasp::util::misc_types::Range32;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SatPreStats {
    pub cl_removed: u32,
    pub cl_added: u32,
    pub lits_removed: u32,
}

#[derive(Debug, Clone)]
pub struct SatPreprocessor {
    clauses: Vec<SatPreClause>,
    units: Vec<Literal>,
    eliminated: Vec<SatPreClause>,
    seen: Range32,
    attached: u32,
    pub stats: SatPreStats,
}

impl Default for SatPreprocessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SatPreprocessor {
    pub fn new() -> Self {
        Self {
            clauses: Vec::new(),
            units: Vec::new(),
            eliminated: Vec::new(),
            seen: Range32::new(1, 1),
            attached: 0,
            stats: SatPreStats::default(),
        }
    }

    pub fn num_clauses(&self) -> u32 {
        self.clauses.len() as u32
    }

    pub fn add_clause(&mut self, clause: &[Literal]) -> bool {
        if clause.is_empty() {
            return false;
        }
        if clause.len() > 1 {
            self.clauses.push(SatPreClause::new(clause.to_vec()));
        } else {
            self.units.push(clause[0]);
        }
        true
    }

    pub fn preprocess(&mut self, ctx: &mut SharedContext) -> bool {
        let mut opts = ctx.configuration().context().sat_pre;
        self.preprocess_with_options(ctx, &mut opts)
    }

    pub fn preprocess_with_options(
        &mut self,
        ctx: &mut SharedContext,
        opts: &mut SatPreParams,
    ) -> bool {
        let result = (|| {
            if !self.add_units(ctx) {
                return false;
            }
            if !ctx.master().propagate() {
                return false;
            }
            if ctx.preserve_models() {
                opts.disable_bce();
            }
            if opts.type_ != 0
                && !opts.clause_limit(self.num_clauses())
                && !self.frozen_limit(ctx, *opts)
                && self.init_preprocess(ctx, opts)
            {
                self.freeze_seen(ctx);
                if !self.attach_clauses(ctx) || !self.do_preprocess(ctx) {
                    return false;
                }
            }
            if !ctx.master().simplify() {
                return false;
            }
            for clause in self.clauses.drain(..) {
                if !clause.add_to(ctx.master()) {
                    return false;
                }
            }
            true
        })();
        self.finish(ctx.num_vars());
        result
    }

    pub fn propagate(&mut self, ctx: &mut SharedContext) -> bool {
        self.add_units(ctx) && ctx.master().propagate() && self.attach_clauses(ctx)
    }

    pub fn extend_model(&mut self, model: &mut ValueVec, open: &mut LitVec) {
        if let Some(last) = open.last_mut() {
            *last = !*last;
        }
        self.do_extend_model(model, open);
        while open.last().is_some_and(|lit| lit.sign()) {
            open.pop_back();
        }
    }

    fn freeze_seen(&mut self, ctx: &mut SharedContext) {
        if !ctx.valid_var(self.seen.lo) {
            self.seen.lo = 1;
        }
        if !ctx.valid_var(self.seen.hi) {
            self.seen.hi = ctx.num_vars() + 1;
        }
        for var in self.seen.lo..self.seen.hi {
            if !ctx.eliminated(var) {
                ctx.set_frozen(var, true);
            }
        }
        self.seen.lo = self.seen.hi;
    }

    fn add_units(&mut self, ctx: &mut SharedContext) -> bool {
        if self.units.iter().copied().all(|lit| ctx.add_unary(lit)) {
            self.units.clear();
            true
        } else {
            false
        }
    }

    fn attach_clauses(&mut self, ctx: &mut SharedContext) -> bool {
        let num_vars = ctx.num_vars();
        ctx.master().acquire_problem_var(num_vars);
        let mut write = self.attached as usize;
        for index in self.attached as usize..self.clauses.len() {
            let mut clause = self.clauses[index].clone();
            let keep = {
                let solver = ctx.master();
                clause.simplify(solver);
                clause.size() > 1
                    && solver.value(clause.lit(0).var()) == crate::clasp::literal::value_free
            };
            if keep {
                self.clauses[write] = clause;
                write += 1;
            } else if !ctx.add_unary(clause.lit(0)) {
                self.clauses.truncate(write);
                return false;
            }
        }
        self.clauses.truncate(write);
        self.attached = write as u32;
        self.do_attach_clauses(ctx)
    }

    fn frozen_limit(&self, ctx: &SharedContext, opts: SatPreParams) -> bool {
        opts.lim_frozen != 0 && ctx.stats().vars.frozen > opts.lim_frozen
    }

    fn finish(&mut self, num_vars: u32) {
        self.seen.hi = num_vars + 1;
        self.clauses.clear();
        self.units.clear();
        self.eliminated.clear();
        self.attached = 0;
        self.do_clean_up();
    }

    fn init_preprocess(&mut self, _ctx: &mut SharedContext, _opts: &mut SatPreParams) -> bool {
        true
    }

    fn do_attach_clauses(&mut self, _ctx: &mut SharedContext) -> bool {
        true
    }

    fn do_preprocess(&mut self, _ctx: &mut SharedContext) -> bool {
        true
    }

    fn do_extend_model(&mut self, _model: &mut ValueVec, _open: &mut LitVec) {}

    fn do_clean_up(&mut self) {}
}
