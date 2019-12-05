use crate::serialize::{DateTimeVer30, Value};
use crate::tokenizer::*;
use crate::constants::*;
use std::collections::HashMap;
use std::str;

#[derive(Debug)]
enum Type {
    Array(Vec<Value>),
    Struct((String, HashMap<String, Value>)), // (key for new item to add, map)
    Str(String),
    Binary(Vec<u8>),
}

#[derive(Debug)]
pub struct ValueTreeBuilder {
    pub major_version: u8,
    pub minor_version: u8,
    pub was_response: bool,
    pub was_fault: bool,
    pub method_name: String,

    pub values: Vec<Value>, // result values
    stack: Vec<Type>,
}

impl ValueTreeBuilder {
    pub fn new() -> ValueTreeBuilder {
        ValueTreeBuilder {
            major_version: 0,
            minor_version: 0,
            was_response: false,
            was_fault: false,
            method_name: String::new(),
            values: vec![],
            stack: vec![],
        }
    }

    fn append_to_last(last: &mut Type, v: Value) {
        match last {
            Type::Array(arr) => {
                arr.push(v);
            }
            Type::Struct((key, strct)) => {
                strct.insert(std::mem::replace(key, String::new()), v);
            }
            _ => {
                unreachable!();
            }
        }
    }
}

impl Callback for ValueTreeBuilder {
    /** Parsing always stop after this callback return. */
    fn error(&mut self) {}

    /* Stop on false, continue on true */
    fn version(&mut self, major_version: u8, minor_version: u8) -> bool {
        self.major_version = major_version;
        self.minor_version = minor_version;
        return true;
    }

    /* Stop on false, continue on true */
    fn call(&mut self, method: &str, lenght: usize) -> bool {
        if self.method_name.capacity() < lenght {
            self.method_name.reserve(lenght);
        }

        self.method_name.push_str(method);
        return true;
    }

    /* Stop on false, continue on true */
    fn response(&mut self) -> bool {
        self.was_response = true;
        true
    }

    /* Stop on false, continue on true */
    fn fault(&mut self) -> bool {
        self.was_fault = true;
        return true;
    }

    /* Stop on false, continue on true */
    fn stream_data(&mut self) -> bool {
        true
    }

    /* Stop on false, continue on true */
    fn null(&mut self) -> bool {
        if let Some(last) = self.stack.last_mut() {
            ValueTreeBuilder::append_to_last(last, Value::Null);
            return true;
        }
        self.values.push(Value::Null);
        return true;
    }

    fn integer(&mut self, v: i64) -> bool {
        if let Some(last) = self.stack.last_mut() {
            ValueTreeBuilder::append_to_last(last, Value::Int(v));
            return true;
        }
        self.values.push(Value::Int(v));
        return true;
    }

    /* Stop on false, continue on true */
    fn boolean(&mut self, v: bool) -> bool {
        if let Some(last) = self.stack.last_mut() {
            ValueTreeBuilder::append_to_last(last, Value::Bool(v));
            return true;
        }
        self.values.push(Value::Bool(v));
        return true;
    }

    fn double_number(&mut self, v: f64) -> bool {
        if let Some(last) = self.stack.last_mut() {
            ValueTreeBuilder::append_to_last(last, Value::Double(v));
            return true;
        }
        self.values.push(Value::Double(v));
        return true;
    }

    fn datetime(&mut self, v: &DateTimeVer30) -> bool {
        if let Some(last) = self.stack.last_mut() {
            ValueTreeBuilder::append_to_last(last, Value::DateTime(*v));
            return true;
        }
        self.values.push(Value::DateTime(*v));
        return true;
    }

    fn string_begin(&mut self, len: usize) -> bool {
        if len > MAX_STR_LENGTH {
            return false;
        }
        let mut v = String::new();
        v.reserve(len);
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
                Type::Str(val) => {
                    val.insert_str(val.len(), str::from_utf8(&v).unwrap());
                }
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
        let empty_key = String::new();
        self.stack.push(Type::Struct((empty_key, h)));
        return true;
    }

    fn struct_key(&mut self, v: &[u8], _len: usize) -> bool {
        if let Some(last) = self.stack.last_mut() {
            match last {
                Type::Struct((key, _)) => {
                    key.insert_str(key.len(), str::from_utf8(&v).unwrap());
                }
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
                Type::Str(v) => Value::Str(v),
                Type::Binary(v) => Value::Binary(v),
            };

            // append to top
            if let Some(top) = self.stack.last_mut() {
                ValueTreeBuilder::append_to_last(top, v);
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
