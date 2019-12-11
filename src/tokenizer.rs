use crate::constants::*;
use crate::DateTimeVer30;
use byteorder::{ByteOrder, LittleEndian};
use std::cmp;
use std::fmt::Debug;
use std::str;

#[derive(Debug)]
enum States {
    Init,
    MessageType,
    CallNameSize,
    Response,
    Fault,
    CallName { length: usize, processed: usize },
    Value,
    Pop,
    Integer1 { is_negative: bool, bytes_cnt: usize },
    Integer3 { bytes_cnt: usize },
    Double,
    ArrayInit { octects: usize },
    ArrayItems { len: usize },
    StrLen { octects: usize },
    StrData { length: usize, processed: usize },
    BinLen { octects: usize },
    BinData { length: usize, processed: usize },
    StructHead { octects: usize },
    StructItem { items: usize },
    StructKeyHead,
    StructKey { length: usize, processed: usize },
    DateTime,
    Finish,
}

/// Tokenizer calls methods in this trait when Token is found in input data
pub trait Callback {
    /** Parsing always stop after this callback return. */
    fn error(&mut self, msg: &str);

    /* Stop on false, continue on true */
    fn version(&mut self, major_version: u8, minor_version: u8) -> bool;

    /* Stop on false, continue on true */
    fn call(&mut self, method: &str, lenght: usize) -> bool;

    /* Stop on false, continue on true */
    fn response(&mut self) -> bool;

    /* Stop on false, continue on true */
    fn fault(&mut self) -> bool;

    /* Stop on false, continue on true */
    fn stream_data(&mut self, v: &[u8]) -> bool;

    /* Stop on false, continue on true */
    fn null(&mut self) -> bool;

    /* Stop on false, continue on true */
    fn integer(&mut self, v: i64) -> bool;
    fn boolean(&mut self, v: bool) -> bool;
    fn double_number(&mut self, v: f64) -> bool;
    fn datetime(&mut self, v: &DateTimeVer30) -> bool;
    /** Called when we reached begin of string with len */
    fn string_begin(&mut self, len: usize) -> bool;
    /* send data chunk 'v' with size smaller or equal of total lenght in 'len'*/
    fn string_data(&mut self, v: &[u8], len: usize) -> bool;

    /** Called when we reached begin of binary data with len */
    fn binary_begin(&mut self, len: usize) -> bool;
    /* send data chunk 'v' with size smaller or equal of total lenght in 'len'*/
    fn binary_data(&mut self, v: &[u8], len: usize) -> bool;

    fn array_begin(&mut self, len: usize) -> bool;
    fn struct_begin(&mut self, len: usize) -> bool; // pushMap
    fn struct_key(&mut self, v: &[u8], len: usize) -> bool;

    /** Called when reached end of string, binary, array or struct */
    fn value_end(&mut self) -> bool;
}

#[derive(Debug)]
struct SourcePtr<'a> {
    pos: usize,
    src: &'a [u8],
}

impl<'a> SourcePtr<'a> {
    fn new(src: &'a [u8]) -> SourcePtr<'a> {
        SourcePtr { pos: 0, src: src }
    }

    /// return size of unconsumed data
    fn available(&self) -> usize {
        return self.src.len() - self.pos;
    }

    /// return slice for unconsumed part of data with `cnt` lenght
    fn data(&self, cnt: usize) -> &[u8] {
        assert!(self.pos + cnt <= self.src.len());
        return &self.src[self.pos..self.pos + cnt];
    }

    /// move ptr for unconsumed part of data
    fn advance(&mut self, cnt: usize) {
        self.pos += cnt;
    }

    /// return number of bytes consumed.
    fn consumed(&self) -> usize {
        //assert_eq!(self.pos, self.src.len());
        return self.pos;
    }

    fn is_all_consumed(&self) -> bool {
        return self.pos == self.src.len();
    }
}

#[derive(Debug)]
struct Buffer {
    cnt: usize,
    data: [u8; 17], // 17 is size of DateTimeFormat3 which is maximal type
}

impl Buffer {
    fn new() -> Buffer {
        Buffer {
            cnt: 0,
            data: [0; 17],
        }
    }

