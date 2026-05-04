//! Port target for original_clasp/libpotassco/potassco/program_opts/program_options.h,
//! original_clasp/libpotassco/src/program_options.cpp.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{BufRead, Write};
use std::marker::PhantomData;
use std::ops::{BitOr, BitOrAssign, Index};

use super::errors::{
    ContextError, ContextErrorType, Error, SyntaxError, SyntaxErrorType, ValueError,
    ValueErrorType, quote,
};
use super::intrusive_ptr::{IntrusiveRefCounted, IntrusiveSharedPtr};
use super::value::{Str, ValueActionPtr, ValueDesc};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum DescriptionLevel {
    #[default]
    Default = 0,
    E1 = 1,
    E2 = 2,
    E3 = 3,
    All = 4,
    Hidden = 5,
}

pub type SharedOption<'a> = IntrusiveSharedPtr<Option<'a>>;

pub struct Option<'a> {
    name: String,
    description: String,
    arg_name: String,
    implicit_value: String,
    action: RefCell<std::option::Option<ValueActionPtr<'a>>>,
    default_value: RefCell<String>,
    id: u32,
    ref_count: Cell<i32>,
    alias: char,
    level: DescriptionLevel,
    implicit: bool,
    flag: bool,
    composing: bool,
    negatable: bool,
    defaulted: Cell<bool>,
}

impl<'a> Option<'a> {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        value: ValueDesc<'a>,
    ) -> Self {
        Self::with_alias(name, description, value, '\0')
    }

    pub fn with_alias(
        name: impl Into<String>,
        description: impl Into<String>,
        value: ValueDesc<'a>,
        alias: char,
    ) -> Self {
        let name = name.into();
        assert!(!name.is_empty(), "option name must not be empty");
        let arg_name = if !value.arg_name.empty() || value.flag {
            value.arg_name.str().to_owned()
        } else {
            "<arg>".to_owned()
        };
        let implicit_value = if !value.implicit_value.empty() || !value.flag {
            value.implicit_value.str().to_owned()
        } else {
            "1".to_owned()
        };
        let default_value = value.default_value.str().to_owned();
        Self {
            name,
            description: description.into(),
            arg_name,
            implicit_value,
            action: RefCell::new(value.action),
            default_value: RefCell::new(default_value),
            id: value.id,
            ref_count: Cell::new(1),
            alias,
            level: value.level,
            implicit: value.implicit,
            flag: value.flag,
            composing: value.composing,
            negatable: value.negatable,
            defaulted: Cell::new(value.defaulted),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn alias(&self) -> char {
        self.alias
    }

    pub fn description_text(&self) -> &str {
        &self.description
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn arg_name(&self) -> &str {
        &self.arg_name
    }

    pub fn default_value(&self) -> String {
        self.default_value.borrow().clone()
    }

    pub fn implicit_value(&self) -> &str {
        &self.implicit_value
    }

    pub fn desc_level(&self) -> DescriptionLevel {
        self.level
    }

    pub fn negatable(&self) -> bool {
        self.negatable
    }

    pub fn composing(&self) -> bool {
        self.composing
    }

    pub fn implicit(&self) -> bool {
        self.implicit
    }

    pub fn flag(&self) -> bool {
        self.flag
    }

    pub fn defaulted(&self) -> bool {
        self.defaulted.get()
    }

    pub fn assign(&self, value: &str) -> bool {
        self.assign_internal(value, false)
    }

    pub fn assign_default(&self) -> bool {
        let default_value = self.default_value.borrow().clone();
        if default_value.is_empty() || self.defaulted() {
            return true;
        }
        self.assign_internal(&default_value, true)
    }

    pub fn assign_default_with(&self, default_value: impl Into<String>) -> bool {
        let default_value = default_value.into();
        if *self.default_value.borrow() != default_value {
            *self.default_value.borrow_mut() = default_value;
            self.defaulted.set(false);
            return self.assign_default();
        }
        true
    }

    pub fn format_description<'b>(&self, out: &'b mut String) -> &'b mut String {
        let mut desc = self.description.as_str();
        out.reserve(desc.len());
        loop {
            let next_end = desc.find('%').unwrap_or(desc.len());
            out.push_str(&desc[..next_end]);
            if next_end == desc.len() {
                break;
            }
            desc = &desc[next_end + 1..];
            if desc.is_empty() {
                break;
            }
            match desc.as_bytes()[0] {
                b'A' => out.push_str(&self.arg_name),
                b'D' => out.push_str(&self.default_value.borrow()),
                b'I' => out.push_str(&self.implicit_value),
                other => out.push(other as char),
            }
            desc = &desc[1..];
        }
        out
    }

    fn assign_internal(&self, value: &str, defaulted: bool) -> bool {
        let assigned = if value.is_empty() && self.implicit {
            self.implicit_value.as_str()
        } else {
            value
        };
        let ok = match self.action.borrow_mut().as_mut() {
            Some(action) => action.assign(self, assigned),
            None => true,
        };
        if ok {
            self.defaulted.set(defaulted);
        }
        ok
    }
}

impl<'a> IntrusiveRefCounted for Option<'a> {
    fn intrusive_add_ref(&self) {
        self.ref_count.set(self.ref_count.get() + 1);
    }

    fn intrusive_release(&self) -> i32 {
        let next = self.ref_count.get() - 1;
        self.ref_count.set(next);
        next
    }

    fn intrusive_count(&self) -> i32 {
        self.ref_count.get()
    }
}

#[derive(Clone, Default)]
pub struct OptionGroup<'a> {
    caption: String,
    options: Vec<SharedOption<'a>>,
    level: DescriptionLevel,
}

