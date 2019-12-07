mod constants;
mod serialize;
mod tokenizer;
mod value_tree_builder;

pub use serialize::Serializer;
pub use tokenizer::Tokenizer;
pub use value_tree_builder::ValueTreeBuilder;

extern crate chrono;
use chrono::prelude::{DateTime, NaiveDateTime, Utc};
use std::collections::HashMap;

// FRPC version 3.0 format (unix_time is 64 bit)
#[derive(Copy, Clone, Debug)]
pub struct DateTimeVer30 {
    pub time_zone: i16, // as difference between UTC and localtime in seconds
    pub unix_time: u64,
    pub week_day: u8,
    pub sec: u8,
    pub min: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub year: u16,
}

#[derive(Debug)]
pub enum Value {
    Int(i64),
    Str(String),
    Null,
    DateTime(DateTimeVer30),
    Struct(HashMap<String, Value>),
    Array(Vec<Value>),
    Double(f64),
    Bool(bool),
    Binary(Vec<u8>),
}

impl Value {
    // Return String format of value
    pub fn to_string(&self) -> String {
        return Value::_to_string(self);
    }

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
                let naive_datetime = NaiveDateTime::from_timestamp(v.unix_time as i64, 0);

                DateTime::<Utc>::from_utc(naive_datetime, Utc)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
                // v.unix_time.to_string()
            }
            Value::Str(v) => "\"".to_owned() + v + &"\"".to_owned(),
            Value::Binary(v) => "b\"".to_owned() + &hex::encode(v) + &"\"".to_owned(),
            Value::Array(v) => {
                "(".to_owned()
                    + &v.iter().map(|x| Value::to_string(&x)).collect::<String>()
                    + &")".to_owned()
            }
            Value::Struct(v) => {
                let mut cnt: usize = 0;
                let total = v.len();
                return "{".to_owned()
                    + &v.iter()
                        .map(|(k, x)| {
                            cnt += 1;

                            if cnt < total {
                                k.clone() + &": ".to_owned() + &Value::to_string(&x) + &", ".to_owned()
                            } else {
                                k.clone() + &": ".to_owned() + &Value::to_string(&x)
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
    use hex::decode;

    // #[test]
    // fn tokenize() {
    //     // serialized frps data
    //     let data = hex::decode(r#"ca110201680b746573742e6d6574686f644005112006737472696e67580538011050050b656d70747920617272617958000c656d70747920737472756374500013686e75736e6520646f75626c65206369736c6f18d9ded9e411d94cc0026964382a047479706520087175657374696f6e5800500050050b656d70747920617272617958000c656d70747920737472756374500013686e75736e6520646f75626c65206369736c6f18d9ded9e411d94cc0026964382a047479706520087175657374696f6e2800d8b53956784a44f63360301c6a61206e6120746f206d616d206a61207365206e657a74726174696d"#).unwrap();

    //     // result Value tree
    //     let mut tree = value_tree_builder::ValueTreeBuilder::new();

    //     // Tokenizer
    //     let mut tokenizer = tokenizer::Tokenizer::new();
    //     let res = tokenizer.parse(&data, &mut tree);
    //     if let Err(e) = res {
    //         println!("Tokenizer returned: {}", e);
    //     }
    //     assert_eq!(res.is_ok(), true);
    //     assert_eq!(tree.major_version, 2);
    //     assert_eq!(tree.minor_version, 1);
    //     dbg!(tree.values);
    // }

    // #[test]
    // fn serialized_tokenize_fault() {

    //     let mut serializer = Serializer::new();

    //     let mut buffer: [u8; 32] = [0; 32];

    //     let res = serializer.write_fault(&mut buffer, 500, "FAULT_TEST");
    //     assert_eq!(res.is_ok(), true);

    //     // result Value tree
    //     let mut tree = value_tree_builder::ValueTreeBuilder::new();

    //     // Tokenizer
    //     let mut tokenizer = tokenizer::Tokenizer::new();
    //     let res = tokenizer.parse(&buffer[..res.unwrap()], &mut tree);
    //     if let Err(e) = res {
    //         println!("Tokenizer returned: {}", e);
    //     }
    //     assert_eq!(res.is_ok(), true);
    //     dbg!(buffer, tree.values);
    // }

    // #[test]
    // fn serialize_deserialize_call_with_nested_arrays_and_ints() {
    //     // First serialize call with array as value
    //     let mut serializer = Serializer::new();
    //     let mut buffer: [u8; 256] = [0; 256];

    //     let mut written = 0;
    //     let cnt = serializer.write_call(&mut buffer, "server.stat");
    //     assert_eq!(cnt.is_ok(), true);
    //     written += cnt.unwrap();

    //     serializer.reset();

    //     // [1, [2, 3]]
    //     let arr = vec![
    //         Value::Int(1),
    //         Value::Array(vec![Value::Int(2), Value::Int(3)]),
    //     ];

    //     let cnt = serializer.write_value(&mut buffer[written..], &Value::Array(arr));
    //     assert_eq!(cnt.is_ok(), true);
    //     written += cnt.unwrap();

    //     println!("Serialized data len: {}", written);

    //     // now deserialize
    //     let mut call = value_tree_builder::ValueTreeBuilder::new();

    //     // Tokenizer
    //     let mut tokenizer = tokenizer::Tokenizer::new();
    //     let res = tokenizer.parse(&buffer[0..written], &mut call);
    //     if let Err(e) = res {
    //         println!("Serializer returned: {}", e);
    //     }

    //     assert!(res.is_ok(), "tokenizer returned error");
    //     assert_eq!(call.method_name, "server.stat");
    //     // call.value == [1, [2, 3]]
    //     let_extract!(Value::Array(v), &call.values[0], unreachable!());

    //     assert_eq!(v.len(), 2, "there are not 2 elements in array");

    //     // last == [2, 3]
    //     if let Some(last) = v.last() {
    //         let last_cnt = match last {
    //             Value::Array(arr) => arr.len(),
    //             _ => 0,
    //         };

    //         assert_eq!(
    //             last_cnt, 2,
    //             "there are not 2 elements in the last element which is array also"
    //         );
    //     }
    // }

    use std::env;
    use std::fs::File;
    use std::io::{self, prelude::*, BufReader};

    fn test_file(name: &str) -> io::Result<()> {
        let mut path = env::current_dir()?;
        path.push(name);

        println!("The current directory is {}", path.display());
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let mut test_name = String::new();
        let mut frps_data = String::new();
        let mut result = String::new();
        let mut binary_data = String::new();

        let mut cnt = 0;
        for line in reader.lines() {
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
                if !frps_data.is_empty() {
                    run_test(&cnt, &test_name, &frps_data, &result, &binary_data);
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

    fn run_test(
        order: &i32,
        test_name: &String,
        frps_data: &String,
        result: &String,
        binary_data: &String,
    ) {
        let mut tokenizer = tokenizer::Tokenizer::new();
        let mut call = value_tree_builder::ValueTreeBuilder::new();
        let mut in_string = false;
        println!("\nRunning test: #{} - {} result:{}", order, test_name, result);

        // separete data by `"`
        let mut failed = false;
        let mut error_msg = String::new();
        let mut need_data = false;
        'outer: for p in frps_data.split('"') {
            let res = if in_string {
                // string is encoded as is
                tokenizer.parse(p.as_bytes(), &mut call)
            } else {
                // filter all whitespace characters from data
                let no_whitespace: String = p.chars().filter(|&c| !c.is_whitespace()).collect();
                // convert hex to bytes
                let chunk_frps = hex::decode(no_whitespace).unwrap();
                // try tokenize
                tokenizer.parse(chunk_frps.as_slice(), &mut call)
            };

            match res {
                Ok((expecting_data,_cnt)) => {
                    if !expecting_data {
                        need_data = false;
                        break 'outer;
                    } else {
                        need_data = true;
                    }
                }
                Err(msg) => {
                    failed = true;
                    if !result.starts_with("error") {
                        dbg!("error", msg);
                        assert!(res.is_ok(), "result should not error");
                    }

                    error_msg = msg.to_owned();
                    break 'outer;
                }
            }
            in_string = if in_string { false } else { true };
        }

        if !failed {
            let r = convert_result(&call);
            println!("Test OK result:{}", r);
            // dbg!(call);
        } else {
            if need_data {
                println!("Test error: expected more data");
            } else {
                println!("Test error:{}", error_msg);
            }
        }
        // check last error
    }

    fn convert_result(call: &ValueTreeBuilder) -> String {
        let mut res = String::new();

        for val in &call.values {
            res += &val.to_string();
        }

        return res;
    }

    #[test]
    fn test_frpc() {
        let res = test_file("tests/frpc.tests");
        assert!(res.is_ok());
    }

    //     #[test]
    //     fn test_frps() {
    //         let res = test_file("tests/frps.tests");
    //         assert!(res.is_ok());
    //     }
}
