use rust_clasp::clasp::cli::clasp_app::{
    ClaspApp, ClaspAppBase, ClaspAppOptions, ColorMode, ExitCode, InputPtr, LemmaLogger,
    LemmaLoggerOptions, OutputFormat, PreFormat, ReifyFlag, WriteCnf,
};
use rust_clasp::potassco::platform::CFile;

#[test]
fn pre_format_matches_upstream_discriminants() {
    assert_eq!(PreFormat::No as u8, 0);
    assert_eq!(PreFormat::Aspif as u8, 1);
    assert_eq!(PreFormat::Smodels as u8, 2);
    assert_eq!(PreFormat::Reify as u8, 3);
    assert_eq!(PreFormat::default(), PreFormat::No);
}

#[test]
fn exit_code_matches_upstream_values() {
    assert_eq!(ExitCode::Unknown as i32, 0);
    assert_eq!(ExitCode::Interrupt as i32, 1);
    assert_eq!(ExitCode::Sat as i32, 10);
    assert_eq!(ExitCode::Exhaust as i32, 20);
    assert_eq!(ExitCode::Memory as i32, 33);
    assert_eq!(ExitCode::Error as i32, 65);
    assert_eq!(ExitCode::NoRun as i32, 128);
}

#[test]
fn write_cnf_tracks_stream_member_like_upstream_shell() {
    let mut writer = WriteCnf::new("out.cnf");

    assert!(!writer.is_open());

    writer.attach_raw_stream_for_test(core::ptr::dangling_mut::<CFile>());
    assert!(writer.is_open());

    writer.close();
    assert!(!writer.is_open());
}

#[test]
fn lemma_logger_options_defaults_match_upstream_members() {
    let options = LemmaLoggerOptions::default();

    assert_eq!(options.log_max, u32::MAX);
    assert_eq!(options.lbd_max, u32::MAX);
    assert!(!options.dom_out);
    assert!(!options.log_text);
}

#[test]
fn lemma_logger_tracks_all_member_fields_from_header() {
    let options = LemmaLoggerOptions {
        log_max: 12,
        lbd_max: 7,
        dom_out: true,
        log_text: true,
    };
    let mut logger = LemmaLogger::new("stdout", options);

    assert!(logger.is_open());
    assert!(!logger.is_asp());
    assert_eq!(logger.step(), 0);
    assert_eq!(logger.logged_count(), 0);
    assert_eq!(logger.options(), options);
    assert_eq!(logger.solver2_asp(), &[]);
    assert_eq!(logger.solver2_name_idx(), &[]);

    logger.close();
    assert!(!logger.is_open());
}

#[test]
fn clasp_app_options_default_layout_matches_upstream_members() {
    let options = ClaspAppOptions::default();

    assert_eq!(options.input, Vec::<String>::new());
    assert_eq!(options.lemma_log, "");
    assert_eq!(options.lemma_in, "");
    assert_eq!(options.hcc_out, "");
    assert_eq!(options.outf, OutputFormat::Def);
    assert_eq!(options.compute, 0);
    assert_eq!(options.lemma, LemmaLoggerOptions::default());
    assert_eq!(
        options.quiet,
        [
            ClaspAppOptions::Q_DEF,
            ClaspAppOptions::Q_DEF,
            ClaspAppOptions::Q_DEF
        ]
    );
    assert_eq!(options.pre, PreFormat::No);
    assert_eq!(options.reify, ReifyFlag::NONE);
    assert_eq!(options.ifs, ' ');
    assert_eq!(options.pred_sep, '\0');
    assert!(!options.hide_aux);
    assert!(!options.print_port);
    assert_eq!(options.color, ColorMode::Auto);
}

#[test]
fn clasp_app_options_text_output_and_reify_flags_match_header_semantics() {
    assert!(ClaspAppOptions::is_text_output(OutputFormat::Def));
    assert!(ClaspAppOptions::is_text_output(OutputFormat::Comp));
    assert!(!ClaspAppOptions::is_text_output(OutputFormat::Json));
    assert!(!ClaspAppOptions::is_text_output(OutputFormat::None));

    let mut flags = ReifyFlag::NONE;
    flags |= ReifyFlag::SCC;
    flags |= ReifyFlag::STEP;

    assert_eq!(flags.bits(), 3);
    assert!(flags.contains(ReifyFlag::SCC));
    assert!(flags.contains(ReifyFlag::STEP));
}

#[test]
fn input_ptr_defaults_to_null_stream_and_deleter() {
    let input = InputPtr::new();

    assert!(input.stream().is_none());
    assert!(input.deleter().is_none());
}

#[test]
fn clasp_app_base_default_layout_matches_header_members() {
    let app = ClaspAppBase::new();

    assert_eq!(
        app.clasp_config,
        rust_clasp::clasp::cli::clasp_options::ClaspCliConfig::default()
    );
    assert_eq!(app.clasp_app_opts, ClaspAppOptions::default());
    assert!(app.clasp.is_none());
    assert!(app.out.is_none());
    assert!(app.logger.is_none());
    assert!(app.lemma_in.is_none());
    assert!(app.input.stream().is_none());
    assert!(app.input.deleter().is_none());
    assert_eq!(app.fpu_mode, 0);
}

#[test]
fn clasp_app_wraps_base_without_extra_member_state() {
    let app = ClaspApp::new();

    assert_eq!(app.base.clasp_app_opts, ClaspAppOptions::default());
    assert!(app.base.clasp.is_none());
    assert_eq!(app.base.fpu_mode, 0);
}
