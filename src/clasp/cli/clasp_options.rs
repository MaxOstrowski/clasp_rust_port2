//! Rust port of the self-contained option conversion helpers from
//! `original_clasp/src/clasp_options.cpp`.

use core::fmt;

pub use crate::clasp::cli::clasp_cli_configs::ConfigKey;
use crate::clasp::cli::clasp_cli_options::{
    self as cli, CliEnum, HeuristicType, asp_logic_program, context_params,
    default_unfounded_check, distributor_policy, heu_params, opt_params, parse_exact,
    reduce_strategy, restart_params, restart_schedule, solve_options, solver_params,
    solver_strategies,
};
use crate::clasp::solver_strategies::{
    OptParams, RestartKeep, RestartSchedule, SatPreParams, ScheduleStrategy, ScheduleType,
};
use crate::clasp::util::misc_types::MovingAvgType;
use crate::potassco::bits;
use crate::potassco::enums::EnumTag;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    input: String,
}

impl ParseError {
    fn new(input: &str) -> Self {
        Self {
            input: input.to_owned(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to parse '{}'", self.input)
    }
}

impl std::error::Error for ParseError {}

struct Cursor<'a> {
    remaining: &'a str,
}

impl<'a> Cursor<'a> {
    fn new(input: &'a str) -> Self {
        Self { remaining: input }
    }

    fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }

    fn consume_if(&mut self, ch: char) -> bool {
        if self.remaining.starts_with(ch) {
            self.remaining = &self.remaining[ch.len_utf8()..];
            true
        } else {
            false
        }
    }

    fn expect_comma(&mut self) -> Result<(), ParseError> {
        if self.consume_if(',') {
            Ok(())
        } else {
            Err(ParseError::new(self.remaining))
        }
    }

    fn parse_u32(&mut self) -> Result<u32, ParseError> {
        let token = self.token_until_comma();
        if token.is_empty() {
            return Err(ParseError::new(self.remaining));
        }
        token.parse::<u32>().map_err(|_| ParseError::new(token))
    }

    fn parse_f64(&mut self) -> Result<f64, ParseError> {
        let token = self.token_until_comma();
        if token.is_empty() {
            return Err(ParseError::new(self.remaining));
        }
        token.parse::<f64>().map_err(|_| ParseError::new(token))
    }

    fn parse_enum<E: CliEnum>(&mut self) -> Result<E, ParseError> {
        let (value, consumed) =
            cli::from_chars::<E>(self.remaining).map_err(|_| ParseError::new(self.remaining))?;
        self.remaining = &self.remaining[consumed..];
        Ok(value)
    }

    fn token_until_comma(&mut self) -> &'a str {
        let split = self.remaining.find(',').unwrap_or(self.remaining.len());
        let token = &self.remaining[..split];
        self.remaining = &self.remaining[split..];
        token
    }
}

fn push_token(out: &mut String, token: impl AsRef<str>) {
    if !out.is_empty() {
        out.push(',');
    }
    out.push_str(token.as_ref());
}

fn push_u32(out: &mut String, value: u32) {
    push_token(out, value.to_string());
}

fn push_f32(out: &mut String, value: f32) {
    push_token(out, value.to_string());
}

fn push_enum<E: CliEnum>(out: &mut String, value: E) {
    if !out.is_empty() {
        out.push(',');
    }
    cli::to_chars(out, value);
}

fn parse_no(input: &str) -> bool {
    input.eq_ignore_ascii_case("no")
}

fn enum_bits<E>(value: E) -> u32
where
    E: EnumTag + Copy,
    E::Repr: Into<u64>,
{
    value.to_underlying().into() as u32
}

