//! Port target for original_clasp/libpotassco/potassco/program_opts/value.h.

use super::intrusive_ptr::{IntrusiveRefCounted, IntrusiveSharedPtr};
use super::program_options::{DescriptionLevel, Option};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Str {
    Literal(&'static str),
    Dynamic(String),
}

impl Default for Str {
    fn default() -> Self {
        Self::Literal("")
    }
}

impl Str {
    pub const fn literal(value: &'static str) -> Self {
        Self::Literal(value)
    }

    pub fn dynamic(value: impl Into<String>) -> Self {
        Self::Dynamic(value.into())
    }

    pub fn str(&self) -> &str {
        match self {
            Self::Literal(value) => value,
            Self::Dynamic(value) => value.as_str(),
        }
    }

    pub fn is_lit(&self) -> bool {
        matches!(self, Self::Literal(_))
    }

    pub fn empty(&self) -> bool {
        self.str().is_empty()
    }

    pub fn size(&self) -> usize {
        self.str().len()
    }

    pub fn remove_prefix(&mut self, prefix_len: usize) {
        match self {
            Self::Literal(value) => {
                *value = value
                    .get(prefix_len..)
                    .expect("prefix must be within bounds and on a UTF-8 boundary");
            }
            Self::Dynamic(value) => {
                let suffix = value
                    .get(prefix_len..)
                    .expect("prefix must be within bounds and on a UTF-8 boundary")
                    .to_owned();
                *value = suffix;
            }
        }
    }
}

impl From<&str> for Str {
    fn from(value: &str) -> Self {
        Self::Dynamic(value.to_owned())
    }
}

impl From<&String> for Str {
    fn from(value: &String) -> Self {
        Self::Dynamic(value.clone())
    }
}

impl From<String> for Str {
    fn from(value: String) -> Self {
        Self::Dynamic(value)
    }
}

pub trait ValueAction<'a> {
    fn assign(&mut self, opt: &Option<'a>, value: &str) -> bool;

    fn release(&mut self) -> bool {
        true
    }
}

impl<'a, T> ValueAction<'a> for Box<T>
where
    T: ValueAction<'a> + ?Sized,
{
    fn assign(&mut self, opt: &Option<'a>, value: &str) -> bool {
        (**self).assign(opt, value)
    }

    fn release(&mut self) -> bool {
        (**self).release()
    }
}

pub trait SharedValueAction<'a> {
    fn assign_shared(&self, opt: &Option<'a>, value: &str) -> bool;
}

pub type ValueActionPtr<'a> = Box<dyn ValueAction<'a> + 'a>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ValueActionRelease;

pub fn make_action<'a, T>(action: T) -> Box<T>
where
    T: ValueAction<'a> + 'a,
{
    Box::new(action)
}

struct SharedActionAdapter<'a, T>
where
    T: SharedValueAction<'a> + IntrusiveRefCounted + 'a,
{
    action: IntrusiveSharedPtr<T>,
    marker: std::marker::PhantomData<&'a ()>,
}

impl<'a, T> ValueAction<'a> for SharedActionAdapter<'a, T>
where
    T: SharedValueAction<'a> + IntrusiveRefCounted + 'a,
{
    fn assign(&mut self, opt: &Option<'a>, value: &str) -> bool {
        self.action
            .get()
            .expect("shared action must be alive")
            .assign_shared(opt, value)
    }
}

pub struct ValueDesc<'a> {
    pub(crate) action: std::option::Option<ValueActionPtr<'a>>,
    pub(crate) arg_name: Str,
    pub(crate) default_value: Str,
    pub(crate) implicit_value: Str,
    pub(crate) id: u32,
    pub(crate) level: DescriptionLevel,
    pub(crate) implicit: bool,
    pub(crate) flag: bool,
    pub(crate) composing: bool,
    pub(crate) negatable: bool,
    pub(crate) defaulted: bool,
}

impl<'a> Default for ValueDesc<'a> {
    fn default() -> Self {
        Self {
            action: None,
            arg_name: Str::default(),
            default_value: Str::default(),
            implicit_value: Str::default(),
            id: 0,
            level: DescriptionLevel::Default,
            implicit: false,
            flag: false,
            composing: false,
            negatable: false,
            defaulted: false,
        }
    }
}

impl<'a> ValueDesc<'a> {
    pub fn new(action: ValueActionPtr<'a>, id: u32) -> Self {
        Self {
            action: Some(action),
            id,
            ..Self::default()
        }
    }

    pub fn from_shared<T>(action: IntrusiveSharedPtr<T>, id: u32) -> Self
    where
        T: SharedValueAction<'a> + IntrusiveRefCounted + 'a,
    {
        Self {
            action: Some(Box::new(SharedActionAdapter {
                action,
                marker: std::marker::PhantomData,
            })),
            id,
            ..Self::default()
        }
    }

    pub fn arg(mut self, name: impl Into<Str>) -> Self {
        self.arg_name = name.into();
        self
    }

    pub fn implicit(mut self, value: impl Into<Str>) -> Self {
        self.implicit_value = value.into();
        self.implicit = true;
        self
    }

    pub fn flag(mut self) -> Self {
        self.flag = true;
        self.implicit = true;
        self
    }

    pub fn negatable(mut self) -> Self {
        self.negatable = true;
        self
    }

    pub fn composing(mut self) -> Self {
        self.composing = true;
        self
    }

    pub fn defaults_to(mut self, value: impl Into<Str>) -> Self {
        self.default_value = value.into();
        self
    }

    pub fn defaults_to_assigned(mut self, value: impl Into<Str>) -> Self {
        self.default_value = value.into();
        self.defaulted = true;
        self
    }

    pub fn level(mut self, value: DescriptionLevel) -> Self {
        self.level = value;
        self
    }

    pub fn arg_name(&self) -> &Str {
        &self.arg_name
    }

    pub fn implicit_value(&self) -> &Str {
        &self.implicit_value
    }

    pub fn default_value(&self) -> &Str {
        &self.default_value
    }

    pub fn desc_level(&self) -> DescriptionLevel {
        self.level
    }

    pub fn action_ref(&self) -> std::option::Option<&(dyn ValueAction<'a> + 'a)> {
        self.action.as_deref()
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn is_negatable(&self) -> bool {
        self.negatable
    }

    pub fn is_flag(&self) -> bool {
        self.flag
    }

    pub fn is_composing(&self) -> bool {
        self.composing
    }

    pub fn is_implicit(&self) -> bool {
        self.implicit
    }

    pub fn is_defaulted(&self) -> bool {
        self.defaulted
    }
}

pub trait IntoValueDesc<'a> {
    fn into_value_desc(self, id: u32) -> ValueDesc<'a>;
}

impl<'a, T> IntoValueDesc<'a> for T
where
    T: ValueAction<'a> + 'a,
{
    fn into_value_desc(self, id: u32) -> ValueDesc<'a> {
        ValueDesc::new(Box::new(self), id)
    }
}

impl<'a, T> IntoValueDesc<'a> for IntrusiveSharedPtr<T>
where
    T: SharedValueAction<'a> + IntrusiveRefCounted + 'a,
{
    fn into_value_desc(self, id: u32) -> ValueDesc<'a> {
        ValueDesc::from_shared(self, id)
    }
}

pub fn value<'a, T>(action: T) -> ValueDesc<'a>
where
    T: IntoValueDesc<'a>,
{
    action.into_value_desc(0)
}

pub fn value_with_id<'a, T>(action: T, id: u32) -> ValueDesc<'a>
where
    T: IntoValueDesc<'a>,
{
    action.into_value_desc(id)
}
