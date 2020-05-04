mod common;
mod serialize;
mod tokenizer;
mod value_tree_builder;

pub use serialize::Serializer;
pub use tokenizer::Tokenizer;
pub use value_tree_builder::{ParsedStatus, ValueTreeBuilder};

use std::collections::{BTreeMap, HashMap};
use std::fmt;

#[derive(Debug)]
pub enum Value {
    Int(i64),
    Str(String),
    Null,
    DateTime(i64), // unix timestamp (UTC) can be negative :-)
    Struct(HashMap<String, Value>),
    Array(Vec<Value>),
    Double(f64),
    Bool(bool),
    Binary(Vec<u8>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", Value::_to_string(self))
    }
}

impl Value {
    // recursive implementation
    fn _to_string(val: &Value) -> String {
        match val {
            Value::Int(v) => v.to_string(),
            Value::Double(v) => v.to_string(),
            Value::Null => "null".to_owned(),
            Value::Bool(v) => {
                if *v {
                    "true".to_owned()
                } else {
                    "false".to_owned()
                }
            }
            Value::DateTime(v) => {
                time::PrimitiveDateTime::from_unix_timestamp(*v).format("%Y-%m-%d %H:%M:%S")
            }
            Value::Str(v) => "\"".to_owned() + v + &"\"".to_owned(),
            Value::Binary(v) => "b\"".to_owned() + &hex::encode(v) + &"\"".to_owned(),
            Value::Array(v) => {
                let mut cnt: usize = 0;
                let total = v.len();

                "(".to_owned()
                    + &v.iter()
                        .map(|x| {
                            cnt += 1;
                            if cnt < total {
                                Value::to_string(&x) + &", ".to_owned()
                            } else {
                                Value::to_string(&x)
                            }
                        })
                        .collect::<String>()
                    + &")".to_owned()
            }
            Value::Struct(v) => {
                let mut cnt: usize = 0;
                let total = v.len();

                // We want sorted according keys so we use BTreeMap
                let v: BTreeMap<_, _> = v.iter().collect();
                return "{".to_owned()
                    + &v.iter()
                        .map(|(k, x)| {
                            cnt += 1;

                            if cnt < total {
                                (*k).to_string() + ": " + &Value::to_string(&x) + ", "
                            } else {
                                (*k).to_string() + ": " + &Value::to_string(&x)
                            }
                        })
                        .collect::<String>()
                    + &"}".to_owned();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enum_extract::let_extract;

    #[test]
    fn tokenize() {
        // serialized frps data
        let data = hex::decode(r#"ca110201680b746573742e6d6574686f644005112006737472696e67580538011050050b656d70747920617272617958000c656d70747920737472756374500013686e75736e6520646f75626c65206369736c6f18d9ded9e411d94cc0026964382a047479706520087175657374696f6e5800500050050b656d70747920617272617958000c656d70747920737472756374500013686e75736e6520646f75626c65206369736c6f18d9ded9e411d94cc0026964382a047479706520087175657374696f6e2800d8b53956784a44f63360301c6a61206e6120746f206d616d206a61207365206e657a74726174696d"#).unwrap();

        // result Value tree
        let mut tree = value_tree_builder::ValueTreeBuilder::new();

        // Tokenizer
        let mut tokenizer = tokenizer::Tokenizer::new_frps();
        let res = tokenizer.parse(&data, &mut tree);
        if let Err(e) = res {
            println!("Tokenizer returned: {}", e);
        }
        assert_eq!(res.is_ok(), true);
        assert_eq!(tree.major_version, 2);
        assert_eq!(tree.minor_version, 1);
        // dbg!(tree.values);
    }

    #[test]
    fn serialized_tokenize_fault() {
        let mut serializer = Serializer::new();

        let mut buffer: [u8; 32] = [0; 32];

        let res = serializer.write_fault(&mut buffer, 500, "FAULT_TEST");
        assert_eq!(res.is_ok(), true);

        // result Value tree
        let mut tree = value_tree_builder::ValueTreeBuilder::new();

        // Tokenizer
        let mut tokenizer = tokenizer::Tokenizer::new_frpc();
        let res = tokenizer.parse(&buffer[..res.unwrap()], &mut tree);
        if let Err(e) = res {
            println!("Tokenizer returned: {}", e);
        }
        assert_eq!(res.is_ok(), true);
        // dbg!(buffer, tree.values);
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

        // println!("Serialized data len: {}", written);

        // now deserialize
        let mut call = value_tree_builder::ValueTreeBuilder::new();

        // Tokenizer
        let mut tokenizer = tokenizer::Tokenizer::new_frpc();
        let res = tokenizer.parse(&buffer[0..written], &mut call);
        if let Err(e) = res {
            println!("Serializer returned: {}", e);
        }

        assert!(res.is_ok(), "tokenizer returned error");
        match call.what {
            ParsedStatus::MethodCall(name) => assert_eq!(name, "server.stat"),
            _ => assert!(false, "invalid call"),
        }
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

    use std::env;
    use std::fs::File;
    use std::io::{self, prelude::*, BufReader};

    fn test_file(
        name: &str,
        is_frps: bool,
        call: impl Fn(&i32, &i32, &String, &String, &String, &String, bool),
    ) -> io::Result<()> {
        let mut path = env::current_dir()?;
        path.push(name);

        // println!("The current directory is {}", path.display());
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let mut test_name = String::new();
        let mut frps_data = String::new();
        let mut result = String::new();
        let mut binary_data = String::new();

        let mut cnt = 0;
        let mut line_cnt = 0;
        for line in reader.lines() {
            line_cnt += 1;
            let line = line?;

            // skip comments
            if line.starts_with("#") {
                continue;
            }

            if line.starts_with('@') {
                test_name = line;
                continue;
            }

            if line.starts_with(' ') || (line.len() == 0) {
                if !frps_data.is_empty()
                /*&& cnt == 86*/
                {
                    call(
                        &cnt,
                        &line_cnt,
                        &test_name,
                        &frps_data,
                        &result,
                        &binary_data,
                        is_frps,
                    );
                }

                // clean for next test
                test_name.clear();
                frps_data.clear();
                binary_data.clear();
                result.clear();
                continue;
            }

            // this line is input data to process
            if frps_data.is_empty() {
                cnt += 1;
                frps_data = line;
                continue;
            }
            // this is result
            if result.is_empty() {
                result = line;
                continue;
            }

            // this is binary data
            binary_data = line;
        }

        Ok(())
    }

    fn test_by_chunk(
        order: &i32,
        line: &i32,
        test_name: &String,
        frps_data: &String,
        result: &String,
        binary_data: &String,
        is_frps: bool,
    ) {
        let mut tokenizer = if is_frps {
            tokenizer::Tokenizer::new_frps()
        } else {
            tokenizer::Tokenizer::new_frpc()
        };
        let mut call = value_tree_builder::ValueTreeBuilder::new();
        let mut in_string = false;
        // println!(
        //     "\nRunning test: #{} - {} result:{}",
        //     order, test_name, result
        // );

        let mut data_after_end = false;
        let mut need_data = true;
        // First separete data by `"` and then feed tokenizer with all data,
        // regardless tokenizer returned that is not expecting_data.
        // This we will test that tokenizer detect 'data after end' cases.
        'outer: for p in frps_data.split('"') {
            let chunk_size: usize;
            let res = if in_string {
                // string is encoded as is
                chunk_size = p.as_bytes().len();
                tokenizer.parse(p.as_bytes(), &mut call)
            } else {
                // filter all whitespace characters from data
                let no_whitespace: String = p.chars().filter(|&c| !c.is_whitespace()).collect();
                // convert hex characters to bytes
                let chunk_frps = hex::decode(no_whitespace).unwrap();
                // try tokenize
                chunk_size = chunk_frps.len();
                tokenizer.parse(chunk_frps.as_slice(), &mut call)
            };

            match res {
                Ok((expecting_data, processed)) => {
                    need_data = expecting_data;
                    if !expecting_data {
                        data_after_end = processed < chunk_size;
                        if data_after_end {
                            break 'outer;
                        }
                    }
                }
                Err(_pos) => {
                    if !result.starts_with("error") {
                        assert!(res.is_ok(), "result should not error");
                    };

                    need_data = false;
                    data_after_end = false;
                    break 'outer;
                }
            }
            in_string = if in_string { false } else { true };
        }

        let res = if need_data {
            String::from("error(unexpected data end)")
        } else if data_after_end {
            String::from("error(data after end)")
        } else {
            format!("{}", call)
        };

        if *result != res {
            println!(
                "Failed test line:{} - #{} - {} => {} != {}",
                line, order, test_name, result, res
            );
        }

        if !binary_data.is_empty() {
            let binary_data: String = binary_data
                .chars()
                .filter(|&c| !c.is_whitespace())
                .collect();

            let parsed_data = hex::encode(call.data);
            if binary_data != parsed_data {
                println!(
                    "Failed test line:{} - #{} - {} => streamdata {} != {}",
                    line, order, test_name, binary_data, parsed_data
                );
            }
        }
    }

    // Testing tokenizer by one byte buffer len will go trought all inner states
    fn test_by_one_byte(
        order: &i32,
        line: &i32,
        test_name: &String,
        frps_data: &String,
        result: &String,
        binary_data: &String,
        is_frps: bool,
    ) {
        let mut tokenizer = if is_frps {
            tokenizer::Tokenizer::new_frps()
        } else {
            tokenizer::Tokenizer::new_frpc()
        };

        let mut call = value_tree_builder::ValueTreeBuilder::new();
        let mut in_string = false;
        // println!(
        //     "\nRunning test: #{} - {} result:{}",
        //     order, test_name, result
        // );

        let mut data_after_end = false;
        let mut need_data = true;
        // First separete data by `"` and then feed tokenizer with all data,
        // regardless tokenizer returned that is not expecting_data.
        // This we will test that tokenizer detect 'data after end' cases.
        let mut data = vec![];
        for p in frps_data.split('"') {
            if in_string {
                data.extend_from_slice(p.as_bytes());
            } else {
                // filter all whitespace characters from data
                let no_whitespace: String = p.chars().filter(|&c| !c.is_whitespace()).collect();
                // convert hex characters to bytes
                let p = hex::decode(no_whitespace).unwrap();
                data.extend_from_slice(&p);
            }
            in_string = if in_string { false } else { true };
        }

        for x in 0..data.len() {
            let res = tokenizer.parse(&data[x..x + 1], &mut call);
            match res {
                Ok((expecting_data, processed)) => {
                    need_data = expecting_data;
                    if !expecting_data {
                        data_after_end = processed < 1;
                        if data_after_end {
                            break;
                        }
                    }
                }
                Err(_pos) => {
                    if !result.starts_with("error") {
                        assert!(res.is_ok(), "result should not error");
                    };

                    need_data = false;
                    data_after_end = false;
                    break;
                }
            }
        }

        let res = if need_data {
            String::from("error(unexpected data end)")
        } else if data_after_end {
            String::from("error(data after end)")
        } else {
            format!("{}", call)
        };

        if *result != res {
            println!(
                "Failed test line:{} - #{} - {} => {} != {}",
                line, order, test_name, result, res
            );
        }

        if !binary_data.is_empty() {
            let binary_data: String = binary_data
                .chars()
                .filter(|&c| !c.is_whitespace())
                .collect();

            let parsed_data = hex::encode(call.data);
            if binary_data != parsed_data {
                println!(
                    "Failed test line:{} - #{} - {} => streamdata {} != {}",
                    line, order, test_name, binary_data, parsed_data
                );
            }
        }
    }

    fn test_serialize_deserialize(
        order: &i32,
        line: &i32,
        test_name: &String,
        frps_data: &String,
        result: &String,
        binary_data: &String,
        is_frps: bool,
    ) {
        // skip errors
        if result.starts_with("error") {
            return;
        }

        // println!(
        //     "\nRunning test: #{} - {} result:{}",
        //     order, test_name, result
        // );

        // First separate data by `"` and then feed tokenizer with all data,
        // regardless tokenizer returned that is not expecting_data.
        // This we will test that tokenizer detect 'data after end' cases.
        let mut data = vec![];
        let mut in_string = false;
        for p in frps_data.split('"') {
            if in_string {
                data.extend_from_slice(p.as_bytes());
            } else {
                // filter all whitespace characters from data
                let no_whitespace: String = p.chars().filter(|&c| !c.is_whitespace()).collect();
                // convert hex characters to bytes
                let p = hex::decode(no_whitespace).unwrap();
                data.extend_from_slice(&p);
            }
            in_string = if in_string { false } else { true };
        }

        let binary_data: String = binary_data
            .chars()
            .filter(|&c| !c.is_whitespace())
            .collect();

        let mut tokenizer = if is_frps {
            tokenizer::Tokenizer::new_frps()
        } else {
            tokenizer::Tokenizer::new_frpc()
        };

        let mut call = value_tree_builder::ValueTreeBuilder::new();
        let res = tokenizer.parse(&data[0..], &mut call);
        match res {
            Ok((expecting_data, _processe)) => {
                assert!(expecting_data == false, "should not expect data")
            }
            Err(_pos) => assert!(res.is_ok(), "result should not error"),
        }

        if !binary_data.is_empty() {
            let parsed_data = hex::encode(&call.data);
            if binary_data != parsed_data {
                println!(
                    "Failed test line:{} - #{} - {} => streamdata {} != {}",
                    line, order, test_name, binary_data, parsed_data
                );
            }
        }

        // now try to serialize
        let mut serializer = Serializer::new();

        let mut buffer = vec![];
        buffer.resize(2 * data.len(), 0); // make buffer large enought
        let mut cnt: usize = 0;
        match &call.what {
            ParsedStatus::Fault => {
                assert!(
                    call.values.len() == 2,
                    "There should be Fault with 2 values"
                );

                let_extract!(Value::Int(code), &call.values[0], unreachable!());
                let_extract!(Value::Str(msg), &call.values[1], unreachable!());

                let r = serializer.write_fault(&mut buffer[cnt..], *code, msg.as_str());
                cnt += r.unwrap();
            }
            ParsedStatus::MethodCall(name) => {
                let r = serializer.write_call(&mut buffer[cnt..], name.as_str());
                cnt += r.unwrap();
                for i in 0..call.values.len() {
                    serializer.reset();
                    let r = serializer.write_value(&mut buffer[cnt..], &call.values[i]);
                    cnt += r.unwrap();
                }
            }
            ParsedStatus::Response => {
                assert!(call.values.len() == 1);

                let r = serializer.write_response(&mut buffer[cnt..], &call.values[0]);
                cnt += r.unwrap();
            }
            _ => return,
        }

        if !binary_data.is_empty() {
            serializer.reset();
            let r = serializer.write_data(&mut buffer[cnt..], &call.data);
            cnt += r.unwrap();
        }

        // Parse again to new call
        let mut call2 = value_tree_builder::ValueTreeBuilder::new();

        tokenizer.reset();
        let res = tokenizer.parse(&buffer[0..cnt], &mut call2);
        match res {
            Ok((expecting_data, _processed)) => {
                assert!(expecting_data == false, "should not expect data")
            }
            Err(_pos) => assert!(res.is_ok(), "result should not error"),
        }

        if !binary_data.is_empty() {
            let parsed_data = hex::encode(&call2.data);
            if binary_data != parsed_data {
                println!(
                    "Failed test line:{} - #{} - {} => streamdata {} != {}",
                    line, order, test_name, binary_data, parsed_data
                );
            }
        }

        // finaly compare result
        assert_eq!(&call.to_string(), &call2.to_string());
    }

    #[test]
    fn test_frpc() {
        let res = test_file("tests/frpc.tests", false, &test_by_chunk);
        assert!(res.is_ok());
    }

    #[test]
    fn test_frpc_by_one_byte() {
        let res = test_file("tests/frpc.tests", false, &test_by_one_byte);
        assert!(res.is_ok());
    }

    #[test]
    fn test_frpc_serialized_deserialize() {
        let res = test_file("tests/frpc.tests", false, &test_serialize_deserialize);
        assert!(res.is_ok());
    }

    #[test]
    fn test_frps() {
        let res = test_file("tests/frps.tests", true, &test_by_chunk);
        assert!(res.is_ok());
    }

    #[test]
    fn test_frps_by_one_byte() {
        let res = test_file("tests/frps.tests", true, &test_by_one_byte);
        assert!(res.is_ok());
    }

    #[test]
    fn test_frps_serialize_deserialize() {
        let res = test_file("tests/frps.tests", true, &test_serialize_deserialize);
        assert!(res.is_ok());
    }
}