fn parse_cli_bitset<E>(input: &str) -> Result<u32, ParseError>
where
    E: CliEnum + EnumTag + Copy,
    E::Repr: Into<u64>,
{
    if let Ok(raw) = input.parse::<u32>() {
        let mut seen_mask = 0u32;
        for entry in cli::enum_map::<E>() {
            seen_mask |= enum_bits(entry.value);
            if raw == enum_bits(entry.value) || (raw != 0 && bits::test_mask(raw, seen_mask)) {
                return Ok(raw);
            }
        }
        return Err(ParseError::new(input));
    }
    let mut cursor = Cursor::new(input);
    let mut value = 0u32;
    loop {
        let next = cursor.parse_enum::<E>()?;
        value |= enum_bits(next);
        if cursor.is_empty() {
            return Ok(value);
        }
        cursor.expect_comma()?;
    }
}

fn format_cli_bitset<E>(out: &mut String, raw: u32)
where
    E: CliEnum + EnumTag + Copy,
    E::Repr: Into<u64>,
{
    if raw == 0 {
        push_token(out, "no");
        return;
    }
    let mut bitset = raw;
    for entry in cli::enum_map::<E>() {
        let value = enum_bits(entry.value);
        if bitset == value || (value != 0 && (value & bitset) == value) {
            push_token(out, entry.key);
            bitset -= value;
            if bitset == 0 {
                return;
            }
        }
    }
}

fn set_or_fill_15(value: u32) -> u32 {
    value.min((1 << 15) - 1)
}

fn set_or_zero(value: u32, max_value: u32) -> u32 {
    if value <= max_value { value } else { 0 }
}

pub fn parse_config_key(input: &str) -> Result<ConfigKey, ParseError> {
    parse_exact::<ConfigKey>(input).map_err(|_| ParseError::new(input))
}

pub fn format_config_key(value: ConfigKey) -> String {
    let mut out = String::new();
    cli::to_chars(&mut out, value);
    out
}

pub fn format_sat_pre_params(value: &SatPreParams) -> String {
    if value.type_ == 0 {
        return "no".to_owned();
    }
    let mut out = value.type_.to_string();
    let pairs = [
        ("iter=", value.lim_iters),
        ("occ=", value.lim_occ),
        ("time=", value.lim_time),
        ("frozen=", value.lim_frozen),
        ("size=", value.lim_clause),
    ];
    for (key, raw) in pairs {
        if raw > 0 {
            out.push(',');
            out.push_str(key);
            out.push_str(&raw.to_string());
        }
    }
    out
}

pub fn parse_sat_pre_params(input: &str) -> Result<SatPreParams, ParseError> {
    if parse_no(input) {
        return Ok(SatPreParams::default());
    }
    let mut cursor = Cursor::new(input);
    let sat_type = cursor.parse_u32()?;
    if sat_type > 3 {
        return Err(ParseError::new(input));
    }
    let mut params = SatPreParams {
        type_: sat_type,
        ..SatPreParams::default()
    };
    let mut values = [0u32, 0u32, 0u32, 0u32, 4000u32];
    let keys = ["iter", "occ", "time", "frozen", "size"];
    let mut index = 0usize;
    while cursor.consume_if(',') {
        let snapshot = cursor.remaining;
        let next_token = snapshot.split_once(',').map_or(snapshot, |(head, _)| head);
        let mut matched_key = None;
        for (position, key) in keys.iter().enumerate() {
            if next_token.len() >= key.len()
                && next_token[..key.len()].eq_ignore_ascii_case(key)
                && next_token[key.len()..].starts_with(['=', ':'])
            {
                matched_key = Some(position);
                break;
            }
        }
        if let Some(position) = matched_key {
            index = position;
            cursor.remaining = &cursor.remaining[keys[position].len()..];
            if !cursor.consume_if('=') {
                cursor.consume_if(':');
            }
        }
        if index > 4 {
            return Err(ParseError::new(snapshot));
        }
        values[index] = cursor.parse_u32()?;
        index += 1;
    }
    if !cursor.is_empty() {
        return Err(ParseError::new(cursor.remaining));
    }
    params.lim_iters = set_or_zero(values[0], (1 << 11) - 1);
    params.lim_occ = set_or_zero(values[1], (1 << 16) - 1);
    params.lim_time = set_or_zero(values[2], (1 << 12) - 1);
    params.lim_frozen = set_or_zero(values[3], (1 << 7) - 1);
    params.lim_clause = set_or_zero(values[4], (1 << 16) - 1);
    Ok(params)
}