impl<'a> OptionGroup<'a> {
    pub fn new(caption: impl Into<String>, desc_level: DescriptionLevel) -> Self {
        Self {
            caption: caption.into(),
            options: Vec::new(),
            level: desc_level,
        }
    }

    pub fn caption(&self) -> &str {
        &self.caption
    }

    pub fn empty(&self) -> bool {
        self.options.is_empty()
    }

    pub fn size(&self) -> usize {
        self.options.len()
    }

    pub fn options(&self) -> &[SharedOption<'a>] {
        &self.options
    }

    pub fn desc_level(&self) -> DescriptionLevel {
        self.level
    }

    pub fn find_by_name(&self, name: &str) -> std::option::Option<SharedOption<'a>> {
        self.options.iter().find(|opt| opt.name() == name).cloned()
    }

    pub fn find_by_alias(&self, alias: char) -> std::option::Option<SharedOption<'a>> {
        self.options
            .iter()
            .find(|opt| opt.alias() == alias)
            .cloned()
    }

    pub fn add_options(&mut self) -> OptionGroupInit<'_, 'a> {
        OptionGroupInit::new_group(self)
    }

    pub fn add_option(&mut self, option: Option<'a>) {
        self.add_shared_option(IntrusiveSharedPtr::new(option));
    }

    pub fn add_shared_option(&mut self, option: SharedOption<'a>) {
        self.options.push(option);
    }

    pub fn format(
        &self,
        out: &mut dyn OptionOutput<'a>,
        max_width: usize,
        level: DescriptionLevel,
    ) {
        for option in &self.options {
            if option.desc_level() <= level {
                out.print_option(option, max_width);
            }
        }
    }

    pub fn max_column(&self, out: &mut dyn OptionOutput<'a>, level: DescriptionLevel) -> usize {
        self.options
            .iter()
            .filter(|opt| opt.desc_level() <= level)
            .map(|opt| out.column_width(opt))
            .max()
            .unwrap_or(0)
    }

    pub fn set_description_level(&mut self, level: DescriptionLevel) {
        self.level = level;
    }
}

impl<'a> Index<usize> for OptionGroup<'a> {
    type Output = SharedOption<'a>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.options[index]
    }
}

enum InitTarget<'ctx, 'a> {
    Group(&'ctx mut OptionGroup<'a>),
    Context(&'ctx mut OptionContext<'a>, usize),
}

pub struct OptionGroupInit<'ctx, 'a> {
    target: InitTarget<'ctx, 'a>,
}

impl<'ctx, 'a> OptionGroupInit<'ctx, 'a> {
    pub fn apply_spec(spec: &str, value: &mut ValueDesc<'a>, alias: &mut char) -> bool {
        *alias = '\0';
        let mut seen = 0u8;
        let mut rest = spec;
        while !rest.is_empty() {
            let head = rest.as_bytes()[0] as char;
            let bit = match head {
                '+' => 0,
                '!' => 1,
                '*' => 2,
                '-' => 3,
                '@' => 4,
                _ => break,
            };
            if (seen & (1 << bit)) != 0 {
                break;
            }
            seen |= 1 << bit;
            match head {
                '+' => {
                    *value = std::mem::take(value).composing();
                    rest = &rest[1..];
                }
                '!' => {
                    *value = std::mem::take(value).negatable();
                    rest = &rest[1..];
                }
                '*' => {
                    *value = std::mem::take(value).flag();
                    rest = &rest[1..];
                }
                '-' => {
                    let mut chars = rest.chars();
                    let _ = chars.next();
                    let Some(next) = chars.next() else {
                        break;
                    };
                    *alias = next;
                    let consumed = 1 + next.len_utf8();
                    rest = &rest[consumed..];
                }
                '@' => {
                    if rest.len() < 2 {
                        break;
                    }
                    let level = rest.as_bytes()[1];
                    if !(level.is_ascii_digit()) {
                        break;
                    }
                    let level = level - b'0';
                    if level > DescriptionLevel::Hidden as u8 {
                        break;
                    }
                    *value = std::mem::take(value).level(match level {
                        0 => DescriptionLevel::Default,
                        1 => DescriptionLevel::E1,
                        2 => DescriptionLevel::E2,
                        3 => DescriptionLevel::E3,
                        4 => DescriptionLevel::All,
                        _ => DescriptionLevel::Hidden,
                    });
                    rest = &rest[2..];
                }
                _ => unreachable!(),
            }
        }
        rest.is_empty()
    }

    pub fn add(
        &mut self,
        name: impl Into<Str>,
        value: ValueDesc<'a>,
        desc: impl Into<Str>,
    ) -> Result<&mut Self, Error> {
        let mut name = name.into();
        let mut spec = String::new();
        if let Some(pos) = name.str().find(',') {
            spec.push_str(&name.str()[..pos]);
            name.remove_prefix(pos + 1);
        }
        self.add_spec(name, &spec, value, desc)
    }

    pub fn add_spec(
        &mut self,
        name: impl Into<Str>,
        spec: &str,
        mut value: ValueDesc<'a>,
        desc: impl Into<Str>,
    ) -> Result<&mut Self, Error> {
        let name = name.into();
        if name.empty() {
            return Err(Error::message("Invalid empty option name"));
        }
        if name.str().contains(',') {
            return Err(Error::message(format!(
                "Invalid comma in name {}",
                quote(name.str())
            )));
        }
        let mut alias = '\0';
        if !Self::apply_spec(spec, &mut value, &mut alias) {
            return Err(Error::message(format!(
                "Invalid option spec {} for option {}",
                quote(spec),
                quote(name.str())
            )));
        }
        let option = Option::with_alias(name.str(), desc.into().str(), value, alias);
        match &mut self.target {
            InitTarget::Group(group) => group.add_option(option),
            InitTarget::Context(ctx, group_id) => ctx.add_option(*group_id, option)?,
        }
        Ok(self)
    }

