//! Rust port of original_clasp/libpotassco/app/lpconvert.cpp.

use std::any::Any;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::potassco::application::{Application, ApplicationBase};
use crate::potassco::aspif::{AspifOutput, read_aspif};
use crate::potassco::aspif_text::AspifTextOutput;
use crate::potassco::convert::SmodelsConvert;
use crate::potassco::enums::{EnumEntries, EnumMetadata, EnumTag, HasEnumEntries, make_entries};
use crate::potassco::error::Error as PotasscoError;
use crate::potassco::program_opts::{
    DescriptionLevel, Error as ProgramOptionsError, OptionContext, ParsedOptions, action_default,
    flag,
};
use crate::potassco::reify::{Reifier, ReifierOptions};
use crate::potassco::smodels::{SmodelsOptions, SmodelsOutput, read_smodels};

const LIB_POTASSCO_VERSION: &str = "0.1.0";

#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum Format {
    #[default]
    Auto = 0,
    Text = 1,
    Smodels = 2,
    AspifV1 = 3,
    Aspif = 4,
    Reify = 5,
}

impl EnumTag for Format {
    type Repr = u8;

    fn to_underlying(self) -> Self::Repr {
        self as u8
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Auto),
            1 => Some(Self::Text),
            2 => Some(Self::Smodels),
            3 => Some(Self::AspifV1),
            4 => Some(Self::Aspif),
            5 => Some(Self::Reify),
            _ => None,
        }
    }

    fn metadata() -> Option<EnumMetadata<Self>> {
        Some(EnumMetadata::Entries(Self::entries_metadata()))
    }
}

impl HasEnumEntries for Format {
    fn entries_metadata() -> EnumEntries<Self> {
        static ENTRIES: &[(Format, &str)] = &[
            (Format::Auto, "auto"),
            (Format::Text, "text"),
            (Format::Smodels, "smodels"),
            (Format::AspifV1, "aspif-v1"),
            (Format::Aspif, "aspif"),
            (Format::Reify, "reify"),
        ];
        make_entries(ENTRIES)
    }
}

pub struct LpConvertApp<'a> {
    base: ApplicationBase,
    input_stream: &'a [u8],
    stdout: &'a mut dyn Write,
    stderr: &'a mut dyn Write,
    input: String,
    output: String,
    pred: String,
    format: Format,
    text: bool,
    reify: bool,
    reify_opts: ReifierOptions,
    potassco: bool,
    filter: bool,
}

impl<'a> LpConvertApp<'a> {
    #[must_use]
    pub fn new(
        input_stream: &'a [u8],
        stdout: &'a mut dyn Write,
        stderr: &'a mut dyn Write,
    ) -> Self {
        Self {
            base: ApplicationBase::new(),
            input_stream,
            stdout,
            stderr,
            input: String::new(),
            output: String::new(),
            pred: String::new(),
            format: Format::Auto,
            text: false,
            reify: false,
            reify_opts: ReifierOptions::default(),
            potassco: false,
            filter: false,
        }
    }

