use crate::serialize::Value;
use crate::tokenizer::*;
use std::collections::HashMap;

enum Type {
    Array(Vec<Value>),
    Struct((String, HashMap<String, Value>)), // (key to add)
}

pub struct ValueTreeBuilder {
    pub major_version: u8,
    pub minor_version: u8,
    pub was_response: bool,
    pub method_name: String,

    pub values: Vec<Value>,
    stack: Vec<Type>,
}

impl ValueTreeBuilder {
    pub fn new() -> ValueTreeBuilder {
        ValueTreeBuilder {
            major_version: 0,
            minor_version: 0,
            was_response: false,
            method_name: String::from(""),
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
                strct.insert(std::mem::replace(key, String::from("")), v);
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
    fn call(&mut self, method: &str, avail: usize, lenght: usize) -> bool {
        if self.method_name.capacity() < lenght {
            self.method_name.reserve(lenght);
        }

        self.method_name.push_str(method);
        return true;
    }

    /* Stop on false, continue on true */
    fn response(&mut self) -> bool {
        true
    }

    /* Stop on false, continue on true */
    fn fault(&mut self) -> bool {
        true
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
        return false;
    }

    fn integer(&mut self, v: i64) -> bool {
        if let Some(last) = self.stack.last_mut() {
            ValueTreeBuilder::append_to_last(last, Value::Int(v));
            return true;
        }
        return false;
    }

    /* Stop on false, continue on true */
    fn boolean(&mut self, v: bool) -> bool {
        if let Some(last) = self.stack.last_mut() {
            ValueTreeBuilder::append_to_last(last, Value::Bool(v));
            return true;
        }
        return false;
    }

    fn double_number(&mut self, v: f64) -> bool {
        if let Some(last) = self.stack.last_mut() {
            ValueTreeBuilder::append_to_last(last, Value::Double(v));
            return true;
        }
        return false;
    }

    fn datetime(&mut self) -> bool {
        true
    }

    fn binary(&mut self, v: &[u8]) -> bool {
        true
    }

    fn push_array(&mut self, len: usize) -> bool {
        let mut v = vec![];
        v.reserve(len);
        self.stack.push(Type::Array(v));
        return true;
    }

    fn push_struct(&mut self, len: usize) -> bool {
        let mut h = HashMap::new();
        h.reserve(len);
        let empty_key = String::from("");
        self.stack.push(Type::Struct((empty_key, h)));
        return true;
    }

    fn map_key(&mut self, key: &mut String) -> bool {
        if let Some(last) = self.stack.last_mut() {
            match last {
                Type::Struct((k, _)) => {
                    std::mem::swap(key, k);
                }
                _ => return false,
            }
            return true;
        }
        return false;
    }

    fn pop_value(&mut self) -> bool {
        if let Some(last) = self.stack.pop() {
            match last {
                Type::Struct((_, v)) => self.values.push(Value::Struct(v)),
                Type::Array(v) => self.values.push(Value::Array(v)),
            }
            return true;
        }
        return false;
    }
}
