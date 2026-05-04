//! Port target for original_clasp/libpotassco/potassco/program_opts/typed_value.h.

use std::cell::{Cell, RefCell};
use std::marker::PhantomData;

use super::intrusive_ptr::{IntrusiveRefCounted, IntrusiveSharedPtr, make_shared};
use super::program_options::Option;
use super::string_convert::{StringTo, parse, string_to};
use super::value::{SharedValueAction, ValueAction, ValueDesc, value};

pub trait Parser<T> {
    fn parse(&mut self, input: &str, out: &mut T) -> bool;
}

impl<T, F> Parser<T> for F
where
    F: FnMut(&str, &mut T) -> bool,
{
    fn parse(&mut self, input: &str, out: &mut T) -> bool {
        self(input, out)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultParser;

impl<T> Parser<T> for DefaultParser
where
    T: StringTo,
{
    fn parse(&mut self, input: &str, out: &mut T) -> bool {
        parse::ok(string_to(input, out))
    }
}

#[derive(Clone, Debug)]
pub struct ParseValues<V> {
    values: Vec<(String, V)>,
}

impl<V> ParseValues<V> {
    pub fn new(values: Vec<(String, V)>) -> Self {
        Self { values }
    }
}

impl<T, V> Parser<T> for ParseValues<V>
where
    V: Clone + Into<T>,
{
    fn parse(&mut self, input: &str, out: &mut T) -> bool {
        for (key, value) in &self.values {
            if parse::eq_ignore_case(key, input) {
                *out = value.clone().into();
                return true;
            }
        }
        false
    }
}

pub struct Store<'a, T, P = DefaultParser> {
    address: *mut T,
    parser: P,
    marker: PhantomData<&'a mut T>,
}

impl<'a, T, P> Store<'a, T, P> {
    pub fn new(value: &'a mut T, parser: P) -> Self {
        Self {
            address: value,
            parser,
            marker: PhantomData,
        }
    }
}

impl<'a, T, P> ValueAction<'a> for Store<'a, T, P>
where
    P: Parser<T> + 'a,
    T: 'a,
{
    fn assign(&mut self, _opt: &Option<'a>, value: &str) -> bool {
        // SAFETY: the lifetime parameter ties the raw pointer to the stored
        // borrow and the pointed-to value outlives the action.
        unsafe { self.parser.parse(value, &mut *self.address) }
    }
}

pub struct TypedAction<'a, T, C, P = DefaultParser> {
    action: C,
    parser: P,
    marker: PhantomData<&'a T>,
}

impl<'a, T, C, P> TypedAction<'a, T, C, P> {
    pub fn new(action: C, parser: P) -> Self {
        Self {
            action,
            parser,
            marker: PhantomData,
        }
    }
}

impl<'a, T, C, P> ValueAction<'a> for TypedAction<'a, T, C, P>
where
    C: FnMut(T) + 'a,
    P: Parser<T> + 'a,
    T: Default + 'a,
{
    fn assign(&mut self, _opt: &Option<'a>, value: &str) -> bool {
        let mut out = T::default();
        if !self.parser.parse(value, &mut out) {
            return false;
        }
        (self.action)(out);
        true
    }
}

pub struct TypedActionWithOption<'a, T, C, P = DefaultParser> {
    action: C,
    parser: P,
    marker: PhantomData<&'a T>,
}

impl<'a, T, C, P> TypedActionWithOption<'a, T, C, P> {
    pub fn new(action: C, parser: P) -> Self {
        Self {
            action,
            parser,
            marker: PhantomData,
        }
    }
}

impl<'a, T, C, P> ValueAction<'a> for TypedActionWithOption<'a, T, C, P>
where
    C: FnMut(&Option<'a>, T) + 'a,
    P: Parser<T> + 'a,
    T: Default + 'a,
{
    fn assign(&mut self, opt: &Option<'a>, value: &str) -> bool {
        let mut out = T::default();
        if !self.parser.parse(value, &mut out) {
            return false;
        }
        (self.action)(opt, out);
        true
    }
}

pub struct CustomBase<const SHARED: bool> {
    rc: Cell<i32>,
}

impl<const SHARED: bool> Default for CustomBase<SHARED> {
    fn default() -> Self {
        Self { rc: Cell::new(1) }
    }
}

pub struct Custom<'a, C, const SHARED: bool = false> {
    handler: RefCell<C>,
    base: CustomBase<SHARED>,
    marker: PhantomData<&'a ()>,
}

impl<'a, C, const SHARED: bool> Custom<'a, C, SHARED> {
    pub fn new(handler: C) -> Self {
        Self {
            handler: RefCell::new(handler),
            base: CustomBase::default(),
            marker: PhantomData,
        }
    }
}

