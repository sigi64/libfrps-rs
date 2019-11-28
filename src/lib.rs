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

    #[test]
    fn tokenize() {
        // serialized frps data
        let data = vec![0xca, 0x11, 0x03, 0x00];

        // result Value tree
        let mut tree = value_tree_builder::ValueTreeBuilder::new();

        // Tokenizer
        let mut tokenizer = tokenizer::Tokenizer::new();
        let res = tokenizer.parse(&data, &mut tree);
        assert_eq!(res.is_ok(), true);
        assert_eq!(tree.major_version, 3);
        assert_eq!(tree.minor_version, 0);
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
        let_extract!(Value::Array(v), call.value, unreachable!());

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
