use rust_clasp::clasp::cb_enumerator::{
    CbConsequences, ConsequenceAlgorithm, ConsequenceInitWarning, ConsequenceModelType, EnumMode,
    EnumOptions, UnsatType,
};

#[test]
fn brave_enumerator_forces_default_algorithm() {
    let consequences =
        CbConsequences::new(ConsequenceModelType::Brave, ConsequenceAlgorithm::Query);

    assert_eq!(consequences.model_type(), ConsequenceModelType::Brave);
    assert_eq!(consequences.algorithm(), ConsequenceAlgorithm::Def);
    assert!(consequences.exhaustive());
}

#[test]
fn cautious_query_mode_is_preserved_until_init() {
    let consequences =
        CbConsequences::new(ConsequenceModelType::Cautious, ConsequenceAlgorithm::Query);

    assert_eq!(consequences.model_type(), ConsequenceModelType::Cautious);
    assert_eq!(consequences.algorithm(), ConsequenceAlgorithm::Query);
}

#[test]
fn enum_options_create_brave_and_cautious_variants_like_upstream() {
    let brave = CbConsequences::from_enum_options(EnumOptions {
        enum_mode: EnumMode::Brave,
    });
    let cautious = CbConsequences::from_enum_options(EnumOptions {
        enum_mode: EnumMode::Cautious,
    });
    let query = CbConsequences::from_enum_options(EnumOptions {
        enum_mode: EnumMode::Query,
    });

    assert_eq!(brave.model_type(), ConsequenceModelType::Brave);
    assert_eq!(brave.algorithm(), ConsequenceAlgorithm::Def);

    assert_eq!(cautious.model_type(), ConsequenceModelType::Cautious);
    assert_eq!(cautious.algorithm(), ConsequenceAlgorithm::Def);

    assert_eq!(query.model_type(), ConsequenceModelType::Cautious);
    assert_eq!(query.algorithm(), ConsequenceAlgorithm::Query);
}

#[test]
fn splitting_and_unsat_behavior_match_algorithm_selection() {
    let default_algo =
        CbConsequences::new(ConsequenceModelType::Cautious, ConsequenceAlgorithm::Def);
    let query_algo =
        CbConsequences::new(ConsequenceModelType::Cautious, ConsequenceAlgorithm::Query);

    assert!(default_algo.supports_splitting(true));
    assert!(!default_algo.supports_splitting(false));
    assert_eq!(default_algo.unsat_type(UnsatType::Stop), UnsatType::Stop);
    assert_eq!(default_algo.unsat_type(UnsatType::Cont), UnsatType::Cont);

    assert!(!query_algo.supports_splitting(true));
    assert_eq!(query_algo.unsat_type(UnsatType::Stop), UnsatType::Cont);
    assert_eq!(query_algo.unsat_type(UnsatType::Cont), UnsatType::Cont);
}

#[test]
fn query_mode_falls_back_to_default_under_optimization() {
    let mut consequences =
        CbConsequences::new(ConsequenceModelType::Cautious, ConsequenceAlgorithm::Query);

    let warning = consequences.prepare_for_init(true);

    assert_eq!(
        warning,
        Some(ConsequenceInitWarning::QueryDoesNotSupportOptimization)
    );
    assert_eq!(consequences.algorithm(), ConsequenceAlgorithm::Def);
}

#[test]
fn default_mode_without_optimization_keeps_query_algorithm() {
    let mut consequences =
        CbConsequences::new(ConsequenceModelType::Cautious, ConsequenceAlgorithm::Query);

    let warning = consequences.prepare_for_init(false);

    assert_eq!(warning, None);
    assert_eq!(consequences.algorithm(), ConsequenceAlgorithm::Query);
}