    fn new_group(group: &'ctx mut OptionGroup<'a>) -> Self {
        Self {
            target: InitTarget::Group(group),
        }
    }

    fn new_context(ctx: &'ctx mut OptionContext<'a>, group_id: usize) -> Self {
        Self {
            target: InitTarget::Context(ctx, group_id),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FindType(u8);

impl FindType {
    pub const NAME: Self = Self(1);
    pub const PREFIX: Self = Self(2);
    pub const NAME_OR_PREFIX: Self = Self(Self::NAME.0 | Self::PREFIX.0);
    pub const ALIAS: Self = Self(4);

    fn contains(self, rhs: Self) -> bool {
        (self.0 & rhs.0) != 0
    }
}

impl BitOr for FindType {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for FindType {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParsedOptions {
    parsed: BTreeSet<String>,
}

impl ParsedOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn empty(&self) -> bool {
        self.parsed.is_empty()
    }

    pub fn size(&self) -> usize {
        self.parsed.len()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.parsed.contains(name)
    }

    pub fn add(&mut self, name: impl Into<String>) {
        self.parsed.insert(name.into());
    }

    pub fn merge(&mut self, other: ParsedOptions) {
        self.parsed.extend(other.parsed);
    }
}

#[derive(Clone, Default)]
pub struct OptionContext<'a> {
    index: BTreeMap<String, usize>,
    options: Vec<SharedOption<'a>>,
    groups: Vec<OptionGroup<'a>>,
    caption: String,
    desc_level: DescriptionLevel,
}

impl<'a> OptionContext<'a> {
    pub fn new(caption: impl Into<String>, desc_default: DescriptionLevel) -> Self {
        Self {
            index: BTreeMap::new(),
            options: Vec::new(),
            groups: Vec::new(),
            caption: caption.into(),
            desc_level: desc_default,
        }
    }

    pub fn caption(&self) -> &str {
        &self.caption
    }

    pub fn size(&self) -> usize {
        self.options.len()
    }

    pub fn groups(&self) -> usize {
        self.groups.len()
    }

    pub fn add(&mut self, group: OptionGroup<'a>) -> Result<&mut Self, Error> {
        let group_index = self.add_group(group.caption(), group.desc_level(), None);
        let options = group.options().to_vec();
        for option in &options {
            self.add_to_index(option.clone())?;
        }
        self.groups[group_index].options.extend(options);
        Ok(self)
    }

    pub fn add_context(&mut self, other: &OptionContext<'a>) -> Result<&mut Self, Error> {
        if std::ptr::eq(self, other) {
            return Ok(self);
        }
        for group in &other.groups {
            self.add(group.clone())?;
        }
        Ok(self)
    }

    pub fn add_options(
        &mut self,
        caption: impl Into<String>,
        desc_level: DescriptionLevel,
    ) -> OptionGroupInit<'_, 'a> {
        let caption = caption.into();
        let mut group_id = 0;
        let _ = self.add_group(&caption, desc_level, Some(&mut group_id));
        OptionGroupInit::new_context(self, group_id)
    }

    pub fn add_alias(
        &mut self,
        index: usize,
        alias_name: impl Into<String>,
    ) -> Result<&mut Self, Error> {
        let alias_name = alias_name.into();
        if index < self.options.len() && !alias_name.is_empty() {
            if self.index.contains_key(&alias_name) {
                return Err(ContextError::new(
                    self.caption(),
                    ContextErrorType::DuplicateOption,
                    alias_name,
                    "",
                )
                .into());
            }
            self.index.insert(alias_name, index);
        }
        Ok(self)
    }

    pub fn option(&self, name: &str, find_type: FindType) -> Result<SharedOption<'a>, Error> {
        Ok(self.options[self.find_option(name, find_type)?].clone())
    }

    pub fn index_of(&self, name: &str, find_type: FindType) -> Result<usize, Error> {
        self.find_option(name, find_type)
    }

    pub fn group(&self, caption: &str) -> Result<&OptionGroup<'a>, Error> {
        self.find_group_key(caption)
            .map(|index| &self.groups[index])
            .ok_or_else(|| {
                ContextError::new(self.caption(), ContextErrorType::UnknownGroup, caption, "")
                    .into()
            })
    }

    pub fn set_active_desc_level(&mut self, level: DescriptionLevel) {
        self.desc_level = level.min(DescriptionLevel::All);
    }

    pub fn get_active_desc_level(&self) -> DescriptionLevel {
        self.desc_level
    }

    pub fn description(&self, out: &mut dyn OptionOutput<'a>) {
        let desc_level = self.desc_level;
        if !out.print_context(self) || self.groups.is_empty() {
            return;
        }
        let mut max_width = 23;
        for group in &self.groups {
            max_width = max_width.max(group.max_column(out, desc_level));
        }
        for group in self.groups.iter().skip(1) {
            if group.desc_level() <= desc_level && out.print_group(group) {
                group.format(out, max_width, desc_level);
            }
        }
        let group = &self.groups[0];
        if group.desc_level() <= desc_level && out.print_group(group) {
            group.format(out, max_width, desc_level);
        }
    }

    pub fn format_description<F>(&self, formatter: &F) -> String
    where
        F: OptionFormatter + ?Sized,
    {
        let desc_level = self.desc_level;
        let mut output = String::new();
        formatter.format_context(&mut output, self);
        if self.groups.is_empty() {
            return output;
        }
        let mut max_width = 23;
        for group in &self.groups {
            max_width = max_width.max(
                group
                    .options()
                    .iter()
                    .map(|option| formatter.column_width(option))
                    .max()
                    .unwrap_or(0),
            );
        }
        for group in self.groups.iter().skip(1) {
            if group.desc_level() <= desc_level {
                formatter.format_group(&mut output, group);
                for option in group.options() {
                    if option.desc_level() <= desc_level {
                        formatter.format_option(&mut output, option, max_width);
                    }
                }
            }
        }
        let group = &self.groups[0];
        if group.desc_level() <= desc_level {
            formatter.format_group(&mut output, group);
            for option in group.options() {
                if option.desc_level() <= desc_level {
                    formatter.format_option(&mut output, option, max_width);
                }
            }
        }
        output
    }

    pub fn defaults(&self, prefix_size: usize) -> String {
        let mut defaults = String::new();
        if self.groups.is_empty() {
            return defaults;
        }
        let mut tmp = String::new();
        let mut written = prefix_size;
        for group in self.groups.iter().skip(1) {
            append_defaults(
                &mut defaults,
                group,
                self.desc_level,
                &mut tmp,
                &mut written,
                prefix_size,
            );
        }
        append_defaults(
            &mut defaults,
            &self.groups[0],
            self.desc_level,
            &mut tmp,
            &mut written,
            prefix_size,
        );
        defaults
    }

    pub fn assign_defaults(&self, exclude: &ParsedOptions) -> Result<(), Error> {
        for option in &self.options {
            if !exclude.contains(option.name()) && !option.assign_default() {
                return Err(ValueError::new(
                    self.caption(),
                    ValueErrorType::InvalidDefault,
                    option.name(),
                    option.default_value(),
                    "",
                )
                .into());
            }
        }
        Ok(())
    }

    fn add_option(&mut self, group_id: usize, option: Option<'a>) -> Result<(), Error> {
        let shared = IntrusiveSharedPtr::new(option);
        self.add_to_index(shared.clone())?;
        self.groups[group_id].add_shared_option(shared);
        Ok(())
    }

    fn find_group_key(&self, caption: &str) -> std::option::Option<usize> {
        self.groups
            .iter()
            .position(|group| group.caption() == caption)
    }

    fn add_group(
        &mut self,
        caption: &str,
        level: DescriptionLevel,
        group_id: std::option::Option<&mut usize>,
    ) -> usize {
        let index = match self.find_group_key(caption) {
            Some(index) => index,
            None => {
                self.groups.push(OptionGroup::new(caption, level));
                self.groups.len() - 1
            }
        };
        let merged_level = self.groups[index].desc_level().min(level);
        self.groups[index].set_description_level(merged_level);
        if let Some(group_id) = group_id {
            *group_id = index;
        }
        index
    }

    fn add_to_index(&mut self, option: SharedOption<'a>) -> Result<(), Error> {
        let key = self.options.len();
        if option.alias() != '\0' {
            let alias = format!("-{}", option.alias());
            if self.index.contains_key(&alias) {
                return Err(ContextError::new(
                    self.caption(),
                    ContextErrorType::DuplicateOption,
                    option.name(),
                    "",
                )
                .into());
            }
            self.index.insert(alias, key);
        }
        if self.index.contains_key(option.name()) {
            return Err(ContextError::new(
                self.caption(),
                ContextErrorType::DuplicateOption,
                option.name(),
                "",
            )
            .into());
        }
        self.index.insert(option.name().to_owned(), key);
        self.options.push(option);
        Ok(())
    }

    fn find_option(&self, name: &str, find_type: FindType) -> Result<usize, Error> {
        let alias_name = if find_type == FindType::ALIAS && !name.starts_with('-') {
            format!("-{name}")
        } else {
            name.to_owned()
        };
        let name = alias_name.as_str();
        if ((find_type.contains(FindType::ALIAS) && name.starts_with('-'))
            || (find_type.contains(FindType::NAME) && !name.starts_with('-')))
            && self.index.contains_key(name)
        {
            return Ok(self.index[name]);
        }
        if find_type.contains(FindType::PREFIX) {
            let start = name.to_owned();
            let matches: Vec<_> = self
                .index
                .range(start..)
                .take_while(|(key, _)| key.starts_with(name))
                .filter(|(key, _)| !key.starts_with('-'))
                .map(|(key, index)| (key.clone(), *index))
                .collect();
            if matches.len() == 1 {
                return Ok(matches[0].1);
            }
            if matches.len() > 1 {
                let alternatives = matches
                    .iter()
                    .map(|(key, _)| format!("  {key}\n"))
                    .collect::<String>();
                return Err(ContextError::new(
                    self.caption(),
                    ContextErrorType::AmbiguousOption,
                    name,
                    alternatives,
                )
                .into());
            }
        }
        Err(ContextError::new(self.caption(), ContextErrorType::UnknownOption, name, "").into())
    }
}

impl fmt::Display for OptionContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut output = String::new();
        DefaultFormat::format_context(&mut output, self);
        if !self.groups.is_empty() {
            let mut max_width = 23;
            for group in &self.groups {
                max_width = max_width.max(
                    group
                        .options()
                        .iter()
                        .map(|option| DefaultFormat::column_width(option))
                        .max()
                        .unwrap_or(0),
                );
            }
            for group in self.groups.iter().skip(1) {
                if group.desc_level() <= self.desc_level {
                    DefaultFormat::format_group(&mut output, group, None);
                    for option in group.options() {
                        if option.desc_level() <= self.desc_level {
                            DefaultFormat::format_option(&mut output, option, max_width, None);
                        }
                    }
                }
            }
            let group = &self.groups[0];
            if group.desc_level() <= self.desc_level {
                DefaultFormat::format_group(&mut output, group, None);
                for option in group.options() {
                    if option.desc_level() <= self.desc_level {
                        DefaultFormat::format_option(&mut output, option, max_width, None);
                    }
                }
            }
        }
        f.write_str(&output)
    }
}

