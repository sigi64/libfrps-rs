use crate::constants::*;
use crate::serialize::DateTimeVer30;
use byteorder::{ByteOrder, LittleEndian};
use std::cmp;
use std::str;
use std::fmt::Debug; 

enum States {
    Init,
    MessageType,
    CallNameSize,
    Response,
    Fault,
    CallName { length: usize, processed: usize },
    Value,
    Pop,
    Integer1 { is_negative: bool, octects: usize },
    Integer3 { octects: usize },
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
}

pub trait Callback {
    /** Parsing always stop after this callback return. */
    fn error(&mut self);

    /* Stop on false, continue on true */
    fn version(&mut self, major_version: u8, minor_version: u8) -> bool;

    /* Stop on false, continue on true */
    fn call(&mut self, method: &str, lenght: usize) -> bool;

    /* Stop on false, continue on true */
    fn response(&mut self) -> bool;

    /* Stop on false, continue on true */
    fn fault(&mut self) -> bool;

    /* Stop on false, continue on true */
    fn stream_data(&mut self) -> bool;

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

    /// return number of bytes consumed. It is expected that all
    /// dat was consumed, otherwise it is error
    fn consumed(&self) -> usize {
        assert_eq!(self.pos, self.src.len());
        return self.src.len();
    }

    fn is_all_consumed(&self) -> bool {
        return self.pos == self.src.len();
    }
}

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
        assert!(need > self.cnt);
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

pub struct Tokenizer {
    // Here we store state for recursive values as array and structs
    stack: Vec<States>,
    buffer: Buffer,

    version_major: u8,
    version_minor: u8,
}

impl Tokenizer {
    pub fn new() -> Tokenizer {
        Tokenizer {
            stack: vec![States::Init],
            buffer: Buffer::new(),

            version_major: 0,
            version_minor: 0,
        }
    }

    pub fn parse<T: Callback + Debug>(&mut self, src: &[u8], cb: &mut T) -> Result<usize, &'static str> {
        let mut src = SourcePtr::new(src);

