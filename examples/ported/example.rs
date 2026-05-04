//! Partial Rust port of `original_clasp/examples/example.h` and
//! `original_clasp/examples/main.cpp`.

use std::io::{self, Write};

use rust_clasp::clasp::enumerator::Model;
use rust_clasp::clasp::literal::pos_lit;
use rust_clasp::clasp::shared_context::OutputTable;

pub fn print_model(out: &OutputTable, model: &Model<'_>) {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    print_model_to(&mut handle, out, model).expect("writing model output should not fail");
}

pub fn print_model_to<W: Write>(
    writer: &mut W,
    out: &OutputTable,
    model: &Model<'_>,
) -> io::Result<()> {
    writeln!(writer, "Model {}: ", model.num)?;
    for pred in out.pred_range() {
        if model.is_true(pred.cond) {
            write!(writer, "{} ", pred.name)?;
        }
    }
    for var in out.vars_range() {
        let value = if model.is_true(pos_lit(var)) {
            var as i32
        } else {
            -(var as i32)
        };
        write!(writer, "{} ", value)?;
    }
    writeln!(writer)
}