fn append_defaults(
    out: &mut String,
    group: &OptionGroup<'_>,
    level: DescriptionLevel,
    tmp: &mut String,
    written: &mut usize,
    prefix_len: usize,
) {
    if group.desc_level() > level {
        return;
    }
    let mut space = usize::from(!out.is_empty() && !out.ends_with(' '));
    for option in group.options() {
        let default_value = option.default_value();
        if !default_value.is_empty() && option.desc_level() <= level {
            tmp.push_str(&" ".repeat(space));
            tmp.push_str("--");
            tmp.push_str(option.name());
            tmp.push('=');
            tmp.push_str(&default_value);
            if tmp.len() + *written > 78 {
                out.push('\n');
                out.push_str(&" ".repeat(prefix_len));
                *written = prefix_len;
            }
            out.push_str(tmp);
            *written += tmp.len();
            tmp.clear();
            space = 1;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptState {
    Open,
    Seen,
    Skip,
}

pub trait ParseContext<'a> {
    fn name(&self) -> &str;
    fn state(&self, opt: &Option<'a>) -> OptState;
    fn do_get_option(&self, name: &str, find_type: FindType) -> Result<SharedOption<'a>, Error>;
    fn do_set_value(&mut self, opt: &SharedOption<'a>, value: &str) -> Result<bool, Error>;
    fn do_finish(&mut self, error: std::option::Option<&Error>);

    fn get_option(&self, name: &str, find_type: FindType) -> Result<SharedOption<'a>, Error> {
        self.do_get_option(name, find_type)
    }

    fn set_value(&mut self, opt: &SharedOption<'a>, value: &str) -> Result<(), Error> {
        if !opt.composing() {
            match self.state(opt) {
                OptState::Skip => return Ok(()),
                OptState::Seen => {
                    return Err(ValueError::new(
                        self.name(),
                        ValueErrorType::MultipleOccurrences,
                        opt.name(),
                        value,
                        "",
                    )
                    .into());
                }
                OptState::Open => {}
            }
        }
        match self.do_set_value(opt, value) {
            Ok(true) => Ok(()),
            Ok(false) => Err(ValueError::new(
                self.name(),
                ValueErrorType::InvalidValue,
                opt.name(),
                value,
                "",
            )
            .into()),
            Err(Error::Message(message)) => Err(ValueError::new(
                self.name(),
                ValueErrorType::InvalidValue,
                opt.name(),
                value,
                message,
            )
            .into()),
            Err(error) => Err(error),
        }
    }

    fn finish(&mut self, error: std::option::Option<&Error>) {
        self.do_finish(error);
    }
}

pub struct DefaultParseContext<'ctx, 'a> {
    ctx: &'ctx OptionContext<'a>,
    parsed: ParsedOptions,
    seen: ParsedOptions,
}

impl<'ctx, 'a> DefaultParseContext<'ctx, 'a> {
    pub fn new(ctx: &'ctx OptionContext<'a>) -> Self {
        Self {
            ctx,
            parsed: ParsedOptions::new(),
            seen: ParsedOptions::new(),
        }
    }

    pub fn parsed(&self) -> &ParsedOptions {
        &self.parsed
    }

    pub fn clear_parsed(&mut self) -> &mut Self {
        self.parsed = ParsedOptions::new();
        self
    }
}

impl<'ctx, 'a> ParseContext<'a> for DefaultParseContext<'ctx, 'a> {
    fn name(&self) -> &str {
        self.ctx.caption()
    }

    fn state(&self, opt: &Option<'a>) -> OptState {
        if self.parsed.contains(opt.name()) {
            OptState::Skip
        } else if self.seen.contains(opt.name()) {
            OptState::Seen
        } else {
            OptState::Open
        }
    }

    fn do_get_option(&self, name: &str, find_type: FindType) -> Result<SharedOption<'a>, Error> {
        self.ctx.option(name, find_type)
    }

    fn do_set_value(&mut self, opt: &SharedOption<'a>, value: &str) -> Result<bool, Error> {
        let ok = opt.assign(value);
        if ok {
            self.seen.add(opt.name());
        }
        Ok(ok)
    }

    fn do_finish(&mut self, _error: std::option::Option<&Error>) {
        self.parsed.merge(std::mem::take(&mut self.seen));
    }
}

pub struct OptionParser<'ctx, 'a, C: ParseContext<'a> + ?Sized> {
    ctx: &'ctx mut C,
    marker: PhantomData<&'a ()>,
}

