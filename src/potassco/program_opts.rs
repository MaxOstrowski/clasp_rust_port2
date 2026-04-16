//! Rust module root for the upstream Potassco program-options subsystem.

#[path = "program_opts/errors.rs"]
pub mod errors;
#[path = "program_opts/intrusive_ptr.rs"]
pub mod intrusive_ptr;
#[path = "program_opts/program_options.rs"]
pub mod program_options;
#[path = "program_opts/string_convert.rs"]
pub mod string_convert;
#[path = "program_opts/typed_value.rs"]
pub mod typed_value;
#[path = "program_opts/value.rs"]
pub mod value;

pub use errors::{
    ContextError, ContextErrorType, Error, SyntaxError, SyntaxErrorType, ValueError, ValueErrorType,
};
pub use intrusive_ptr::{IntrusiveRefCounted, IntrusiveSharedPtr, make_shared};
pub use program_options::{
    COMMAND_LINE_ALLOW_FLAG_VALUE, DefaultFormat, DefaultFormatElement, DefaultParseContext,
    DescriptionLevel, FindType, OptState, Option, OptionContext, OptionFormatter, OptionGroup,
    OptionGroupInit, OptionOutput, OptionOutputImpl, OptionParser, OptionPrinter, OutputSink,
    ParseContext, ParsedOptions, PosOption, SharedOption, parse_cfg_file, parse_command_array,
    parse_command_string,
};
pub use string_convert::{
    EnumEntries, Errc, FromCharsResult, ParseChars, StringTo, extract, from_chars,
    from_chars_str_ref, parse as string_parse, string_to, string_to_errc,
};
pub use typed_value::{
    Custom, CustomBase, DefaultParser, FlagTarget, ParseValues, Parser, Store, TypedAction,
    TypedActionWithOption, action, action_default, action_with_option, action_with_option_default,
    flag, flag_with, flag_with_init, make_custom, parse, parse_with_option, store_false, store_to,
    store_to_init, store_to_with, values,
};
pub use value::{
    IntoValueDesc, Str, ValueAction, ValueActionPtr, ValueActionRelease, ValueDesc, make_action,
    value, value_with_id,
};
