use crate::constants::*;
use crate::tokenizer::*;
use crate::{DateTimeVer30, Value};
use std::collections::HashMap;
use std::{fmt, str};

/// Items are stored on stack during tokenizing. Strings are incomplete utf8
/// codepoints hence we have to use `vec<u8>` not `std::String`
#[derive(Debug)]
enum Type {
    Array(Vec<Value>),
    Struct((Vec<u8>, HashMap<String, Value>)), // (key for new item to add, map)
    Str(Vec<u8>),
    Binary(Vec<u8>),
}

#[derive(Debug)]
pub enum ParsedStatus {
    Init,
    Error(String),
    Response,
    Fault,
    MethodCall(String),
}

#[derive(Debug)]
pub struct ValueTreeBuilder {
    pub major_version: u8,
    pub minor_version: u8,

    /// What was parsed from
    pub what: ParsedStatus,

    /// result value according type what was parsed
    pub values: Vec<Value>,
    stack: Vec<Type>,

    // Frps streamed data
    pub data: Vec<u8>,
}

impl ValueTreeBuilder {
    pub fn new() -> ValueTreeBuilder {
        ValueTreeBuilder {
            major_version: 0,
            minor_version: 0,
            what: ParsedStatus::Init,
            values: vec![],
            stack: vec![],
            data: vec![],
        }
    }

    fn append_to_last(last: &mut Type, v: Value) -> bool {
        match last {
            Type::Array(arr) => {
                arr.push(v);
            }
            Type::Struct((key, strct)) => {
                // check utf8 character validity. We make move a copy of key
                // vectory for new key
                let new_key_valid_utf8 = String::from_utf8(key.to_vec());
                if new_key_valid_utf8.is_err() {
                    return false; // invalid utf8 string
                }
                strct.insert(new_key_valid_utf8.unwrap(), v);
                // prepare struct to acumulate next item, key is used as
                // accumulator
                key.clear();
            }
            _ => {
                unreachable!();
            }
        }
        return true;
    }
}

impl fmt::Display for ValueTreeBuilder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.what {
            ParsedStatus::Init => write!(f, "{}", "initialized"),
            ParsedStatus::Error(msg) => write!(f, "error({})", msg),
            ParsedStatus::Response => write!(
                f,
                "{}",
                &self
                    .values
                    .iter()
                    .map(|x| format!("{}", x))
                    .collect::<String>()
            ),
            ParsedStatus::MethodCall(name) => {
                let len = self.values.len();
                let mut cnt: usize = 0;

                write!(
                    f,
                    "{}({})",
                    name,
                    &self
                        .values
                        .iter()
                        .map(|x| {
                            cnt += 1;
                            if cnt < len {
                                format!("{}, ", x)
                            } else {
                                format!("{}", x)
                            }
                        })
                        .collect::<String>()
                )
            }
            ParsedStatus::Fault => write!(f, "fault({}, {})", &self.values[0], &self.values[1]),
        }
    }
}

impl Callback for ValueTreeBuilder {
    /// Parsing always stop after this callback return.
    fn error(&mut self, msg: &str) {
        self.what = ParsedStatus::Error(msg.to_owned())
    }

    /// Called when version is tokenized. Stop on false, continue on true
    fn version(&mut self, major_version: u8, minor_version: u8) -> bool {
        self.major_version = major_version;
        self.minor_version = minor_version;
        return true;
    }

    // Stop on false, continue on true
    fn call(&mut self, method: &str, lenght: usize) -> bool {
        // Method can be called multiple times
        match &mut self.what {
            ParsedStatus::MethodCall(name) => {
                if name.capacity() < lenght {
                    name.reserve(lenght);
                }
                name.push_str(method);
            }
            _ => {
                let name = method.to_owned();
                self.what = ParsedStatus::MethodCall(name);
            }
        }
        return true;
    }

    /// Stop on false, continue on true
    fn response(&mut self) -> bool {
        self.what = ParsedStatus::Response;
        true
    }

    /// Stop on false, continue on true
    fn fault(&mut self) -> bool {        
        self.what = ParsedStatus::Fault;
        // Fault can apppear in frps almost everywhere
        self.stack.clear();
        self.values.clear();
        return true;
    }