    fn execute(&mut self) -> Result<(), String> {
        let mut input_file = None;
        let mut output_file = None;
        if !self.input.is_empty() && self.input != "-" {
            input_file =
                Some(File::open(&self.input).map_err(|_| "Could not open input file".to_owned())?);
        }
        if !self.output.is_empty() && self.output != "-" {
            if self.input == self.output {
                return Err("Input and output must be different".to_owned());
            }
            output_file = Some(
                File::create(&self.output).map_err(|_| "Could not open output file".to_owned())?,
            );
        }

        let mut reader: Box<dyn BufRead> = if let Some(file) = input_file {
            Box::new(BufReader::new(file))
        } else {
            Box::new(BufReader::new(self.input_stream))
        };
        let mut first = 0u8;
        if let Some(byte) = reader
            .fill_buf()
            .map_err(|e| e.to_string())?
            .first()
            .copied()
        {
            first = byte;
        }
        if first != b'a' && !first.is_ascii_digit() {
            return Err(format!(
                "Unrecognized input format '{}' - expected 'aspif' or <digit>",
                first as char
            ));
        }

        let output_target: &mut dyn Write = if let Some(file) = output_file.as_mut() {
            file
        } else {
            self.stdout
        };

        let mut format = if self.text {
            Format::Text
        } else if self.reify {
            Format::Reify
        } else {
            self.format
        };
        if format == Format::Auto && first == b'a' {
            format = Format::Smodels;
        }
        self.format = format;
        let mut smodels_opts = SmodelsOptions::default();
        if self.potassco {
            smodels_opts = smodels_opts
                .enable_clasp_ext()
                .convert_edges()
                .convert_heuristic();
            if self.filter {
                smodels_opts = smodels_opts.drop_converted();
            }
        }

        match format {
            Format::Text => {
                let mut text = AspifTextOutput::new(output_target);
                if !self.pred.is_empty() {
                    match catch_unwind(AssertUnwindSafe(|| text.set_atom_pred(&self.pred))) {
                        Ok(()) => {}
                        Err(payload) => {
                            let message = payload_to_string(payload);
                            if message.contains("atom predicate") {
                                return Err(format!(
                                    "invalid aux predicate: '{}'\natom prefix (e.g. 'x_') or unary predicate (e.g. '_id/1') expected",
                                    self.pred
                                ));
                            }
                            return Err(message);
                        }
                    }
                }
                if first == b'a' {
                    read_aspif(&mut reader, &mut text);
                } else {
                    read_smodels(&mut reader, &mut text, smodels_opts);
                }
            }
            Format::Smodels => {
                let mut smodels = SmodelsOutput::new(output_target, self.potassco, 0);
                let mut convert = SmodelsConvert::new(&mut smodels, self.potassco);
                if first == b'a' {
                    read_aspif(&mut reader, &mut convert);
                } else {
                    read_smodels(&mut reader, &mut convert, smodels_opts);
                }
            }
            Format::AspifV1 => {
                let mut aspif = AspifOutput::new(output_target, 1);
                if first == b'a' {
                    read_aspif(&mut reader, &mut aspif);
                } else {
                    read_smodels(&mut reader, &mut aspif, smodels_opts);
                }
            }
            Format::Auto | Format::Aspif => {
                let mut aspif = AspifOutput::new(output_target, 2);
                if first == b'a' {
                    read_aspif(&mut reader, &mut aspif);
                } else {
                    read_smodels(&mut reader, &mut aspif, smodels_opts);
                }
            }
            Format::Reify => {
                let mut reifier = Reifier::new(output_target, self.reify_opts);
                if first == b'a' {
                    read_aspif(&mut reader, &mut reifier);
                } else {
                    read_smodels(&mut reader, &mut reifier, smodels_opts);
                }
            }
        }
        Ok(())
    }

    fn handle_conversion_error(&self, text: &str) {
        let mut error = text;
        let mut info = "";
        if let Some((head, tail)) = text.split_once('\n') {
            error = head;
            info = tail;
        }
        if self.format == Format::Smodels && error.contains("not supported") {
            if let Some(pos) = error.rfind(':') {
                error = &error[..pos];
            }
            info = "Try different format or enable potassco extensions";
        }
        self.fail(1, error, info);
    }
}