impl<'ctx, 'a, C: ParseContext<'a> + ?Sized> OptionParser<'ctx, 'a, C> {
    pub fn new(ctx: &'ctx mut C) -> Self {
        Self {
            ctx,
            marker: PhantomData,
        }
    }

    pub fn ctx(&self) -> &C {
        self.ctx
    }

    pub fn get_option(&self, name: &str, find_type: FindType) -> Result<SharedOption<'a>, Error> {
        self.ctx.get_option(name, find_type)
    }

    pub fn apply_value(&mut self, opt: &SharedOption<'a>, value: &str) -> Result<(), Error> {
        self.ctx.set_value(opt, value)
    }

    pub fn parse<F>(&mut self, parse: F) -> Result<(), Error>
    where
        F: FnOnce(&mut Self) -> Result<(), Error>,
    {
        match parse(self) {
            Ok(()) => {
                self.ctx.finish(None);
                Ok(())
            }
            Err(error) => {
                self.ctx.finish(Some(&error));
                Err(error)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DefaultFormatElement {
    Caption,
    Alias,
    Name,
    Arg,
    Description,
}

pub type StyleCallback = dyn Fn(DefaultFormatElement, bool) -> &'static str;

pub trait OptionFormatter {
    fn format_context<'a, 'b>(
        &self,
        buffer: &'b mut String,
        ctx: &OptionContext<'a>,
    ) -> &'b mut String;
    fn format_group<'a, 'b>(
        &self,
        buffer: &'b mut String,
        group: &OptionGroup<'a>,
    ) -> &'b mut String;
    fn format_option<'a, 'b>(
        &self,
        buffer: &'b mut String,
        option: &Option<'a>,
        max_width: usize,
    ) -> &'b mut String;
    fn column_width<'a>(&self, option: &Option<'a>) -> usize;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultFormat;

impl DefaultFormat {
    pub fn format_context<'a, 'b>(
        buffer: &'b mut String,
        _ctx: &OptionContext<'a>,
    ) -> &'b mut String {
        buffer
    }

    pub fn format_group<'a, 'b>(
        buffer: &'b mut String,
        group: &OptionGroup<'a>,
        style: std::option::Option<&StyleCallback>,
    ) -> &'b mut String {
        if !group.caption().is_empty() {
            buffer.reserve(group.caption().len() + 4);
            buffer.push('\n');
            apply_style(buffer, style, DefaultFormatElement::Caption, true);
            buffer.push_str(group.caption());
            buffer.push(':');
            apply_style(buffer, style, DefaultFormatElement::Caption, false);
            buffer.push('\n');
            buffer.push('\n');
        }
        buffer
    }

    pub fn format_option<'a, 'b>(
        buffer: &'b mut String,
        option: &Option<'a>,
        max_width: usize,
        style: std::option::Option<&StyleCallback>,
    ) -> &'b mut String {
        let width = Self::column_width(option);
        let arg = option.arg_name();
        let neg_name = if arg.is_empty() && option.negatable() {
            "[no-]"
        } else {
            ""
        };
        buffer.reserve(max_width.max(width) + 6 + option.description().len());
        buffer.push_str("  ");
        if option.alias() != '\0' {
            apply_style(buffer, style, DefaultFormatElement::Alias, true);
            buffer.push('-');
            buffer.push(option.alias());
            apply_style(buffer, style, DefaultFormatElement::Alias, false);
            buffer.push(',');
        }
        apply_style(buffer, style, DefaultFormatElement::Name, true);
        buffer.push_str("--");
        buffer.push_str(neg_name);
        buffer.push_str(option.name());
        apply_style(buffer, style, DefaultFormatElement::Name, false);
        if !arg.is_empty() {
            if option.implicit() {
                buffer.push('[');
                buffer.push('=');
            } else {
                buffer.push(if option.alias() != '\0' { ' ' } else { '=' });
            }
            apply_style(buffer, style, DefaultFormatElement::Arg, true);
            buffer.push_str(arg);
            if option.negatable() {
                buffer.push_str("|no");
            }
            apply_style(buffer, style, DefaultFormatElement::Arg, false);
            if option.implicit() {
                buffer.push(']');
            }
        }
        if width < max_width {
            buffer.push_str(&" ".repeat(max_width - width));
        }
        if !option.description().is_empty() {
            buffer.push_str(": ");
            apply_style(buffer, style, DefaultFormatElement::Description, true);
            option.format_description(buffer);
            apply_style(buffer, style, DefaultFormatElement::Description, false);
        }
        buffer.push('\n');
        buffer
    }

    pub fn column_width<'a>(option: &Option<'a>) -> usize {
        let mut width = 2usize;
        if option.alias() != '\0' {
            width += 3;
        }
        width += option.name().len() + 2;
        let arg = option.arg_name();
        if !arg.is_empty() {
            width += arg.len() + 1;
            if option.implicit() {
                width += 2;
            }
            if option.negatable() {
                width += 3;
            }
        } else if option.negatable() {
            width += 5;
        }
        width
    }
}