    /// Stop on false, continue on true
    fn stream_data(&mut self, v: &[u8]) -> bool {        
        self.data.extend_from_slice(v);
        true
    }

    /* Stop on false, continue on true */
    fn null(&mut self) -> bool {
        if let Some(last) = self.stack.last_mut() {
            return ValueTreeBuilder::append_to_last(last, Value::Null);
        }
        self.values.push(Value::Null);
        return true;
    }

    fn integer(&mut self, v: i64) -> bool {
        if let Some(last) = self.stack.last_mut() {
            return ValueTreeBuilder::append_to_last(last, Value::Int(v));
        }
        self.values.push(Value::Int(v));
        return true;
    }

    /* Stop on false, continue on true */
    fn boolean(&mut self, v: bool) -> bool {
        if let Some(last) = self.stack.last_mut() {
            return ValueTreeBuilder::append_to_last(last, Value::Bool(v));
        }
        self.values.push(Value::Bool(v));
        return true;
    }

    fn double_number(&mut self, v: f64) -> bool {
        if let Some(last) = self.stack.last_mut() {
            return ValueTreeBuilder::append_to_last(last, Value::Double(v));
        }
        self.values.push(Value::Double(v));
        return true;
    }

    fn datetime(&mut self, v: &DateTimeVer30) -> bool {
        if let Some(last) = self.stack.last_mut() {
            return ValueTreeBuilder::append_to_last(last, Value::DateTime(*v));
        }
        self.values.push(Value::DateTime(*v));
        return true;
    }

    fn string_begin(&mut self, len: usize) -> bool {
        if len > MAX_STR_LENGTH {
            return false;
        }
        let v = Vec::with_capacity(len);
        self.stack.push(Type::Str(v));
        return true;
    }

    fn string_data(&mut self, v: &[u8], _len: usize) -> bool {
        // empty string is valid too
        if v.len() == 0 {
            return true;
        }

        if let Some(last) = self.stack.last_mut() {
            match last {
                Type::Str(val) => val.extend_from_slice(v),
                _ => return false,
            }
            return true;
        }
        return false;
    }

    fn binary_begin(&mut self, len: usize) -> bool {
        if len > MAX_BIN_LENGTH {
            return false;
        }
        let mut v: Vec<u8> = vec![];
        v.reserve(len);
        self.stack.push(Type::Binary(v));
        return true;
    }

    fn binary_data(&mut self, v: &[u8], _len: usize) -> bool {
        // empty binary is valid too
        if v.len() == 0 {
            return true;
        }
        if let Some(last) = self.stack.last_mut() {
            match last {
                Type::Binary(val) => {
                    val.extend_from_slice(&v);
                }
                _ => return false,
            }
            return true;
        }
        return false;
    }

    fn array_begin(&mut self, len: usize) -> bool {
        if len > MAX_ARRAY_LENGTH {
            return false;
        }
        let mut v = vec![];
        v.reserve(len);
        self.stack.push(Type::Array(v));
        return true;
    }

    fn struct_begin(&mut self, len: usize) -> bool {
        if len > MAX_STRUCT_LENGTH {
            return false;
        }
        let mut h = HashMap::new();
        h.reserve(len);
        self.stack.push(Type::Struct((vec![], h)));
        return true;
    }

    fn struct_key(&mut self, v: &[u8], _len: usize) -> bool {
        if let Some(last) = self.stack.last_mut() {
            match last {
                Type::Struct((key, _)) => key.extend_from_slice(v),
                _ => return false,
            }
            return true;
        }
        return false;
    }

    fn value_end(&mut self) -> bool {
        if let Some(last) = self.stack.pop() {
            // construct value
            let v = match last {
                Type::Struct((_, v)) => Value::Struct(v),
                Type::Array(v) => Value::Array(v),
                Type::Str(v) => {
                    // let check utf8 charactes validity
                    let v = String::from_utf8(v);
                    if v.is_err() {
                        return false; // is not valid utf8 encoded
                    }
                    Value::Str(v.unwrap())
                }
                Type::Binary(v) => Value::Binary(v),
            };

            // append to top
            if let Some(top) = self.stack.last_mut() {
                return ValueTreeBuilder::append_to_last(top, v);
            } else {
                // when stack is empty we reach result value
                // it can be struct, array or single value
                self.values.push(v);
            }
            return true;
        }
        return false;
    }
}