        while let Some(state) = self.stack.last_mut() {
            match state {
                States::Init => {
                    // first 4 bytes is header with magic and version
                    if !self.buffer.consume(4, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    // check FRPC magic and version
                    if self.buffer.data[0] != 0xca || self.buffer.data[1] != 0x11 {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("Invalid magic expected 0xCA11");
                    }

                    self.version_major = self.buffer.data[2];
                    self.version_minor = self.buffer.data[3];

                    // We support version 3.0 or less (2.1, 2.0, 1.0)
                    if self.version_major > 3
                        || ((self.version_major == 3) && (self.version_minor > 0))
                    {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("Unsuported version of FRPC/S protocol");
                    }

                    if !cb.version(self.version_major, self.version_minor) {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("Invalid version");
                    }

                    *state = States::MessageType;
                    self.buffer.reset();
                }

                States::Pop => {
                    self.buffer.reset();
                    self.stack.pop();
                }

                States::MessageType => {
                    // first byte is message type
                    if !self.buffer.consume(1, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    match self.buffer.data[0] & TYPE_MASK {
                        CALL_ID => *state = States::CallNameSize,
                        RESPOSE_ID => *state = States::Response,
                        FAULT_RESPOSE_ID => *state = States::Fault,
                        _ => {
                            dbg!(src.pos, &src.src[src.pos..], cb);
                            return Err("Invalid message type");
                        }
                    }

                    self.buffer.reset();
                }

                States::CallNameSize => {
                    // first byte is method name lenght
                    if !self.buffer.consume(1, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    let length: usize = self.buffer.data[0] as usize;
                    if length == 0 {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("Invalid lenght of method name");
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
                        return Ok(src.consumed());
                    }

                    let run = cb.call(str::from_utf8(src.data(avail)).unwrap(), *length);

                    *processed += avail;
                    src.advance(avail);

                    if !run || *length != *processed {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    *state = States::Value;
                    self.buffer.reset();
                }

                States::Value => {
                    // first byte is value type
                    if !self.buffer.consume(1, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    match self.buffer.data[0] & TYPE_MASK {
                        VINT_ID | INT_ID | U_VINT_ID => {
                            // get used octects
                            let octects = (self.buffer.data[0] & OCTET_CNT_MASK) as usize;

                            if self.version_major >= 3 {
                                *state = States::Integer3 { octects };
                            } else {
                                // negative number
                                let is_negative = (self.buffer.data[0] & VINT_ID) != 0;
                                *state = States::Integer1 {
                                    is_negative,
                                    octects,
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
                            let run = cb.null();
                            if !run {
                                dbg!(src.pos, &src.src[src.pos..], cb);
                                return Err("cb::null in Value failed");
                            }
                            *state = States::Pop;
                        }
                        BOOL_ID => {
                            let v = (self.buffer.data[0] & OCTET_CNT_MASK) != 0;
                            let run = cb.boolean(v);
                            if !run {
                                dbg!(src.pos, &src.src[src.pos..], cb);
                                return Err("cb::boolean in Value failed");
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
                            dbg!(src.pos, &src.src[src.pos..], cb);
                            return Err("Invalid type id");
                        }
                    }

                    self.buffer.reset();
                }

                States::Integer3 { octects } => {
                    let bytes_cnt = *octects + 1;
                    if !self.buffer.consume(bytes_cnt, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    let run = cb.integer(zigzag_decode(&self.buffer.data[0..bytes_cnt]));
                    if !run {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("cb::integer in Integer3 failed");
                    }
                    *state = States::Pop;
                }

                States::Integer1 {
                    is_negative,
                    octects,
                } => {
                    let bytes_cnt = *octects + 1;
                    if !self.buffer.consume(bytes_cnt, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    let mut v = read_i64(&self.buffer.data[0..bytes_cnt]);
                    if *is_negative {
                        v *= -1;
                    }

                    let run = cb.integer(v);
                    if !run {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("cb::integer in Integer1 failed");
                    }
                    *state = States::Pop;
                }
                // String
                States::StrLen { octects } => {
                    let bytes_cnt = *octects + 1;
                    // read array len
                    if !self.buffer.consume(bytes_cnt, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    let cnt = read_i64(&self.buffer.data[0..bytes_cnt]) as usize;

                    let run = cb.string_begin(cnt);
                    if !run {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("cb::string_begin in StrLen failed");
                    }

                    *state = States::StrData {
                        length: cnt,
                        processed: 0,
                    };
                    self.buffer.reset();
                }

                States::StrData { length, processed } => {
                    assert!(*processed < *length, "invalid state");
                    // Do we have any string data?
                    if src.is_all_consumed() {
                        return Ok(src.consumed());
                    }

                    // Process available or missing part
                    let cnt = cmp::min(src.available(), *length - *processed);
                    let run = cb.string_data(src.data(cnt), *length);
                    if !run {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("cb::string_data in StrData failed");
                    }

                    // update processed data
                    src.advance(cnt);
                    *processed += cnt;

                    // did we process all string data?
                    if processed != length {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed()); // no we need more data
                    }

                    // string is completed
                    let run = cb.value_end();
                    if !run {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("cb::value_end in StrData failed");
                    }

                    *state = States::Pop;
                }
                // Binary
                States::BinLen { octects } => {
                    let bytes_cnt = *octects + 1;
                    // read array len
                    if !self.buffer.consume(bytes_cnt, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    let cnt = read_i64(&self.buffer.data[0..bytes_cnt]) as usize;

                    let run = cb.binary_begin(cnt);
                    if !run {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("cb::binary_begin in BinLen failed");
                    }

                    *state = States::BinData {
                        length: cnt,
                        processed: 0,
                    };
                    self.buffer.reset();
                }

                States::BinData { length, processed } => {
                    assert!(*processed < *length, "invalid state");
                    // Do we have any binary data?
                    if src.is_all_consumed() {
                        return Ok(src.consumed());
                    }

                    // Process available or missing part
                    let cnt = cmp::min(src.available(), *length - *processed);
                    let run = cb.binary_data(src.data(cnt), *length);
                    if !run {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("cb::binary_data in BinData failed");
                    }

                    // update processed data
                    src.advance(cnt);
                    *processed += cnt;

                    // did we process all binary data?
                    if processed != length {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed()); // no we need more data
                    }

                    // binary is completed
                    let run = cb.value_end();
                    if !run {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("cb::value_end in BinData failed");
                    }

                    *state = States::Pop;
                }
                // Array
                States::ArrayInit { octects } => {
                    let bytes_cnt = *octects + 1;
                    // read array len
                    if !self.buffer.consume(bytes_cnt, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    let cnt = read_i64(&self.buffer.data[0..bytes_cnt]) as usize;
                    let run = cb.array_begin(cnt);
                    if !run {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("cb::array_begin in ArrayInit failed");
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
                            dbg!(src.pos, &src.src[src.pos..], cb);
                            return Err("cb::value_end in ArrayItem failed");
                        }
                        *state = States::Pop;
                    }
                }
                // Struct
                States::StructHead { octects } => {
                    let bytes_cnt = *octects + 1;
                    // read array len
                    if !self.buffer.consume(bytes_cnt, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    let cnt = read_i64(&self.buffer.data[0..bytes_cnt]) as usize;

                    let run = cb.struct_begin(cnt);
                    if !run {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("cb::struct_begin in StructHead failed");
                    }

                    *state = States::StructItem { items: cnt };
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
                            dbg!(src.pos, &src.src[src.pos..], cb);
                            return Err("cb::value_end in StructItem failed");
                        }
                        *state = States::Pop;
                    }
                }

                States::StructKeyHead => {
                    if !self.buffer.consume(1, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    let len = self.buffer.data[0] as usize;
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
                        return Ok(src.consumed());
                    }

                    // Process available or missing part
                    let cnt = cmp::min(src.available(), *length - *processed);
                    let run = cb.struct_key(src.data(cnt), *length);
                    if !run {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("cb::struct_key in StructKey failed");
                    }

                    // update processed data
                    src.advance(cnt);
                    *processed += cnt;

                    // did we process all binary data?
                    if processed != length {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed()); // no we need more data
                    }

                    *state = States::Pop;
                }

                States::Response => {
                    let run = cb.response();
                    if !run {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("cb::response in Response failed");
                    }

                    *state = States::Value;
                }

                States::Fault => {
                    let run = cb.fault();
                    if !run {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("cb::fault in Fault failed");
                    }

                    self.stack.push(States::Value); // Message
                    self.stack.push(States::Value); // status code
                }

                States::Double => {
                    if !self.buffer.consume(8, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    let v = LittleEndian::read_f64(&self.buffer.data[0..8]);
                    let run = cb.double_number(v);
                    if !run {
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("cb::double_number failed");
                    }
                    *state = States::Pop;
                }

                States::DateTime => {
                    let bytes: usize = if self.version_major == 3 { 14 } else { 10 };

                    if !self.buffer.consume(bytes, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
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
                        let unix_time = LittleEndian::read_u64(&self.buffer.data[2..]);
                        let week_day: u8 = self.buffer.data[10] & 0x07;
                        let sec: u8 = ((self.buffer.data[10] & 0xf8) >> 3)
                            | ((self.buffer.data[11] & 0x01) << 5);
                        let min: u8 = (self.buffer.data[11] & 0x7e) >> 1;
                        let hour: u8 = ((self.buffer.data[11] & 0x80) >> 7)
                            | ((self.buffer.data[12] & 0x0f) << 1);
                        let day: u8 = ((self.buffer.data[12] & 0xf0) >> 4)
                            | ((self.buffer.data[13] & 0x01) << 4);
                        let month: u8 = (self.buffer.data[13] & 0x1e) >> 1;
                        let year = (((self.buffer.data[13] as u16) & 0xe0) >> 5)
                            | ((self.buffer.data[14] as u16) << 3) + 1600;

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
                        let unix_time = LittleEndian::read_u32(&self.buffer.data[2..]) as u64;
                        let week_day = self.buffer.data[6] & 0x07;
                        let sec = ((self.buffer.data[6] & 0xf8) >> 3)
                            | ((self.buffer.data[7] & 0x01) << 5);
                        let min = (self.buffer.data[7] & 0x7e) >> 1;
                        let hour = ((self.buffer.data[7] & 0x80) >> 7)
                            | ((self.buffer.data[8] & 0x0f) << 1);
                        let day = ((self.buffer.data[8] & 0xf0) >> 4)
                            | ((self.buffer.data[9] & 0x01) << 4);
                        let month = (self.buffer.data[9] & 0x1e) >> 1;
                        let year = (((self.buffer.data[9] as u16) & 0xe0) >> 5)
                            | ((self.buffer.data[10] as u16) << 3) + 1600;

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
                        dbg!(src.pos, &src.src[src.pos..], cb);
                        return Err("cb::datetime in Datetime failed");
                    }

                    *state = States::Pop;
                }
            }
        }

        dbg!(src.pos, &src.src[src.pos..], cb);
        assert!(src.is_all_consumed());
        return Ok(src.consumed());
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