pub fn format_opt_params(value: &OptParams) -> String {
    let mut out = String::new();
    cli::to_chars(
        &mut out,
        match value.type_ {
            1 => opt_params::Type::TypeUsc,
            _ => opt_params::Type::TypeBb,
        },
    );
    if value.type_ == 1 {
        push_enum(
            &mut out,
            match value.algo {
                1 => opt_params::UscAlgo::UscOne,
                2 => opt_params::UscAlgo::UscK,
                3 => opt_params::UscAlgo::UscPmr,
                _ => opt_params::UscAlgo::UscOll,
            },
        );
        if value.algo == 2 {
            push_u32(&mut out, value.k_lim);
        }
        if value.opts != 0 {
            format_cli_bitset::<opt_params::UscOption>(&mut out, value.opts);
        }
    } else {
        push_enum(
            &mut out,
            match value.algo {
                1 => opt_params::BBAlgo::BbHier,
                2 => opt_params::BBAlgo::BbInc,
                3 => opt_params::BBAlgo::BbDec,
                _ => opt_params::BBAlgo::BbLin,
            },
        );
    }
    out
}

pub fn set_opt_legacy(params: &mut OptParams, mut value: u32) -> bool {
    if value >= 20 {
        return false;
    }
    params.type_ = u32::from(value >= 4);
    params.algo = if value < 4 { value } else { 0 };
    params.opts = 0;
    params.k_lim = 0;
    if value > 4 {
        value -= 4;
        if bits::test_bit(value, 0) {
            params.opts |= opt_params::UscOption::UscDisjoint as u32;
        }
        if bits::test_bit(value, 1) {
            params.opts |= opt_params::UscOption::UscSuccinct as u32;
        }
        if bits::test_bit(value, 2) {
            params.algo = opt_params::UscAlgo::UscPmr as u32;
        }
        if bits::test_bit(value, 3) {
            params.opts |= opt_params::UscOption::UscStratify as u32;
        }
    }
    true
}

pub fn parse_opt_params(input: &str) -> Result<OptParams, ParseError> {
    if let Ok(value) = input.parse::<u32>() {
        let mut params = OptParams::default();
        if set_opt_legacy(&mut params, value) {
            return Ok(params);
        }
        return Err(ParseError::new(input));
    }
    let mut cursor = Cursor::new(input);
    let opt_type = cursor.parse_enum::<opt_params::Type>()?;
    let mut params = OptParams::default();
    let base = match opt_type {
        opt_params::Type::TypeBb => 0,
        opt_params::Type::TypeUsc => 4,
    };
    let _ = set_opt_legacy(&mut params, base);
    params.type_ = opt_type as u32;
    if cursor.consume_if(',') {
        if let Ok(legacy) = cursor.remaining.parse::<u32>() {
            let mut temp = OptParams::default();
            if set_opt_legacy(&mut temp, legacy + base) {
                return Ok(temp);
            }
            return Err(ParseError::new(input));
        }
        match opt_type {
            opt_params::Type::TypeBb => {
                params.algo = cursor.parse_enum::<opt_params::BBAlgo>()? as u32;
            }
            opt_params::Type::TypeUsc => {
                let mut algo = opt_params::UscAlgo::UscOll;
                let mut more = true;
                if let Ok(parsed) = cursor.parse_enum::<opt_params::UscAlgo>() {
                    algo = parsed;
                    if algo == opt_params::UscAlgo::UscK {
                        let snapshot = cursor.remaining;
                        if cursor.consume_if(',') {
                            params.k_lim = set_or_fill_15(cursor.parse_u32()?);
                        } else {
                            cursor.remaining = snapshot;
                        }
                    }
                    more = cursor.consume_if(',');
                }
                params.algo = algo as u32;
                if more {
                    params.opts = if parse_no(cursor.remaining) {
                        0
                    } else {
                        parse_cli_bitset::<opt_params::UscOption>(cursor.remaining)?
                    };
                    cursor.remaining = "";
                }
            }
        }
    }
    if !cursor.is_empty() {
        return Err(ParseError::new(cursor.remaining));
    }
    Ok(params)
}

