use rust_clasp::clasp::literal::{
    Literal, WeightLiteral, decode_lit, decode_var, encode_lit, false_value, hash_lit, is_sentinel,
    lit_false, lit_true, neg_lit, pos_lit, sent_var, swap, to_int, to_lit, true_value, val_sign,
    value_false, value_true, var_max,
};
use std::cmp::Ordering;

#[test]
fn test_ctor() {
    let p = Literal::default();
    let q = Literal::new(42, true);

    assert_eq!(p.var(), 0);
    assert!(!p.sign());
    assert!(!p.flagged());

    assert_eq!(q.var(), 42);
    assert!(q.sign());
    assert!(!q.flagged());

    let x = pos_lit(7);
    let y = neg_lit(7);
    assert_eq!(x.var(), y.var());
    assert_eq!(y.var(), 7);
    assert!(!x.sign());
    assert!(y.sign());
}

#[test]
fn test_id() {
    let min = lit_true;
    let mid = pos_lit(var_max / 2);
    let max = pos_lit(var_max - 1);

    assert_eq!(min.id(), 0);
    assert_eq!((!min).id(), 1);

    assert_eq!(max.id(), max.var() * 2);
    assert_eq!((!max).id(), (max.var() * 2) + 1);

    assert_eq!(mid.id(), mid.var() * 2);
    assert_eq!((!mid).id(), (mid.var() * 2) + 1);
}

#[test]
fn test_id_ignores_flag() {
    let max = pos_lit(var_max - 1);
    let mut flagged = max;
    flagged.flag();
    assert_eq!(max.id(), flagged.id());
}

#[test]
fn test_from_id() {
    let min = lit_true;
    let mid = pos_lit(var_max / 2);
    let max = pos_lit(var_max - 1);

    assert_eq!(min, Literal::from_id(min.id()));
    assert_eq!(mid, Literal::from_id(mid.id()));
    assert_eq!(max, Literal::from_id(max.id()));
}

#[test]
fn test_flag() {
    let mut p = pos_lit(42);
    p.flag();
    assert!(p.flagged());
    p.unflag();
    assert!(!p.flagged());
}

#[test]
fn test_flag_copy() {
    let mut p = pos_lit(42);
    p.flag();
    let q = p;
    assert!(q.flagged());
}

#[test]
fn test_complement() {
    let lit = pos_lit(7);
    let complement = !lit;
    assert_eq!(lit.var(), complement.var());
    assert!(!lit.sign());
    assert!(complement.sign());
    assert_eq!(lit, !!lit);
}

#[test]
fn test_complement_is_not_flagged() {
    let mut lit = pos_lit(7);
    lit.flag();
    let complement = !lit;
    assert!(!complement.flagged());
}

#[test]
fn test_equality() {
    let p = pos_lit(42);
    let q = neg_lit(42);
    assert_eq!(p, p);
    assert_eq!(p, !q);
    assert_ne!(p, q);
    assert_eq!(Literal::default(), Literal::default());
}

#[test]
fn test_value() {
    assert_eq!(value_true, true_value(lit_true));
    assert_eq!(value_false, true_value(lit_false));
    assert_eq!(value_false, false_value(lit_true));
    assert_eq!(value_true, false_value(lit_false));
}

#[test]
fn test_less() {
    let p = pos_lit(42);
    let q = neg_lit(42);
    assert_eq!(p.cmp(&p), Ordering::Equal);
    assert_eq!(q.cmp(&q), Ordering::Equal);
    assert!(p < q);
    assert!(q >= p);

    let one = Literal::new(1, false);
    let two = Literal::new(2, true);
    let neg_one = !one;
    assert!(one < two);
    assert!(neg_one < two);
    assert!(two >= neg_one);
}

#[test]
fn test_helper_round_trips() {
    assert_eq!(to_lit(7), pos_lit(7));
    assert_eq!(to_lit(-7), neg_lit(7));
    assert_eq!(to_int(pos_lit(9)), 9);
    assert_eq!(to_int(neg_lit(9)), -9);
    assert!(is_sentinel(lit_true));
    assert!(is_sentinel(lit_false));
    assert_eq!(encode_lit(lit_true), 1);
    assert_eq!(encode_lit(neg_lit(2)), -3);
    assert_eq!(decode_var(3), 2);
    assert_eq!(decode_var(-3), 2);
    assert_eq!(decode_lit(encode_lit(lit_true)), lit_true);
    assert_eq!(decode_lit(encode_lit(neg_lit(2))), neg_lit(2));
}

#[test]
fn test_hash_and_xor_helpers() {
    assert_ne!(hash_lit(lit_true), hash_lit(lit_false));
    assert_eq!(lit_true ^ true, lit_false);
    assert_eq!(lit_true ^ false, lit_true);
    assert_eq!(true ^ lit_true, lit_false);
}

#[test]
fn test_swap() {
    let mut left = pos_lit(1);
    let mut right = neg_lit(2);
    swap(&mut left, &mut right);
    assert_eq!(left, neg_lit(2));
    assert_eq!(right, pos_lit(1));
}

#[test]
fn test_weight_literal_ordering() {
    assert_eq!(
        WeightLiteral {
            lit: lit_true,
            weight: 2
        },
        WeightLiteral {
            lit: lit_true,
            weight: 2
        }
    );
    assert_ne!(
        WeightLiteral {
            lit: lit_false,
            weight: 2
        },
        WeightLiteral {
            lit: lit_true,
            weight: 2
        }
    );
    assert_ne!(
        WeightLiteral {
            lit: lit_false,
            weight: 1
        },
        WeightLiteral {
            lit: lit_true,
            weight: 2
        }
    );
    assert!(
        WeightLiteral {
            lit: lit_true,
            weight: 2
        } < WeightLiteral {
            lit: lit_false,
            weight: 1
        }
    );
    assert!(
        WeightLiteral {
            lit: lit_true,
            weight: 1
        } < WeightLiteral {
            lit: lit_true,
            weight: 2
        }
    );
}

#[test]
fn test_value_sign_and_display() {
    assert!(!val_sign(value_true));
    assert!(val_sign(value_false));
    assert_eq!(lit_true.to_string(), "0");
    assert_eq!(neg_lit(12).to_string(), "-12");
    assert_eq!(
        WeightLiteral {
            lit: neg_lit(5),
            weight: 3
        }
        .to_string(),
        "(-5, 3)"
    );
    assert_eq!(sent_var, 0);
}

#[test]
fn upstream_literal_test_antecedent_sections_remain_blocked_on_solver_types() {}
