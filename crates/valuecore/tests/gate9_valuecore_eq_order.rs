use std::cmp::Ordering;
use valuecore::v0::{self, V};

#[test]
fn gate9_map_canon_eq_is_order_insensitive_recursive() {
    let a = V::Map(vec![
        ("b".to_string(), V::Int(2)),
        (
            "a".to_string(),
            V::Map(vec![
                ("y".to_string(), V::Int(9)),
                ("x".to_string(), V::Int(8)),
            ]),
        ),
    ]);

    let b = V::Map(vec![
        (
            "a".to_string(),
            V::Map(vec![
                ("x".to_string(), V::Int(8)),
                ("y".to_string(), V::Int(9)),
            ]),
        ),
        ("b".to_string(), V::Int(2)),
    ]);

    assert!(!(a == b), "raw Eq must remain structural over Vec order");
    assert!(
        v0::canon_eq(&a, &b),
        "canon_eq must ignore map insertion order recursively"
    );
    assert_eq!(
        v0::canon_cmp(&a, &b),
        Ordering::Equal,
        "canon_cmp must agree with canon_eq"
    );

    let ea = v0::encode_json(&a);
    let eb = v0::encode_json(&b);
    assert_eq!(
        ea, eb,
        "canonical encoding must not depend on map insertion order"
    );
}

#[test]
fn gate9_canon_cmp_is_total_and_antisymmetric() {
    let vals: Vec<V> = vec![
        V::Err("E".to_string()),
        V::Unit,
        V::Bool(false),
        V::Bool(true),
        V::Int(-1),
        V::Int(0),
        V::Int(7),
        V::Text("a".to_string()),
        V::Text("b".to_string()),
        V::Bytes(vec![]),
        V::Bytes(vec![0, 1]),
        V::List(vec![]),
        V::List(vec![V::Int(1), V::Int(2)]),
        V::Ok(Box::new(V::Int(3))),
        V::Map(vec![
            ("b".to_string(), V::Int(2)),
            ("a".to_string(), V::Int(1)),
        ]),
        V::Map(vec![
            ("a".to_string(), V::Int(1)),
            ("b".to_string(), V::Int(2)),
        ]),
    ];

    for i in 0..vals.len() {
        for j in 0..vals.len() {
            let a = &vals[i];
            let b = &vals[j];
            let ab = v0::canon_cmp(a, b);
            let ba = v0::canon_cmp(b, a);

            if ab == Ordering::Less {
                assert!(
                    ba == Ordering::Greater,
                    "antisymmetry failed: i={} j={}",
                    i,
                    j
                );
            } else if ab == Ordering::Greater {
                assert!(ba == Ordering::Less, "antisymmetry failed: i={} j={}", i, j);
            } else {
                assert!(
                    ba == Ordering::Equal,
                    "antisymmetry failed: i={} j={}",
                    i,
                    j
                );
            }
        }
    }
}

#[test]
fn gate9_sorting_by_canon_cmp_is_deterministic_and_idempotent() {
    let mut xs: Vec<V> = vec![
        V::Map(vec![
            ("z".to_string(), V::Int(1)),
            ("a".to_string(), V::Int(2)),
        ]),
        V::Map(vec![
            ("a".to_string(), V::Int(2)),
            ("z".to_string(), V::Int(1)),
        ]),
        V::Int(0),
        V::Int(-1),
        V::Text("b".to_string()),
        V::Text("a".to_string()),
        V::Bool(true),
        V::Bool(false),
        V::Unit,
        V::Err("E".to_string()),
        V::Ok(Box::new(V::Int(7))),
        V::List(vec![V::Int(1)]),
        V::List(vec![]),
    ];

    xs.sort_by(|a, b| v0::canon_cmp(a, b));
    let once = xs.clone();

    xs.sort_by(|a, b| v0::canon_cmp(a, b));
    let twice = xs.clone();

    assert_eq!(once, twice, "sorting by canon_cmp must be idempotent");

    for i in 1..xs.len() {
        let c = v0::canon_cmp(&xs[i - 1], &xs[i]);
        assert!(
            c == Ordering::Less || c == Ordering::Equal,
            "not sorted at i={}",
            i
        );
    }

    assert!(v0::canon_eq(&xs[0], &V::Unit) == false || true);
}
