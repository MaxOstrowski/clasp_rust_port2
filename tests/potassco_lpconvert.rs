use rust_clasp::potassco::app::lpconvert::run_lpconvert;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn lpconvert_converts_simple_aspif_program_to_text() {
    let program = concat!("asp 1 0 0\n", "1 0 1 1 0 0\n", "4 1 a 0\n", "0\n");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_lpconvert(&["--text"], program.as_bytes(), &mut stdout, &mut stderr);
    assert_eq!(code, 0);
    assert!(String::from_utf8(stdout).unwrap().contains("a."));
    assert!(stderr.is_empty());
}

#[test]
fn lpconvert_rejects_mutually_exclusive_text_and_format_options() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_lpconvert(
        &["--text", "--format=aspif"],
        b"asp 1 0 0\n0\n",
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(code, 1);
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("mutually exclusive")
    );
}

#[test]
fn lpconvert_reports_invalid_aux_predicate_like_upstream() {
    let program = concat!("asp 1 0 0\n", "1 0 1 1 0 0\n", "4 1 a 0\n", "0\n");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_lpconvert(
        &["--text", "--aux-pred=1bad"],
        program.as_bytes(),
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    let stderr = String::from_utf8(stderr).unwrap();
    assert!(stderr.contains("invalid aux predicate: '1bad'"));
    assert!(stderr.contains("atom prefix (e.g. 'x_') or unary predicate (e.g. '_id/1') expected"));
}

#[test]
fn lpconvert_reads_positional_input_file_and_writes_output_file() {
    let input_path = temp_path("lpconvert-input", ".aspif");
    let output_path = temp_path("lpconvert-output", ".lp");
    let program = concat!("asp 1 0 0\n", "1 0 1 1 0 0\n", "4 1 a 0\n", "0\n");
    fs::write(&input_path, program).unwrap();

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_lpconvert(
        &[
            "--text",
            input_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ],
        b"",
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
    assert!(fs::read_to_string(&output_path).unwrap().contains("a."));

    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&output_path);
}

#[test]
fn lpconvert_rejects_identical_input_and_output_files() {
    let path = temp_path("lpconvert-same", ".aspif");
    fs::write(&path, "asp 1 0 0\n0\n").unwrap();

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_lpconvert(
        &[path.to_str().unwrap(), "--output", path.to_str().unwrap()],
        b"",
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("Input and output must be different")
    );

    let _ = fs::remove_file(&path);
}

fn temp_path(prefix: &str, suffix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{unique}{suffix}", std::process::id()))
}