impl Application for LpConvertApp<'_> {
    fn base(&self) -> &ApplicationBase {
        &self.base
    }

    fn get_name(&self) -> &str {
        "lpconvert"
    }

    fn get_version(&self) -> &str {
        "2.0.0"
    }

    fn get_positional(&self, _value: &str) -> &str {
        "input"
    }

    fn get_usage(&self) -> &str {
        "[options] [<file>]\nConvert program in <file> or standard input"
    }

    fn init_options<'b>(&mut self, root: &mut OptionContext<'b>) {
        let app = self as *mut Self as usize;
        let mut convert = crate::potassco::program_opts::OptionGroup::new(
            "Conversion Options",
            DescriptionLevel::Default,
        );
        convert
            .add_options()
            .add(
                "-i@2,input",
                action_default(move |value: String| unsafe {
                    let app = app as *mut Self;
                    (*app).input = value;
                }),
                "Input file",
            )
            .unwrap()
            .add(
                "-p,potassco",
                flag(move |enabled| unsafe {
                    let app = app as *mut Self;
                    (*app).potassco = enabled;
                }),
                "Enable potassco extensions",
            )
            .unwrap()
            .add(
                "-f,filter",
                flag(move |enabled| unsafe {
                    let app = app as *mut Self;
                    (*app).filter = enabled;
                }),
                "Hide converted potassco predicates",
            )
            .unwrap()
            .add(
                "-o,output",
                action_default(move |value: String| unsafe {
                    let app = app as *mut Self;
                    (*app).output = value;
                })
                .arg("<file>"),
                "Write output to <file> (default: stdout)",
            )
            .unwrap()
            .add(
                "format",
                action_default(move |value: Format| unsafe {
                    let app = app as *mut Self;
                    (*app).format = value;
                }),
                "Output format (text|smodels|aspif|aspif-v1|reify)",
            )
            .unwrap()
            .add(
                "-t,text",
                flag(move |enabled| unsafe {
                    let app = app as *mut Self;
                    (*app).text = enabled;
                }),
                "Convert to ground text format",
            )
            .unwrap()
            .add(
                "-r,reify",
                flag(move |enabled| unsafe {
                    let app = app as *mut Self;
                    (*app).reify = enabled;
                }),
                "Convert program to reified facts",
            )
            .unwrap()
            .add(
                "aux-pred",
                action_default(move |value: String| unsafe {
                    let app = app as *mut Self;
                    (*app).pred = value;
                }),
                "Prefix/Predicate for atom numbers in text output",
            )
            .unwrap()
            .add(
                "reify-sccs",
                flag(move |enabled| unsafe {
                    let app = app as *mut Self;
                    (*app).reify_opts.calculate_sccs = enabled;
                }),
                "Calculate SCCs for reified output",
            )
            .unwrap()
            .add(
                "reify-steps",
                flag(move |enabled| unsafe {
                    let app = app as *mut Self;
                    (*app).reify_opts.reify_step = enabled;
                }),
                "Add step numbers to reified output",
            )
            .unwrap();
        root.add(convert).unwrap();
    }

    fn validate_options<'b>(
        &mut self,
        _root: &OptionContext<'b>,
        parsed: &ParsedOptions,
    ) -> Result<(), ProgramOptionsError> {
        if parsed.contains("text") && parsed.contains("format") {
            return Err(ProgramOptionsError::message(
                "options 'text' and 'format' are mutually exclusive".to_owned(),
            ));
        }
        Ok(())
    }

    fn on_help(&mut self, info: &str, _level: DescriptionLevel) {
        let _ = writeln!(self.stdout, "{info}");
    }

    fn on_version(&mut self, info: &str) {
        let _ = writeln!(
            self.stdout,
            "{info}\nlibpotassco version {LIB_POTASSCO_VERSION}\nCopyright (C) Benjamin Kaufmann\nLicense: The MIT License <https://opensource.org/licenses/MIT>"
        );
    }

    fn setup(&mut self) {}

    fn run(&mut self) {
        match catch_unwind(AssertUnwindSafe(|| self.execute())) {
            Ok(Ok(())) => {}
            Ok(Err(text)) => self.handle_conversion_error(&text),
            Err(payload) => self.handle_conversion_error(&payload_to_string(payload)),
        }
    }

    fn on_unhandled_exception(&mut self, msg: &str) -> bool {
        let _ = writeln!(self.stderr, "{msg}");
        false
    }

    fn flush(&mut self) {
        let _ = self.stdout.flush();
        let _ = self.stderr.flush();
    }
}

pub fn run_lpconvert(
    args: &[&str],
    input: &[u8],
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let mut app = LpConvertApp::new(input, stdout, stderr);
    app.main(args)
}

fn payload_to_string(payload: Box<dyn Any + Send>) -> String {
    if let Some(err) = payload.downcast_ref::<PotasscoError>() {
        err.to_string()
    } else if let Some(err) = payload.downcast_ref::<ProgramOptionsError>() {
        err.to_string()
    } else if let Some(err) = payload.downcast_ref::<String>() {
        err.clone()
    } else if let Some(err) = payload.downcast_ref::<&'static str>() {
        (*err).to_owned()
    } else {
        "Unknown exception".to_owned()
    }
}
