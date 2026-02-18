use valuecore::v0::{self, V};

#[test]
fn gate10_value_cid_vectors_are_frozen() {
    let vectors: Vec<(V, &str)> = vec![
        (V::Unit, "sha256:91e321035af75af8327b2d94d23e1fa73cfb5546f112de6a65e494645148a3ea"),
        (V::Bool(false), "sha256:4edf146e54aeae5a988adc5f8e6961ebcda73457d71cd354d2b3c152b65b7b3c"),
        (V::Bool(true), "sha256:474e734415c728f930d7a264ba36465c10ccea9aa594f9fadf2eaf0281c3ec00"),
        (V::Int(0), "sha256:8647fe4dbd2e599b82e8333a8623aae5a1b1955ca59267cf1b16b4bccd65cd99"),
        (V::Int(-7), "sha256:5c25b5b5e300e1abe8cb9d4474e883e49849496fbf6afa656732c1964149ec84"),
        (V::Text("".to_string()), "sha256:bddb2e55dca4041e6e75466dc018e5fdbc0193152ce42088518f6b7ba3b16b3f"),
        (V::Text("a".to_string()), "sha256:b65bc2ddfe489b49ddd850d0615c8282d1d103aa035b7caf1bc702e61cd40748"),
        (V::Text("a\nb".to_string()), "sha256:84573e4b9c48ba161ec0dab9d4ac03467e5de0245675b10d4e3747d4433d6b51"),
        (V::Bytes(vec![]), "sha256:ca3e3c3ffe02add703409cd4878df13b9d525501562eddc6d554f7e5cb436ebd"),
        (V::Bytes(vec![0, 255, 16]), "sha256:b3327ca228633cef57155a637fe00f3021f234d03f9e57449108246ea7a8ec35"),
        (V::List(vec![]), "sha256:9207ca3117bcffdc76df1ce260ecdd52a4a0c74a278d94687f717944eb0baaa4"),
        (V::List(vec![V::Unit, V::Int(1)]), "sha256:74b165fffe99956918b4bdc7fe32054e7f8447c31b8abff3179701baa14906a8"),
        (
            V::Map(vec![
                ("a".to_string(), V::Int(1)),
                ("b".to_string(), V::Int(2)),
            ]),
            "sha256:32c88b60573be546bb4165c81ec4964c08d4b8e4e33e5656bdc4c51366f0e250",
        ),
        (V::Ok(Box::new(V::Int(9))), "sha256:d68c71fd523ad0cfd9e9eaa94c176aa0ef3f5d754166c5dc480592fb636ae305"),
        (V::Err("E1".to_string()), "sha256:bf9d1ff441e89dc9508fef0df234bb7c9ec25a8a1de131782d71e4b8354fa5f3"),
    ];

    for (v, expect) in vectors {
        let got = v0::value_cid(&v);
        assert_eq!(got, expect, "value cid mismatch");
    }
}

#[test]
fn gate10_map_order_does_not_change_value_cid() {
    let a = V::Map(vec![
        ("b".to_string(), V::Int(2)),
        ("a".to_string(), V::Int(1)),
    ]);
    let b = V::Map(vec![
        ("a".to_string(), V::Int(1)),
        ("b".to_string(), V::Int(2)),
    ]);

    let ca = v0::value_cid(&a);
    let cb = v0::value_cid(&b);
    assert_eq!(ca, cb, "map order must not affect cid");
}