    // prepare buffer for data
    fn reset(&mut self) {
        self.cnt = 0;
    }

    // Try to read `need` bytes from `src` and update src
    // return true when enough data was read, false otherwise
    fn consume(&mut self, need: usize, src: &mut SourcePtr) -> bool {
        assert!(need >= self.cnt);
        assert!(need <= self.data.len());

        let cnt = cmp::min(need - self.cnt, src.available());
        if cnt > 0 {
            // make slices same size for copy_from_slice
            let d = &mut self.data[self.cnt..self.cnt + cnt];
            d.copy_from_slice(src.data(cnt));
            src.advance(cnt);
            self.cnt += cnt;
        }

        // do we have all data ?
        return need == self.cnt;
    }
}

enum Context {
    Init,
    Response,
    Call { args: usize },
    Fault { args: usize },
    Data { len: usize },
}

pub struct Tokenizer {
    // Here we store state for recursive values as array and structs
    stack: Vec<States>,
    buffer: Buffer,

    version_major: u8,
    version_minor: u8,
    /// When `true` tokenizer is ready to accept methods arguments (which are optional)
    context: Context,
}

impl Tokenizer {
    pub fn new() -> Tokenizer {
        Tokenizer {
            stack: vec![States::Init],
            buffer: Buffer::new(),

            version_major: 0,
            version_minor: 0,
            context: Context::Init,
        }
    }

    fn arg_context(&self) -> bool {
        match &self.context {
            Context::Fault { args: arg } => *arg < (3 as usize),
            Context::Call { args: _ } => false,
            _ => true,
        }
    }