impl OptionFormatter for DefaultFormat {
    fn format_context<'a, 'b>(
        &self,
        buffer: &'b mut String,
        ctx: &OptionContext<'a>,
    ) -> &'b mut String {
        Self::format_context(buffer, ctx)
    }

    fn format_group<'a, 'b>(
        &self,
        buffer: &'b mut String,
        group: &OptionGroup<'a>,
    ) -> &'b mut String {
        Self::format_group(buffer, group, None)
    }

    fn format_option<'a, 'b>(
        &self,
        buffer: &'b mut String,
        option: &Option<'a>,
        max_width: usize,
    ) -> &'b mut String {
        Self::format_option(buffer, option, max_width, None)
    }

    fn column_width<'a>(&self, option: &Option<'a>) -> usize {
        Self::column_width(option)
    }
}

fn apply_style(
    buffer: &mut String,
    style: std::option::Option<&StyleCallback>,
    element: DefaultFormatElement,
    open: bool,
) {
    if let Some(style) = style {
        buffer.push_str(style(element, open));
    }
}

pub trait OptionOutput<'a> {
    fn print_context(&mut self, ctx: &OptionContext<'a>) -> bool;
    fn print_group(&mut self, group: &OptionGroup<'a>) -> bool;
    fn print_option(&mut self, option: &Option<'a>, max_width: usize) -> bool;
    fn column_width(&mut self, option: &Option<'a>) -> usize;
}

pub trait AppendSink {
    fn append(&mut self, value: &str);
}

impl AppendSink for String {
    fn append(&mut self, value: &str) {
        self.push_str(value);
    }
}

pub enum OutputSink<'a> {
    Appender(&'a mut dyn AppendSink),
    Writer(&'a mut dyn Write),
}

impl<'a> OutputSink<'a> {
    pub fn from_append_sink<S>(sink: &'a mut S) -> Self
    where
        S: AppendSink + 'a,
    {
        Self::Appender(sink)
    }

    pub fn write(&mut self, value: &str) -> &mut Self {
        match self {
            Self::Appender(sink) => {
                sink.append(value);
            }
            Self::Writer(writer) => {
                let _ = writer.write_all(value.as_bytes());
            }
        }
        self
    }
}

impl<'a> From<&'a mut String> for OutputSink<'a> {
    fn from(value: &'a mut String) -> Self {
        Self::from_append_sink(value)
    }
}

pub struct OptionOutputImpl<'a, F = DefaultFormat>
where
    F: OptionFormatter,
{
    sink: OutputSink<'a>,
    buffer: String,
    formatter: F,
}

impl<'a> OptionOutputImpl<'a, DefaultFormat> {
    pub fn new<S>(sink: S) -> Self
    where
        S: Into<OutputSink<'a>>,
    {
        Self::with_formatter(sink.into(), DefaultFormat)
    }
}

impl<'a, F> OptionOutputImpl<'a, F>
where
    F: OptionFormatter,
{
    pub fn with_formatter(sink: OutputSink<'a>, formatter: F) -> Self {
        Self {
            sink,
            buffer: String::new(),
            formatter,
        }
    }

    pub fn with_writer(writer: &'a mut dyn Write, formatter: F) -> Self {
        Self::with_formatter(OutputSink::Writer(writer), formatter)
    }

    fn write_buffer(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        let value = std::mem::take(&mut self.buffer);
        self.sink.write(&value);
    }
}

impl<'a, F> OptionOutput<'a> for OptionOutputImpl<'a, F>
where
    F: OptionFormatter,
{
    fn print_context(&mut self, ctx: &OptionContext<'a>) -> bool {
        self.formatter.format_context(&mut self.buffer, ctx);
        self.write_buffer();
        true
    }

    fn print_group(&mut self, group: &OptionGroup<'a>) -> bool {
        self.formatter.format_group(&mut self.buffer, group);
        self.write_buffer();
        true
    }

    fn print_option(&mut self, option: &Option<'a>, max_width: usize) -> bool {
        self.formatter
            .format_option(&mut self.buffer, option, max_width);
        self.write_buffer();
        true
    }

    fn column_width(&mut self, option: &Option<'a>) -> usize {
        self.formatter.column_width(option)
    }
}

pub type OptionPrinter<'a> = OptionOutputImpl<'a, DefaultFormat>;

pub type PosOption<'a> = dyn FnMut(&str, &mut String) -> bool + 'a;

pub const COMMAND_LINE_ALLOW_FLAG_VALUE: u32 = 1;

pub fn parse_command_array<'a, C>(
    ctx: &mut C,
    args: &[&str],
    pos: std::option::Option<&mut PosOption<'_>>,
    flags: u32,
) -> Result<(), Error>
where
    C: ParseContext<'a> + ?Sized,
{
    let mut index = 0usize;
    parse_command_tokens(
        ctx,
        || {
            if index < args.len() {
                let current = args[index].to_owned();
                index += 1;
                Some(current)
            } else {
                None
            }
        },
        pos,
        flags,
    )
}

pub fn parse_command_string<'a, C>(
    ctx: &mut C,
    args: &str,
    pos: std::option::Option<&mut PosOption<'_>>,
    flags: u32,
) -> Result<(), Error>
where
    C: ParseContext<'a> + ?Sized,
{
    let mut tokens = CommandStringTokens::new(args);
    parse_command_tokens(ctx, || tokens.next(), pos, flags)
}

pub fn parse_cfg_file<'a, C, R>(ctx: &mut C, input: &mut R) -> Result<(), Error>
where
    C: ParseContext<'a> + ?Sized,
    R: BufRead,
{
    let mut parser = OptionParser::new(ctx);
    parser.parse(|parser| {
        let mut section_name = String::new();
        let mut section_value = String::new();
        let mut in_section = false;
        for line in input.lines() {
            let mut line = line.map_err(|error| Error::message(error.to_string()))?;
            trim_left(&mut line, " \t");
            trim_right(&mut line, " \t");
            if line.is_empty() || line.starts_with('#') {
                if in_section {
                    let option = parser.get_option(&section_name, FindType::NAME_OR_PREFIX)?;
                    parser.apply_value(&option, &section_value)?;
                    in_section = false;
                }
                continue;
            }
            if let Some(position) = line.find('=') {
                if in_section {
                    let option = parser.get_option(&section_name, FindType::NAME_OR_PREFIX)?;
                    parser.apply_value(&option, &section_value)?;
                }
                section_name.clear();
                section_name.push_str(&line[..position]);
                section_value.clear();
                section_value.push_str(&line[position + 1..]);
                trim_right(&mut section_name, " \t");
                trim_left(&mut section_value, " \t\n");
                in_section = true;
            } else if in_section {
                section_value.push(' ');
                section_value.push_str(&line);
            } else {
                return Err(SyntaxError::new(SyntaxErrorType::InvalidFormat, line).into());
            }
        }
        if in_section {
            let option = parser.get_option(&section_name, FindType::NAME_OR_PREFIX)?;
            parser.apply_value(&option, &section_value)?;
        }
        Ok(())
    })
}

fn parse_command_tokens<'a, C, N>(
    ctx: &mut C,
    mut next: N,
    mut pos: std::option::Option<&mut PosOption<'_>>,
    flags: u32,
) -> Result<(), Error>
where
    C: ParseContext<'a> + ?Sized,
    N: FnMut() -> std::option::Option<String>,
{
    let mut parser = OptionParser::new(ctx);
    parser.parse(|parser| {
        while let Some(current) = next() {
            if current == "--" {
                break;
            }
            if let Some(option) = current.strip_prefix("--") {
                handle_long_opt(parser, &mut next, option, flags)?;
            } else if current.starts_with('-') && current.len() > 1 {
                handle_short_opt(parser, &mut next, &current[1..])?;
            } else {
                handle_positional_opt(parser, pos.as_deref_mut(), &current)?;
            }
        }
        Ok(())
    })
}

fn handle_short_opt<'a, C, N>(
    parser: &mut OptionParser<'_, 'a, C>,
    next: &mut N,
    mut option_name: &str,
) -> Result<(), Error>
where
    C: ParseContext<'a> + ?Sized,
    N: FnMut() -> std::option::Option<String>,
{
    while !option_name.is_empty() {
        let current = option_name
            .chars()
            .next()
            .expect("checked non-empty option segment");
        let current_len = current.len_utf8();
        let key = format!("-{current}");
        let tail = &option_name[current_len..];
        let option = parser.get_option(&key, FindType::ALIAS)?;
        if option.implicit() {
            if !option.flag() {
                parser.apply_value(&option, tail)?;
                return Ok(());
            }
            parser.apply_value(&option, "")?;
            option_name = tail;
        } else {
            let value = if tail.is_empty() {
                next().unwrap_or_default()
            } else {
                tail.to_owned()
            };
            if value.is_empty() {
                return Err(SyntaxError::new(SyntaxErrorType::MissingValue, key).into());
            }
            parser.apply_value(&option, &value)?;
            return Ok(());
        }
    }
    Ok(())
}