impl<'a, C, const SHARED: bool> ValueAction<'a> for Custom<'a, C, SHARED>
where
    C: FnMut(&Option<'a>, &str) -> bool + 'a,
{
    fn assign(&mut self, opt: &Option<'a>, value: &str) -> bool {
        (self.handler.get_mut())(opt, value)
    }
}

impl<'a, C> SharedValueAction<'a> for Custom<'a, C, true>
where
    C: FnMut(&Option<'a>, &str) -> bool + 'a,
{
    fn assign_shared(&self, opt: &Option<'a>, value: &str) -> bool {
        (self.handler.borrow_mut())(opt, value)
    }
}

impl<'a, C> IntrusiveRefCounted for Custom<'a, C, true>
where
    C: FnMut(&Option<'a>, &str) -> bool + 'a,
{
    fn intrusive_add_ref(&self) {
        self.base.rc.set(self.base.rc.get() + 1);
    }

    fn intrusive_release(&self) -> i32 {
        let next = self.base.rc.get() - 1;
        self.base.rc.set(next);
        next
    }

    fn intrusive_count(&self) -> i32 {
        self.base.rc.get()
    }
}

pub trait FlagTarget<'a> {
    fn into_flag_with<P>(self, parser: P) -> ValueDesc<'a>
    where
        P: Parser<bool> + 'a;
}

impl<'a> FlagTarget<'a> for &'a mut bool {
    fn into_flag_with<P>(self, parser: P) -> ValueDesc<'a>
    where
        P: Parser<bool> + 'a,
    {
        store_to_with(self, parser).flag()
    }
}

impl<'a, C> FlagTarget<'a> for C
where
    C: FnMut(bool) + 'a,
{
    fn into_flag_with<P>(self, parser: P) -> ValueDesc<'a>
    where
        P: Parser<bool> + 'a,
    {
        action(self, parser).flag()
    }
}

pub fn values<T, I, K>(entries: I) -> ParseValues<T>
where
    I: IntoIterator<Item = (K, T)>,
    K: Into<String>,
{
    ParseValues::new(
        entries
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}

pub fn store_to<'a, T>(target: &'a mut T) -> ValueDesc<'a>
where
    DefaultParser: Parser<T>,
    T: 'a,
{
    value(Store::new(target, DefaultParser))
}

pub fn store_to_with<'a, T, P>(target: &'a mut T, parser: P) -> ValueDesc<'a>
where
    P: Parser<T> + 'a,
    T: 'a,
{
    value(Store::new(target, parser))
}

pub fn store_to_init<'a, T>(value: &'a mut T, init: T) -> ValueDesc<'a>
where
    DefaultParser: Parser<T>,
    T: 'a,
{
    *value = init;
    store_to(value)
}

pub fn action<'a, T, C, P>(callable: C, parser: P) -> ValueDesc<'a>
where
    C: FnMut(T) + 'a,
    P: Parser<T> + 'a,
    T: Default + 'a,
{
    value(TypedAction::new(callable, parser))
}

pub fn action_default<'a, T, C>(callable: C) -> ValueDesc<'a>
where
    C: FnMut(T) + 'a,
    DefaultParser: Parser<T>,
    T: Default + 'a,
{
    action(callable, DefaultParser)
}

pub fn action_with_option<'a, T, C, P>(callable: C, parser: P) -> ValueDesc<'a>
where
    C: FnMut(&Option<'a>, T) + 'a,
    P: Parser<T> + 'a,
    T: Default + 'a,
{
    value(TypedActionWithOption::new(callable, parser))
}

pub fn action_with_option_default<'a, T, C>(callable: C) -> ValueDesc<'a>
where
    C: FnMut(&Option<'a>, T) + 'a,
    DefaultParser: Parser<T>,
    T: Default + 'a,
{
    action_with_option(callable, DefaultParser)
}

pub fn store_false(input: &str, out: &mut bool) -> bool {
    let mut temp = false;
    if DefaultParser.parse(input, &mut temp) {
        *out = !temp;
        true
    } else {
        false
    }
}

pub fn flag<'a, F>(target: F) -> ValueDesc<'a>
where
    F: FlagTarget<'a>,
{
    target.into_flag_with(DefaultParser)
}

pub fn flag_with<'a, F, P>(target: F, parser: P) -> ValueDesc<'a>
where
    F: FlagTarget<'a>,
    P: Parser<bool> + 'a,
{
    target.into_flag_with(parser)
}

pub fn flag_with_init<'a, P>(target: &'a mut bool, init: bool, parser: P) -> ValueDesc<'a>
where
    P: Parser<bool> + 'a,
{
    *target = init;
    store_to_with(target, parser).flag()
}

pub fn parse<'a, C>(mut parser: C) -> ValueDesc<'a>
where
    C: FnMut(&str) -> bool + 'a,
{
    value(Custom::<_, false>::new(
        move |_opt: &Option<'a>, value: &str| parser(value),
    ))
}

pub fn parse_with_option<'a, C>(parser: C) -> ValueDesc<'a>
where
    C: FnMut(&Option<'a>, &str) -> bool + 'a,
{
    value(Custom::<_, false>::new(parser))
}

pub fn make_custom<'a, C>(parser: C) -> IntrusiveSharedPtr<Custom<'a, C, true>>
where
    C: FnMut(&Option<'a>, &str) -> bool + 'a,
{
    make_shared(Custom::<_, true>::new(parser))
}