    /// Function tokenize `src` and call `cb` for storing Tokens.
    ///
    /// Return Ok (`true` if more data are expected and how many `bytes` was processed) or error description
    pub fn parse<T: Callback + Debug>(
        &mut self,
        src: &[u8],
        cb: &mut T,
    ) -> Result<(bool, usize), usize> {
        let mut src = SourcePtr::new(src);

        while let Some(state) = self.stack.last_mut() {
            // dbg!(&state);
            match state {
                States::Init => {
                    // first 4 bytes is header with magic and version
                    if !self.buffer.consume(4, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    // check FRPC magic and version
                    if self.buffer.data[0] != 0xca || self.buffer.data[1] != 0x11 {
                        // dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("Invalid magic expected 0xCA11");
                        return Err(src.pos);
                    }

                    self.version_major = self.buffer.data[2];
                    self.version_minor = self.buffer.data[3];

                    // We support versions: 3.0, 2.1, 2.0, 1.0
                    if !(((self.version_major == 3) && (self.version_minor == 0))
                        || ((self.version_major == 2) && (self.version_minor == 1))
                        || ((self.version_major == 2) && (self.version_minor == 0))
                        || ((self.version_major == 1) && (self.version_minor == 0)))
                    {
                        // dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("bad protocol version");
                        return Err(src.pos);
                    }

                    if !cb.version(self.version_major, self.version_minor) {
                        // dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::version invalid version");
                        return Err(src.pos);
                    }

                    *state = States::MessageType;
                    self.buffer.reset();
                }

                States::Pop => {
                    self.buffer.reset();
                    self.stack.pop();

                    // If we started process method arguments try to read Value
                    //  again  when stack is empty
                    // Fault put to 2 values to stack so we dont have to care
                    if self.stack.is_empty() {
                        match self.context {
                            Context::Call { args: _ } => self.stack.push(States::Value),
                            _ => {}
                        }
                    }
                }

                States::MessageType => {
                    // first byte is message type
                    if !self.buffer.consume(1, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    match self.buffer.data[0] & TYPE_MASK {
                        CALL_ID => {
                            *state = States::CallNameSize;
                            self.context = Context::Call { args: 0 };
                        }
                        RESPOSE_ID => {
                            *state = States::Response;
                            self.context = Context::Response;
                        }
                        FAULT_RESPOSE_ID => {
                            *state = States::Fault;
                        }
                        _ => {
                            // dbg!(&src.pos, &src.src[src.pos..], &cb);
                            cb.error("unknown type");
                            return Err(src.pos);
                        }
                    }

                    self.buffer.reset();
                }

                States::CallNameSize => {
                    // first byte is method name lenght
                    if !self.buffer.consume(1, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    let length: usize = self.buffer.data[0] as usize;
                    if length == 0 {
                        //dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("bad call name");
                        return Err(src.pos);
                    }

                    *state = States::CallName {
                        length: length,
                        processed: 0,
                    };
                    self.buffer.reset();
                }

                States::CallName { length, processed } => {
                    // read method name
                    let avail = cmp::min(*length - *processed, src.available());
                    if avail == 0 {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    let run = cb.call(str::from_utf8(src.data(avail)).unwrap(), *length);

                    *processed += avail;
                    src.advance(avail);

                    if !run || *length != *processed {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    // since parameters are optional we suppose there is no parameters
                    *state = States::Pop;
                    self.buffer.reset();
                }

                States::Value => {
                    // first byte is value type
                    if !self.buffer.consume(1, &mut src) {
                        assert!(src.is_all_consumed());
                        // when we processing method arguments we dont need data
                        return Ok((self.arg_context(), src.consumed()));
                    }

                    match self.buffer.data[0] & TYPE_MASK {
                        VINT_ID | U_VINT_ID => {
                            // get used octects
                            let mut octects = (self.buffer.data[0] & OCTET_CNT_MASK) as usize;

                            if self.version_major == 1 {
                                cb.error("invalid type");
                                return Err(src.pos);
                            }
                            octects += 1;
                            // negative number
                            let is_negative = (self.buffer.data[0] & VINT_ID) != 0;
                            *state = States::Integer1 {
                                is_negative,
                                bytes_cnt: octects,
                            };
                        }
                        INT_ID => {
                            // get used octects
                            let mut octects = (self.buffer.data[0] & OCTET_CNT_MASK) as usize;

                            if self.version_major == 3 {
                                octects += 1;
                            }

                            if self.version_major == 3 {
                                *state = States::Integer3 { bytes_cnt: octects };
                            } else {
                                // negative number
                                *state = States::Integer1 {
                                    is_negative: false,
                                    bytes_cnt: octects,
                                };
                            }
                        }
                        STRING_ID => {
                            // get used octects
                            let octects = (self.buffer.data[0] & OCTET_CNT_MASK) as usize;
                            *state = States::StrLen { octects };
                        }
                        BIN_ID => {
                            // get used octects
                            let octects = (self.buffer.data[0] & OCTET_CNT_MASK) as usize;
                            *state = States::BinLen { octects };
                        }
                        STRUCT_ID => {
                            // get used octects
                            let octects = (self.buffer.data[0] & OCTET_CNT_MASK) as usize;
                            *state = States::StructHead { octects };
                        }
                        ARRAY_ID => {
                            // get array len used octects
                            let octects = (self.buffer.data[0] & OCTET_CNT_MASK) as usize;
                            *state = States::ArrayInit { octects };
                        }
                        NULL_ID => {
                            if self.version_major == 1 {
                                cb.error("unknown type");
                                return Err(src.pos);
                            }

                            // octects bits should be zero
                            if (self.buffer.data[0] & OCTET_CNT_MASK) as usize != 0 {
                                cb.error("invalid value");
                                return Err(src.pos);
                            }

                            let run = cb.null();
                            if !run {
                                // dbg!(src.pos, &src.src[src.pos..], cb);
                                cb.error("cb::null in Value failed");
                                return Err(src.pos);
                            }
                            *state = States::Pop;
                        }
                        BOOL_ID => {
                            let v = (self.buffer.data[0] & OCTET_CNT_MASK) != 0;
                            let run = cb.boolean(v);
                            if !run {
                                // dbg!(src.pos, &src.src[src.pos..], cb);
                                cb.error("cb::boolean in Value failed");
                                return Err(src.pos);
                            }
                            *state = States::Pop;
                        }
                        DOUBLE_ID => {
                            *state = States::Double;
                        }
                        DATETIME_ID => {
                            *state = States::DateTime;
                        }
                        _ => {
                            // dbg!(src.pos, &src.src[src.pos..], &self.buffer, cb);
                            cb.error("unknown type");
                            return Err(src.pos);
                        }
                    }

                    self.buffer.reset();

                    // Check if we are processing arguments for call and
                    // in case of fault check that values are of correct
                    // type and number
                    match &mut self.context {
                        Context::Call { args: arg } => *arg += 1,
                        Context::Fault { args: arg } => {
                            // fault can have only 2 params:
                            //  1st - Int
                            //  2nd - String
                            //  more or less arguments is error
                            if *arg == 0 {
                                match *state {
                                    States::Integer1 {
                                        is_negative: _,
                                        bytes_cnt: _,
                                    }
                                    | States::Integer3 { bytes_cnt: _ } => {}
                                    _ => {
                                        cb.error("invalid fault");
                                        return Err(src.pos);
                                    }
                                }
                            } else if *arg == 1 {
                                match *state {
                                    States::StrLen { octects: _ } => {}
                                    _ => {
                                        cb.error("invalid fault");
                                        return Err(src.pos);
                                    }
                                }
                            } else if *arg == 2 {
                                cb.error("invalid fault");
                                return Err(src.pos);
                            }
                            *arg += 1;
                        }
                        _ => {}
                    }
                }

                // Protocol version 3.0 zigzack
                States::Integer3 { bytes_cnt } => {
                    if !self.buffer.consume(*bytes_cnt, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    let run = cb.integer(zigzag_decode(&self.buffer.data[0..*bytes_cnt]));
                    if !run {
                        //dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::integer in Integer3 failed");
                        return Err(src.pos);
                    }
                    *state = States::Pop;
                }

                States::Integer1 {
                    is_negative,
                    bytes_cnt,
                } => {
                    if !self.buffer.consume(*bytes_cnt, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    let mut v = read_i64(&self.buffer.data[0..*bytes_cnt]);
                    if *is_negative {
                        v *= -1;
                    }

                    let run = cb.integer(v);
                    if !run {
                        //dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::integer in Integer1 failed");
                        return Err(src.pos);
                    }
                    *state = States::Pop;
                }
                // String
                States::StrLen { octects } => {
                    let bytes_cnt = if self.version_major != 1 {
                        *octects + 1
                    } else {
                        if (self.version_major == 1) && (*octects == 0) {
                            cb.error("bad size");
                            return Err(src.pos);
                        }
                        if *octects > 4 {
                            cb.error("String len is greater than 4 bytes");
                            return Err(src.pos);
                        }

                        *octects
                    };

                    // read array len
                    if !self.buffer.consume(bytes_cnt, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    let cnt = read_i64(&self.buffer.data[0..bytes_cnt]) as usize;

                    if cnt > MAX_STR_LENGTH {
                        cb.error("too large string");
                        return Err(src.pos);
                    }

                    let run = cb.string_begin(cnt);
                    if !run {
                        // dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::string_begin in StrLen failed");
                        return Err(src.pos);
                    }

                    *state = States::StrData {
                        length: cnt,
                        processed: 0,
                    };
                    self.buffer.reset();
                }

                States::StrData { length, processed } => {
                    // if *length == 0 {
                    //     return Err("Invalid string length");
                    // }
                    assert!(*processed <= *length, "invalid state");
                    // Do we have any string data? and string is not empty
                    if (*processed != *length) && src.is_all_consumed() {
                        return Ok((true, src.consumed()));
                    }

                    // Process available or missing part
                    let cnt = cmp::min(src.available(), *length - *processed);
                    let run = cb.string_data(src.data(cnt), *length);
                    if !run {
                        //dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::string_data in StrData failed");
                        return Err(src.pos);
                    }

                    // update processed data
                    src.advance(cnt);
                    *processed += cnt;

                    // did we process all string data?
                    if processed != length {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    // string is completed
                    let run = cb.value_end();
                    if !run {
                        //dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::value_end in StrData failed");
                        return Err(src.pos);
                    }

                    *state = States::Pop;
                }
                // Binary
                States::BinLen { octects } => {
                    let bytes_cnt = if self.version_major != 1 {
                        *octects + 1
                    } else {
                        if (self.version_major == 1) && (*octects == 0) {
                            cb.error("bad size");
                            return Err(src.pos);
                        }

                        if *octects > 4 {
                            cb.error("Binary len is greater than 4 bytes");
                            return Err(src.pos);
                        }

                        *octects
                    };

                    // read array len
                    if !self.buffer.consume(bytes_cnt, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    let cnt = read_i64(&self.buffer.data[0..bytes_cnt]) as usize;

                    if cnt > MAX_BIN_LENGTH {
                        cb.error("too large binary data");
                        return Err(src.pos);
                    }

                    let run = cb.binary_begin(cnt);
                    if !run {
                        //dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::binary_begin in BinLen failed");
                        return Err(src.pos);
                    }

                    *state = States::BinData {
                        length: cnt,
                        processed: 0,
                    };
                    self.buffer.reset();
                }

                States::BinData { length, processed } => {
                    assert!(*processed <= *length, "invalid state");
                    // Do we have any binary data and is not 0 lenght?
                    if (*processed != *length) && src.is_all_consumed() {
                        return Ok((true, src.consumed()));
                    }

                    // Process available or missing part
                    let cnt = cmp::min(src.available(), *length - *processed);
                    let run = cb.binary_data(src.data(cnt), *length);
                    if !run {
                        //dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::binary_data in BinData failed");
                        return Err(src.pos);
                    }

                    // update processed data
                    src.advance(cnt);
                    *processed += cnt;

                    // did we process all binary data?
                    if processed != length {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    // binary is completed
                    let run = cb.value_end();
                    if !run {
                        //dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::value_end in BinData failed");
                        return Err(src.pos);
                    }

                    *state = States::Pop;
                }
                // Array
                States::ArrayInit { octects } => {
                    let bytes_cnt = if self.version_major != 1 {
                        *octects + 1
                    } else {
                        if *octects > 4 {
                            cb.error("Array len is greater than 4 bytes");
                            return Err(src.pos);
                        }

                        *octects
                    };

                    // read array len
                    if !self.buffer.consume(bytes_cnt, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    let cnt = read_i64(&self.buffer.data[0..bytes_cnt]) as usize;

                    if cnt > MAX_ARRAY_LENGTH {
                        cb.error("too large array");
                        return Err(src.pos);
                    }

                    let run = cb.array_begin(cnt);
                    if !run {
                        // dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::array_begin in ArrayInit failed");
                        return Err(src.pos);
                    }

                    *state = States::ArrayItems { len: cnt };
                    self.buffer.reset();
                }

                States::ArrayItems { len } => {
                    if *len > 0 {
                        *len -= 1;
                        self.stack.push(States::Value);
                    } else {
                        let run = cb.value_end();
                        if !run {
                            // dbg!(src.pos, &src.src[src.pos..], cb);
                            cb.error("cb::value_end in ArrayItem failed");
                            return Err(src.pos);
                        }
                        *state = States::Pop;
                    }
                }
                // Struct
                States::StructHead { octects } => {
                    let bytes_cnt = if self.version_major != 1 {
                        *octects + 1
                    } else {
                        if *octects > 4 {
                            cb.error("Struct len is greater than 4 bytes");
                            return Err(src.pos);
                        }

                        *octects
                    };

                    // read struct len
                    if !self.buffer.consume(bytes_cnt, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    let items = read_i64(&self.buffer.data[0..bytes_cnt]) as usize;

                    let run = cb.struct_begin(items);
                    if !run {
                        // dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::struct_begin in StructHead failed");
                        return Err(src.pos);
                    }

                    *state = States::StructItem { items };
                    self.buffer.reset();
                }

                States::StructItem { items } => {
                    if *items > 0 {
                        *items -= 1;
                        self.stack.push(States::Value);
                        self.stack.push(States::StructKeyHead);
                    } else {
                        let run = cb.value_end();
                        if !run {
                            // dbg!(src.pos, &src.src[src.pos..], cb);
                            cb.error("cb::value_end in StructItem failed");
                            return Err(src.pos);
                        }
                        *state = States::Pop;
                    }
                }

                States::StructKeyHead => {
                    if !self.buffer.consume(1, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    let len = self.buffer.data[0] as usize;
                    if len == 0 {
                        cb.error("bad key length");
                        return Err(src.pos);
                    }

                    if len > 255 {}

                    *state = States::StructKey {
                        length: len,
                        processed: 0,
                    };
                    self.buffer.reset();
                }

                States::StructKey { length, processed } => {
                    assert!(*processed < *length, "invalid state");
                    // Do we have any binary data?
                    if src.is_all_consumed() {
                        return Ok((true, src.consumed()));
                    }

                    // Process available or missing part
                    let cnt = cmp::min(src.available(), *length - *processed);
                    let run = cb.struct_key(src.data(cnt), *length);
                    if !run {
                        //dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::struct_key in StructKey failed");
                        return Err(src.pos);
                    }

                    // update processed data
                    src.advance(cnt);
                    *processed += cnt;

                    // did we process all binary data?
                    if processed != length {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    *state = States::Pop;
                }

                States::Response => {
                    let run = cb.response();
                    if !run {
                        //dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::response in Response failed");
                        return Err(src.pos);
                    }

                    *state = States::Finish;
                    self.stack.push(States::Value); // Value
                }

                States::Fault => {
                    let run = cb.fault();
                    if !run {
                        //dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::fault in Fault failed");
                        return Err(src.pos);
                    }

                    *state = States::Finish;
                    self.context = Context::Fault { args: 0 };
                    self.stack.push(States::Value); // Message
                    self.stack.push(States::Value); // status code
                }

                States::Double => {
                    if !self.buffer.consume(8, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    let v = LittleEndian::read_f64(&self.buffer.data[0..8]);
                    let run = cb.double_number(v);
                    if !run {
                        //dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::double_number failed");
                        return Err(src.pos);
                    }
                    *state = States::Pop;
                }

                States::DateTime => {
                    let bytes: usize = if self.version_major == 3 { 14 } else { 10 };

                    if !self.buffer.consume(bytes, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    let val = if self.version_major == 3 {
                        // struct DateTimeFormat3_t {
                        //     uint8_t timeZone : 8;
                        //     uint64_t unixTime : 64;
                        //     uint8_t weekDay : 3;
                        //     uint8_t sec : 6;
                        //     uint8_t minute : 6;
                        //     uint8_t hour : 5;
                        //     uint8_t day : 5;
                        //     uint8_t month : 4;
                        //     uint16_t year : 11;
                        // } __attribute__((packed));
                        let time_zone = (self.buffer.data[0] as i16) * 15 * 60;
                        let unix_time = LittleEndian::read_u64(&self.buffer.data[1..]);
                        let week_day: u8 = self.buffer.data[9] & 0x07;
                        let sec: u8 = ((self.buffer.data[9] & 0xf8) >> 3)
                            | ((self.buffer.data[11] & 0x01) << 5);
                        let min: u8 = (self.buffer.data[10] & 0x7e) >> 1;
                        let hour: u8 = ((self.buffer.data[10] & 0x80) >> 7)
                            | ((self.buffer.data[12] & 0x0f) << 1);
                        let day: u8 = ((self.buffer.data[11] & 0xf0) >> 4)
                            | ((self.buffer.data[12] & 0x01) << 4);
                        let month: u8 = (self.buffer.data[12] & 0x1e) >> 1;
                        let year = (((self.buffer.data[12] as u16) & 0xe0) >> 5)
                            | ((self.buffer.data[13] as u16) << 3) + 1600;

                        DateTimeVer30 {
                            time_zone,
                            unix_time,
                            week_day,
                            sec,
                            min,
                            hour,
                            day,
                            month,
                            year,
                        }
                    } else {
                        // Verion 2.1 or 1.0

                        // struct DateTimeFormat1_t {
                        //     uint8_t timeZone : 8;
                        //     uint32_t unixTime : 32;
                        //     uint8_t weekDay : 3;
                        //     uint8_t sec : 6;
                        //     uint8_t minute : 6;
                        //     uint8_t hour : 5;
                        //     uint8_t day : 5;
                        //     uint8_t month : 4;
                        //     uint16_t year : 11;
                        // } __attribute__((packed));

                        let time_zone = (self.buffer.data[0] as i16) * 15 * 60;
                        let unix_time = LittleEndian::read_u32(&self.buffer.data[1..]) as u64;
                        let week_day = self.buffer.data[5] & 0x07;
                        let sec = ((self.buffer.data[5] & 0xf8) >> 3)
                            | ((self.buffer.data[6] & 0x01) << 5);
                        let min = (self.buffer.data[6] & 0x7e) >> 1;
                        let hour = ((self.buffer.data[6] & 0x80) >> 7)
                            | ((self.buffer.data[7] & 0x0f) << 1);
                        let day = ((self.buffer.data[7] & 0xf0) >> 4)
                            | ((self.buffer.data[9] & 0x01) << 4);
                        let month = (self.buffer.data[8] & 0x1e) >> 1;
                        let year = (((self.buffer.data[8] as u16) & 0xe0) >> 5)
                            | ((self.buffer.data[9] as u16) << 3) + 1600;

                        DateTimeVer30 {
                            time_zone,
                            unix_time,
                            week_day,
                            sec,
                            min,
                            hour,
                            day,
                            month,
                            year,
                        }
                    };

                    let run = cb.datetime(&val);
                    if !run {
                        //dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::datetime in Datetime failed");
                        return Err(src.pos);
                    }

                    *state = States::Pop;
                }

                States::Finish => {
                    // Detect calling tokenizer after it returned not needed data
                    // and there are unexpected data in source stream
                    if !src.is_all_consumed() {
                        cb.error("data after end");
                        return Err(src.pos);
                    }

                    // Don't pop stack, keep finish state to detect unexpected data
                    return Ok((false, src.consumed()));
                }
            }
        }

        // if !src.is_all_consumed() {
        //     dbg!(src.pos, &src.src[src.pos..], cb);
        //     assert!(src.is_all_consumed());
        // }
        return Ok((false, src.consumed()));
    }
}

fn read_i64(s: &[u8]) -> i64 {
    let mut tmp: [u8; 8] = [0; 8];

    let cnt = cmp::min(tmp.len(), s.len());
    if cnt > 0 {
        // make slices same size for copy_from_slice
        let d = &mut tmp[0..cnt];
        d.copy_from_slice(s);
    }
    return i64::from_le_bytes(tmp);
}

/** Dencodes signed integer from unsigned,
 * with positive values even and negative values odd
 * starting around zero.
 * This saves transfer space and unifies integer encoding.
 * 0 -> 0
 * 1 -> -1
 * 2 -> 1
 * 3 -> -2
 * 4 -> 2
 * ...
 */
fn zigzag_decode(s: &[u8]) -> i64 {
    let n = read_i64(s);
    return (n >> 1) ^ (-(n & 1));
}