pub fn format_schedule_strategy(mut value: ScheduleStrategy) -> String {
    if value.disabled() {
        return "0".to_owned();
    }
    if value.defaulted() {
        value = ScheduleStrategy::default();
    }
    let mut out = String::new();
    match value.schedule_type {
        ScheduleType::Geom => {
            push_token(&mut out, "x");
            push_u32(&mut out, value.base);
            push_f32(&mut out, value.grow);
            if value.len != 0 {
                push_u32(&mut out, value.len);
            }
        }
        ScheduleType::Arith => {
            if value.grow != 0.0 {
                push_token(&mut out, "+");
                push_u32(&mut out, value.base);
                push_u32(&mut out, value.grow as u32);
                if value.len != 0 {
                    push_u32(&mut out, value.len);
                }
            } else {
                push_token(&mut out, "f");
                push_u32(&mut out, value.base);
            }
        }
        ScheduleType::Luby => {
            push_token(&mut out, "l");
            push_u32(&mut out, value.base);
            if value.len != 0 {
                push_u32(&mut out, value.len);
            }
        }
    }
    out
}

pub fn parse_schedule_strategy(input: &str) -> Result<ScheduleStrategy, ParseError> {
    let mut cursor = Cursor::new(input);
    let token = cursor.token_until_comma();
    let kind = if token.eq_ignore_ascii_case("f") || token.eq_ignore_ascii_case("fixed") {
        'f'
    } else if token.eq_ignore_ascii_case("l") || token.eq_ignore_ascii_case("luby") {
        'l'
    } else if token.eq_ignore_ascii_case("x") || token == "*" {
        'x'
    } else if token == "+" || token.eq_ignore_ascii_case("add") {
        '+'
    } else {
        return Err(ParseError::new(input));
    };
    cursor.expect_comma()?;
    let base = cursor.parse_u32()?;
    if base == 0 {
        return Err(ParseError::new(input));
    }
    let strategy = match kind {
        'f' => {
            if !cursor.is_empty() {
                return Err(ParseError::new(cursor.remaining));
            }
            ScheduleStrategy::fixed(base)
        }
        'l' => {
            let limit = if cursor.consume_if(',') {
                cursor.parse_u32()?
            } else {
                0
            };
            if !cursor.is_empty() {
                return Err(ParseError::new(cursor.remaining));
            }
            ScheduleStrategy::luby(base, limit)
        }
        'x' => {
            cursor.expect_comma()?;
            let grow = cursor.parse_f64()?;
            let limit = if cursor.consume_if(',') {
                cursor.parse_u32()?
            } else {
                0
            };
            if !cursor.is_empty() {
                return Err(ParseError::new(cursor.remaining));
            }
            ScheduleStrategy::geom(base, grow, limit)
        }
        _ => {
            cursor.expect_comma()?;
            let grow = cursor.parse_u32()?;
            let limit = if cursor.consume_if(',') {
                cursor.parse_u32()?
            } else {
                0
            };
            if !cursor.is_empty() {
                return Err(ParseError::new(cursor.remaining));
            }
            ScheduleStrategy::arith(base, grow, limit)
        }
    };
    Ok(strategy)
}

