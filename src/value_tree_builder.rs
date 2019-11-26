use crate::tokenizer::*;
use crate::serialize::Value;

pub struct ValueTreeBuilder {
    pub major_version: u8,
    pub minor_version: u8,
    
    pub was_response: bool,
    pub method_name: String,

    pub values: Vec<Value>,
    stack: Vec<Value>,
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
}

impl Callback for ValueTreeBuilder {

    /** Parsing always stop after this callback return. */
    fn error(&mut self) {

    }

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
        return true
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
        true
    }

    fn integer(&mut self, v:i64) -> bool {
        if self.stack.is_empty() {
            self.values.push(Value::Int(v));
            return true;
        }

        let last = self.stack.last_mut().unwrap();
        match last {
            Value::Array(arr) => arr.push(Value::Int(v)),
            _ => return false,
        }

        return true;
    }

    /* Stop on false, continue on true */
    fn boolean(&mut self) -> bool {
        true
    }

    fn double_number(&mut self) -> bool {
        true
    }

    fn datetime(&mut self) -> bool {
        true
    }

    fn binary(&mut self) -> bool {
        true
    }

    fn push_array(&mut self) -> bool {
        true
    }

    fn push_struct(&mut self) -> bool {
        true
    } // pushMap

    fn map_key(&mut self) -> bool {
        true
    }

    fn pop_context(&mut self) {
        
    }
}