fn handle_long_opt<'a, C, N>(
    parser: &mut OptionParser<'_, 'a, C>,
    next: &mut N,
    option_name: &str,
    flags: u32,
) -> Result<(), Error>
where
    C: ParseContext<'a> + ?Sized,
    N: FnMut() -> std::option::Option<String>,
{
    let (name, mut value) = match option_name.split_once('=') {
        Some((name, value)) => (name, std::option::Option::Some(value.to_owned())),
        None => (option_name, None),
    };
    let mut fallback = None;
    let mut allow_flag_value = (flags & COMMAND_LINE_ALLOW_FLAG_VALUE) != 0;
    if value.is_none() && option_name.starts_with("no-") {
        if let Ok(option) = parser.get_option(&option_name[3..], FindType::NAME_OR_PREFIX) {
            if option.negatable() {
                fallback = Some(option);
            }
        }
    }
    let option = match parser.get_option(name, FindType::NAME_OR_PREFIX) {
        Ok(option) => option,
        Err(Error::Context(error))
            if error.kind() == ContextErrorType::UnknownOption && fallback.is_some() =>
        {
            allow_flag_value = true;
            value = Some("no".to_owned());
            fallback.expect("checked fallback above")
        }
        Err(error) => return Err(error),
    };
    if value.is_none() && !option.implicit() {
        let next_value = next().unwrap_or_default();
        if next_value.is_empty() {
            return Err(SyntaxError::new(SyntaxErrorType::MissingValue, name).into());
        }
        value = Some(next_value);
    }
    if value.is_some() && !allow_flag_value && option.flag() {
        return Err(SyntaxError::new(SyntaxErrorType::ExtraValue, name).into());
    }
    parser.apply_value(&option, value.as_deref().unwrap_or(""))
}

