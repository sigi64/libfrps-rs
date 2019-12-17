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
    ValueInt,
    ValueString,
    Pop,
    IntegerHead { head: u8 },
    Integer1 { is_negative: bool, bytes_cnt: usize },
    Integer3 { bytes_cnt: usize },
    Double,
    ArrayInit { octects: usize },
    ArrayItems { len: usize },
    StrHead { head: u8 },
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
    DataInit,
    DataLen { octects: usize },
    Data { length: usize, processed: usize },
}

/// Tokenizer calls methods in this trait when Token is found in input data
pub trait Callback {
    /** Parsing always stop after this callback return. */
    fn error(&mut self, msg: &str);

    /* Stop on false, continue on true */
    fn version(&mut self, major_version: u8, minor_version: u8) -> bool;

    /* Stop on false, continue on true */
    fn call(&mut self, method: &str, length: usize) -> bool;

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
    /* send data chunk 'v' with size smaller or equal of total length in 'len'*/
    fn string_data(&mut self, v: &[u8], len: usize) -> bool;

    /** Called when we reached begin of binary data with len */
    fn binary_begin(&mut self, len: usize) -> bool;
    /* send data chunk 'v' with size smaller or equal of total length in 'len'*/
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

    /// return slice for unconsumed part of data with `cnt` length
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
    Data,
}

pub struct Tokenizer {
    // Here we store state for recursive values as array and structs
    stack: Vec<States>,
    buffer: Buffer,

    version_major: u8,
    version_minor: u8,
    /// When `true` tokenizer is ready to accept methods arguments (which are optional)
    context: Context,
    is_frps: bool,
}

impl Tokenizer {
    pub fn new_frpc() -> Tokenizer {
        Tokenizer {
            stack: vec![States::Init],
            buffer: Buffer::new(),

            version_major: 0,
            version_minor: 0,
            context: Context::Init,
            is_frps: false,
        }
    }

    pub fn new_frps() -> Tokenizer {
        Tokenizer {
            stack: vec![States::Init],
            buffer: Buffer::new(),

            version_major: 0,
            version_minor: 0,
            context: Context::Init,
            is_frps: true,
        }
    }