pub fn format_restart_schedule(value: &RestartSchedule) -> String {
    if value.disabled() || !value.is_dynamic() {
        return format_schedule_strategy(value.as_schedule());
    }
    let mut out = String::from("d");
    push_u32(&mut out, value.base);
    push_f32(&mut out, value.grow);
    let limit = value.lbd_lim();
    let fast = value.fast_avg();
    let slow = value.slow_avg();
    if limit != 0 || fast != MovingAvgType::AvgSma || slow != MovingAvgType::AvgSma {
        push_u32(&mut out, limit);
    }
    if fast != MovingAvgType::AvgSma || slow != MovingAvgType::AvgSma {
        push_enum(&mut out, fast);
    }
    if fast != MovingAvgType::AvgSma && value.keep_avg() != RestartKeep::Never {
        push_enum(
            &mut out,
            match value.keep_avg() {
                RestartKeep::Restart => restart_schedule::Keep::KeepRestart,
                RestartKeep::Block => restart_schedule::Keep::KeepBlock,
                RestartKeep::Always => restart_schedule::Keep::KeepAlways,
                RestartKeep::Never => restart_schedule::Keep::KeepNever,
            },
        );
    }
    if slow != MovingAvgType::AvgSma {
        push_enum(&mut out, slow);
        if value.slow_win() != 0 {
            push_u32(&mut out, value.slow_win());
        }
    }
    out
}

pub fn parse_restart_schedule(input: &str) -> Result<RestartSchedule, ParseError> {
    if !(input.starts_with("d,") || input.starts_with("D,")) {
        return parse_schedule_strategy(input).map(RestartSchedule::from_schedule);
    }
    let mut cursor = Cursor::new(&input[2..]);
    let base = cursor.parse_u32()?;
    cursor.expect_comma()?;
    let k = cursor.parse_f64()?;
    if base == 0 || k <= 0.0 {
        return Err(ParseError::new(input));
    }
    let mut limit = 0u32;
    let mut fast = MovingAvgType::AvgSma;
    let mut slow = MovingAvgType::AvgSma;
    let mut keep = RestartKeep::Never;
    let mut slow_win = 0u32;
    if cursor.consume_if(',') {
        limit = cursor.parse_u32()?;
    }
    if cursor.consume_if(',') {
        fast = cursor.parse_enum::<MovingAvgType>()?;
    }
    if fast != MovingAvgType::AvgSma {
        let snapshot = cursor.remaining;
        if cursor.consume_if(',') {
            if let Ok(parsed) = cursor.parse_enum::<restart_schedule::Keep>() {
                keep = match parsed {
                    restart_schedule::Keep::KeepRestart => RestartKeep::Restart,
                    restart_schedule::Keep::KeepBlock => RestartKeep::Block,
                    restart_schedule::Keep::KeepAlways => RestartKeep::Always,
                    restart_schedule::Keep::KeepNever => RestartKeep::Never,
                };
            } else {
                cursor.remaining = snapshot;
            }
        }
    }
    if cursor.consume_if(',') {
        slow = cursor.parse_enum::<MovingAvgType>()?;
    }
    if slow != MovingAvgType::AvgSma && cursor.consume_if(',') {
        slow_win = cursor.parse_u32()?;
    }
    if !cursor.is_empty() {
        return Err(ParseError::new(cursor.remaining));
    }
    Ok(RestartSchedule::dynamic(
        base, k as f32, limit, fast, keep, slow, slow_win,
    ))
}

pub fn parse_bool_flag(input: &str) -> Result<bool, ParseError> {
    if input.eq_ignore_ascii_case("true") || input == "1" {
        Ok(true)
    } else if input.eq_ignore_ascii_case("false") || input == "0" || parse_no(input) {
        Ok(false)
    } else {
        Err(ParseError::new(input))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OffType;

const _: fn() = || {
    let _ = (
        HeuristicType::Def,
        context_params::ShareMode::ShareNo,
        default_unfounded_check::ReasonStrategy::CommonReason,
        distributor_policy::Types::No,
        heu_params::Score::ScoreAuto,
        reduce_strategy::Algorithm::ReduceLinear,
        restart_params::SeqUpdate::SeqContinue,
        solve_options::EnumType::EnumAuto,
        solver_params::Forget::ForgetHeuristic,
        solver_strategies::SignHeu::SignAtom,
        asp_logic_program::AtomSorting::SortAuto,
    );
};