fn handle_positional_opt<'a, C>(
    parser: &mut OptionParser<'_, 'a, C>,
    mut pos: std::option::Option<&mut PosOption<'_>>,
    token: &str,
) -> Result<(), Error>
where
    C: ParseContext<'a> + ?Sized,
{
    let mut name = String::new();
    if let Some(pos) = pos.as_mut() {
        if !pos(token, &mut name) {
            name.push_str("Positional Option");
        }
    } else {
        name.push_str("Positional Option");
    }
    let option = parser.get_option(&name, FindType::NAME_OR_PREFIX)?;
    parser.apply_value(&option, token)
}

struct CommandStringTokens<'a> {
    input: &'a str,
    token: String,
}

impl<'a> CommandStringTokens<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            token: String::with_capacity(80),
        }
    }

    fn next(&mut self) -> std::option::Option<String> {
        while let Some(current) = self.input.chars().next() {
            if current.is_whitespace() {
                self.input = &self.input[current.len_utf8()..];
            } else {
                break;
            }
        }
        if self.input.is_empty() {
            return None;
        }
        self.token.clear();
        let mut terminator = ' ';
        while let Some(current) = self.input.chars().next() {
            let current_len = current.len_utf8();
            if current == terminator {
                self.input = &self.input[current_len..];
                if terminator == ' ' {
                    break;
                }
                terminator = ' ';
                continue;
            }
            if (current == '\'' || current == '"') && terminator == ' ' {
                terminator = current;
                self.input = &self.input[current_len..];
                continue;
            }
            if current == '\\' {
                let mut chars = self.input.chars();
                let _ = chars.next();
                if let Some(next) = chars.next() {
                    if matches!(next, '\\' | '\'' | '"') {
                        self.token.push(next);
                        self.input = &self.input[current_len + next.len_utf8()..];
                        continue;
                    }
                }
            }
            self.token.push(current);
            self.input = &self.input[current_len..];
        }
        Some(self.token.clone())
    }
}

fn trim_left(input: &mut String, chars: &str) {
    match input.find(|ch| !chars.contains(ch)) {
        Some(0) => {}
        Some(index) => {
            input.drain(..index);
        }
        None => input.clear(),
    }
}

fn trim_right(input: &mut String, chars: &str) {
    match input.rfind(|ch| !chars.contains(ch)) {
        Some(index) => input.truncate(index + 1),
        None => input.clear(),
    }
}