    fn need_data(&self) -> bool {
        match &self.context {
            Context::Fault { args: arg } => *arg < (3 as usize),
            Context::Data | Context::Call { args: _ } => false,
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
            //dbg!(&state);
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
                    // first byte is method name length
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

                    // since parameters and data are optional we suppose there is no parameters or data
                    *state = States::Pop;
                    self.buffer.reset();
                }

                States::Response => {
                    let run = cb.response();
                    if !run {
                        //dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::response in Response failed");
                        return Err(src.pos);
                    }

                    // data or value follows. In FRPS data can be interleaved
                    // with values:: E.G. RS {... DATA .. VAl .. DATA ... VAl }
                    *state = States::Value;
                    if self.is_frps {
                        *state = States::DataInit;
                        self.stack.push(States::Value);                      
                    }                    
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
                    self.stack.push(States::ValueString); // message string
                    self.stack.push(States::ValueInt); // status code
                }

                States::Value => {
                    // first byte is value type
                    if !self.buffer.consume(1, &mut src) {
                        assert!(src.is_all_consumed());
                        // when we processing method arguments we dont need data
                        return Ok((self.need_data(), src.consumed()));
                    }

                    match self.buffer.data[0] & TYPE_MASK {
                        VINT_ID | U_VINT_ID | INT_ID => {
                            *state = States::IntegerHead {
                                head: self.buffer.data[0],
                            };
                        }
                        STRING_ID => {
                            *state = States::StrHead {
                                head: self.buffer.data[0],
                            };
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
                            let octects = self.buffer.data[0] & OCTET_CNT_MASK;
                            let v = match octects {
                                0 => false,
                                1 => true,
                                _ => {
                                    cb.error("invalid bool value");
                                    return Err(src.pos);
                                }
                            };
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
                        FRPS_DATA => {
                            if !self.is_frps {
                                cb.error("unknown type");
                                return Err(src.pos);
                            }

                            let octects: usize = match self.buffer.data[0] & OCTET_CNT_MASK {
                                0 => 0,
                                1 => 2,
                                2 => 4,
                                4 => 8,
                                _ => {
                                    cb.error("invalid type");
                                    return Err(src.pos);
                                }
                            };

                            *state = States::DataLen { octects };
                        }
                        FAULT_RESPOSE_ID => {
                            if !self.is_frps {
                                cb.error("unknown type");
                                return Err(src.pos);
                            }

                            *state = States::Fault;
                        }
                        _ => {
                            // dbg!(src.pos, &src.src[src.pos..], &self.buffer, cb);
                            cb.error("unknown type");
                            return Err(src.pos);
                        }
                    }

                    self.buffer.reset();

                    // Check if we are processing arguments for call and
                    match &mut self.context {
                        Context::Call { args: arg } => *arg += 1,
                        _ => {}
                    }
                }

                // Extract Int into separate state for Fault to be able specify
                // exact type of value expected next in data
                States::ValueInt => {
                    // first byte is value type
                    if !self.buffer.consume(1, &mut src) {
                        assert!(src.is_all_consumed());
                        // when we processing method arguments we dont need data
                        return Ok((true, src.consumed()));
                    }

                    *state = States::IntegerHead {
                        head: self.buffer.data[0],
                    };
                    self.buffer.reset();
                }

                States::IntegerHead { head } => {
                    match *head & TYPE_MASK {
                        VINT_ID | U_VINT_ID => {
                            // get used octects
                            let mut octects = (*head & OCTET_CNT_MASK) as usize;

                            if self.version_major == 1 {
                                cb.error("invalid type");
                                return Err(src.pos);
                            }
                            octects += 1;
                            // negative number
                            let is_negative = (*head & VINT_ID) != 0;
                            *state = States::Integer1 {
                                is_negative,
                                bytes_cnt: octects,
                            };
                        }
                        INT_ID => {
                            // get used octects
                            let mut octects = (*head & OCTET_CNT_MASK) as usize;

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
                        _ => {
                            cb.error("invalid type id");
                            return Err(src.pos);
                        }
                    }

                    self.buffer.reset();
                }

                // Extract String into separate state for Fault to be able
                // exact type of value expected next in data
                States::ValueString => {
                    // first byte is value type
                    if !self.buffer.consume(1, &mut src) {
                        assert!(src.is_all_consumed());
                        // when we processing method arguments we dont need data
                        return Ok((true, src.consumed()));
                    }

                    *state = States::StrHead {
                        head: self.buffer.data[0],
                    };
                    self.buffer.reset();
                }

                // Protocol version 3.0 zigzag encoded int
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
                    if *bytes_cnt == 0 {
                        cb.error("bad size");
                        return Err(src.pos);
                    }

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
                States::StrHead { head } => {
                    match *head & TYPE_MASK {
                        STRING_ID => {
                            // get used octects
                            let octects = (*head & OCTET_CNT_MASK) as usize;
                            *state = States::StrLen { octects };
                        }
                        _ => {
                            cb.error("invalid type id");
                            return Err(src.pos);
                        }
                    }
                }
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
                    // Do we have any binary data and is not 0 length?
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

                States::DataInit => {
                    // first byte is data type or fault
                    if !self.buffer.consume(1, &mut src) {
                        assert!(src.is_all_consumed());
                        // data is optional so we dont need data
                        return Ok((false, src.consumed()));
                    }

                    match self.buffer.data[0] & TYPE_MASK {
                        FRPS_DATA => {
                            let octects: usize = match self.buffer.data[0] & OCTET_CNT_MASK {
                                0 => 0,
                                1 => 2,
                                2 => 4,
                                4 => 8,
                                _ => {
                                    cb.error("invalid type");
                                    return Err(src.pos);
                                }
                            };

                            *state = States::DataLen { octects };
                            self.context = Context::Data;
                        }
                        FAULT_RESPOSE_ID => *state = States::Fault,
                        _ => {
                            cb.error("unknown type id");
                            return Err(src.pos);
                        }
                    }

                    self.buffer.reset();
                }

                States::DataLen { octects } => {
                    // read array len
                    if !self.buffer.consume(*octects, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    let length = read_i64(&self.buffer.data[0..*octects]) as usize;

                    *state = States::Data {
                        length,
                        processed: 0,
                    };

                    self.buffer.reset();
                }

                States::Data { length, processed } => {
                    assert!(*processed <= *length, "invalid state");
                    // Do we have any stream data and is not 0 length?
                    if (*processed != *length) && src.is_all_consumed() {
                        return Ok((true, src.consumed()));
                    }

                    // Process available or missing part
                    let cnt = cmp::min(src.available(), *length - *processed);
                    let run = cb.stream_data(src.data(cnt));
                    if !run {
                        //dbg!(src.pos, &src.src[src.pos..], cb);
                        cb.error("cb::data_stream in BinData failed");
                        return Err(src.pos);
                    }

                    // update processed data
                    src.advance(cnt);
                    *processed += cnt;

                    // did we process all stream data?
                    if processed != length {
                        assert!(src.is_all_consumed());
                        return Ok((true, src.consumed()));
                    }

                    *state = States::Value;
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

/// Read i64 integer from slice with variable number of bytes betwwen 1 to 8
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

/*
   Decodes signed integer from unsigned,
   with positive values even and negative values odd
   starting around zero.
   This saves transfer space and unifies integer encoding.
   * -1 -> -9223372036854775808
   *  0 -> 0
   *  1 -> -1
   *  2 -> 1
   *  3 -> -2
   *  4 -> 2
   * ...
   a = -9223372036854775808 Min i64
   1000000000000000000000000000000000000000000000000000000000000000
   a << 1
   0000000000000000000000000000000000000000000000000000000000000000
   a >> 63
   1111111111111111111111111111111111111111111111111111111111111111
   b = encoded(a)
   1111111111111111111111111111111111111111111111111111111111111111
   signed(b) >> 1
   1111111111111111111111111111111111111111111111111111111111111111
   unsigned(b) >> 1
   0111111111111111111111111111111111111111111111111111111111111111
   -(b & 1))
   1111111111111111111111111111111111111111111111111111111111111111
   c = decoded(b)
   1000000000000000000000000000000000000000000000000000000000000000

   a = -1
   1111111111111111111111111111111111111111111111111111111111111111
   a << 1
   1111111111111111111111111111111111111111111111111111111111111110
   a >> 63
   1111111111111111111111111111111111111111111111111111111111111111
   b = encoded(a)
   0000000000000000000000000000000000000000000000000000000000000001
   signed(b) >> 1
   0000000000000000000000000000000000000000000000000000000000000000
   unsigned(b) >> 1
   0000000000000000000000000000000000000000000000000000000000000000
   -(b & 1))
   1111111111111111111111111111111111111111111111111111111111111111
   c = decoded(b)
   1111111111111111111111111111111111111111111111111111111111111111

   a = 1
   0000000000000000000000000000000000000000000000000000000000000001
   a << 1
   0000000000000000000000000000000000000000000000000000000000000010
   a >> 63
   0000000000000000000000000000000000000000000000000000000000000000
   b = encoded(a)
   0000000000000000000000000000000000000000000000000000000000000010
   signed(b) >> 1
   0000000000000000000000000000000000000000000000000000000000000001
   unsigned(b) >> 1
   0000000000000000000000000000000000000000000000000000000000000001
   -(b & 1))
   0000000000000000000000000000000000000000000000000000000000000000
   c = decoded(b)
   0000000000000000000000000000000000000000000000000000000000000001

   a = 9223372036854775807 - Max i64
   0111111111111111111111111111111111111111111111111111111111111111
   a << 1
   1111111111111111111111111111111111111111111111111111111111111110
   a >> 63
   0000000000000000000000000000000000000000000000000000000000000000
   b = encoded(a)
   1111111111111111111111111111111111111111111111111111111111111110
   signed(b) >> 1
   1111111111111111111111111111111111111111111111111111111111111111
   unsigned(b) >> 1
   0111111111111111111111111111111111111111111111111111111111111111
   -(b & 1))
   0000000000000000000000000000000000000000000000000000000000000000
   c = decoded(b)
   0111111111111111111111111111111111111111111111111111111111111111

   int64_t encode(int64_t n) {
       return ((n << 1) ^ (n >> 63));
   }
   static int64_t decode(int64_t s) {
       uint64_t n = static_cast<uint64_t>(s);
       return static_cast<int64_t>((n >> 1) ^ (-(s & 1)));
   }
*/

fn zigzag_decode(s: &[u8]) -> i64 {
    let s = read_i64(s);

    let n = u64::from_le_bytes(s.to_le_bytes()) >> 1;
    let n = i64::from_le_bytes(n.to_le_bytes());

    return n ^ (-(s & 1));
}
