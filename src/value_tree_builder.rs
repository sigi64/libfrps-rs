use crate::constants::*;
use crate::tokenizer::*;
use crate::{DateTimeVer30, Value};
use std::collections::HashMap;
use std::{fmt, str};

#[derive(Debug)]
enum Type {
    Array(Vec<Value>),
    Struct((String, HashMap<String, Value>)), // (key for new item to add, map)
    Str(String),
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

impl fmt::Display for ValueTreeBuilder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.what {
            ParsedStatus::Init => {
                write!(f, "{}", "initialized");
            }
            ParsedStatus::Error(msg) => {
                write!(f, "error({})", msg);
            }
            ParsedStatus::Response => {
                write!(f, "response(");
                for v in &self.values {
                    write!(f, "{}", v);
                }
                write!(f, ")");
            }
            ParsedStatus::MethodCall(name) => {
                write!(f, "method {}(", name);
                for v in &self.values {
                    write!(f, "{}", v);
                }
                write!(f, ")");
            }
            ParsedStatus::Fault => {
                write!(f, "fault(");
                for v in &self.values {
                    write!(f, "{}", v);
                }
                write!(f, ")");
            }            
        }
        return Ok(());
    }
}

impl Callback for ValueTreeBuilder {
    /** Parsing always stop after this callback return. */
    fn error(&mut self, msg: &str) {
        self.what = ParsedStatus::Error(msg.to_owned())
    }

    /* Stop on false, continue on true */
    fn version(&mut self, major_version: u8, minor_version: u8) -> bool {
        self.major_version = major_version;
        self.minor_version = minor_version;
        return true;
    }

    /* Stop on false, continue on true */
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

    /* Stop on false, continue on true */
    fn response(&mut self) -> bool {
        self.what = ParsedStatus::Response;
        true
    }

    /* Stop on false, continue on true */
    fn fault(&mut self) -> bool {
        self.what = ParsedStatus::Fault;
        return true;
    }

    /* Stop on false, continue on true */
    fn stream_data(&mut self, v: &[u8]) -> bool {
        //self.data.push(v);
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
