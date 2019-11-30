mod constants;
mod serialize;
mod tokenizer;
mod value_tree_builder;

pub use serialize::DateTimeVer30;
pub use serialize::Serializer;
pub use serialize::Value;

pub use tokenizer::Tokenizer;
pub use value_tree_builder::ValueTreeBuilder;

#[cfg(test)]
mod tests {
    use super::*;
    use enum_extract::let_extract;
    use hex::decode;

    #[test]
    fn tokenize() {
        // serialized frps data
        let data = hex::decode(r#"ca110201680b746573742e6d6574686f644005112006737472696e67580538011050050b656d70747920617272617958000c656d70747920737472756374500013686e75736e6520646f75626c65206369736c6f18d9ded9e411d94cc0026964382a047479706520087175657374696f6e5800500050050b656d70747920617272617958000c656d70747920737472756374500013686e75736e6520646f75626c65206369736c6f18d9ded9e411d94cc0026964382a047479706520087175657374696f6e2800d8b53956784a44f63360301c6a61206e6120746f206d616d206a61207365206e657a74726174696d"#).unwrap();

        // result Value tree
        let mut tree = value_tree_builder::ValueTreeBuilder::new();

        // Tokenizer
        let mut tokenizer = tokenizer::Tokenizer::new();
        let res = tokenizer.parse(&data, &mut tree);
        if let Err(e) = res {
            println!("Tokenizer returned: {}", e);
        }
        assert_eq!(res.is_ok(), true);
        assert_eq!(tree.major_version, 2);
        assert_eq!(tree.minor_version, 1);
    
        dbg!(tree.values);
    }

    #[test]
    fn serialize_deserialize_call_with_nested_arrays_and_ints() {
        // First serialize call with array as value
        let mut serializer = Serializer::new();
        let mut buffer: [u8; 256] = [0; 256];

        let mut written = 0;
        let cnt = serializer.write_call(&mut buffer, "server.stat");
        assert_eq!(cnt.is_ok(), true);
        written += cnt.unwrap();

        serializer.reset();

        // [1, [2, 3]]
        let arr = vec![
            Value::Int(1),
            Value::Array(vec![Value::Int(2), Value::Int(3)]),
        ];

        let cnt = serializer.write_value(&mut buffer[written..], &Value::Array(arr));
        assert_eq!(cnt.is_ok(), true);
        written += cnt.unwrap();

        println!("Serialized data len: {}", written);

        // now deserialize
        let mut call = value_tree_builder::ValueTreeBuilder::new();

        // Tokenizer
        let mut tokenizer = tokenizer::Tokenizer::new();
        let res = tokenizer.parse(&buffer[0..written], &mut call);
        if let Err(e) = res {
            println!("Serializer returned: {}", e);
        }

        assert!(res.is_ok(), "tokenizer returned error");
        assert_eq!(call.method_name, "server.stat");
        // call.value == [1, [2, 3]]
        let_extract!(Value::Array(v), &call.values[0], unreachable!());

        assert_eq!(v.len(), 2, "there are not 2 elements in array");

        // last == [2, 3]
        if let Some(last) = v.last() {
            let last_cnt = match last {
                Value::Array(arr) => arr.len(),
                _ => 0,
            };

            assert_eq!(
                last_cnt, 2,
                "there are not 2 elements in the last element which is array also"
            );
        }
    }
}
