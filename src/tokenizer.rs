use std::cmp;
use std::str;

use crate::constants::*;

enum States {
    Init,
    MessageType,
    CallNameSize,
    Response,
    FaultInit,
    CallName((usize, usize)), // method name length, processed
    Value,
    ValueComplete,
    Integer1((bool, usize)), // negative value, octect cnt
    Integer3(usize),         // octect cnt
}

pub trait Callback {
    /** Parsing always stop after this callback return. */
    fn error(&mut self);

    /* Stop on false, continue on true */
    fn version(&mut self, major_version: u8, minor_version: u8) -> bool;

    /* Stop on false, continue on true */
    fn call(&mut self, method: &str, avail: usize, lenght: usize) -> bool;

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
    fn boolean(&mut self) -> bool;
    fn double_number(&mut self) -> bool;
    fn datetime(&mut self) -> bool;
    fn binary(&mut self) -> bool;
    fn push_array(&mut self) -> bool;
    fn push_struct(&mut self) -> bool; // pushMap
    fn map_key(&mut self) -> bool;

    fn pop_context(&mut self);
}

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
    data: [u8; 11],
}

impl Buffer {
    fn new() -> Buffer {
        Buffer {
            cnt: 0,
            data: [0; 11],
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

    pub fn parse<T: Callback>(&mut self, src: &[u8], cb: &mut T) -> Result<usize, &'static str> {
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
                        return Err("Invalid magic expected 0xCA11");
                    }

                    self.version_major = self.buffer.data[2];
                    self.version_minor = self.buffer.data[3];

                    if !cb.version(self.version_major, self.version_minor) {
                        return Err("Invalid version");
                    }

                    *state = States::MessageType;
                    self.buffer.reset();
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
                        FAULT_RESPOSE_ID => *state = States::Response,
                        _ => return Err("Invalid message type"),
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
                        return Err("Invalid lenght of method name");
                    }

                    *state = States::CallName((length, 0));
                    self.buffer.reset();
                }

                States::CallName((lenght, procesed)) => {
                    // read method name
                    let avail = cmp::min(*lenght - *procesed, src.available());
                    if avail == 0 {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    let run = cb.call(str::from_utf8(src.data(avail)).unwrap(), avail, *lenght);

                    let procesed = *procesed + avail;
                    src.advance(avail);

                    if !run || *lenght != procesed {
                        *state = States::CallName((*lenght, procesed));
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
                            let octects = (self.buffer.data[0] & OCTET_CNT_MASK) as usize;

                            if self.version_major == 3 {
                                *state = States::Integer3(octects);
                            } else {
                                // negative number
                                let negative = (self.buffer.data[0] & VINT_ID) != 0;
                                *state = States::Integer1((negative, octects));
                            }
                        }
                        STRING_ID => {}
                        BIN_ID => {}
                        NULL_ID => {}
                        STRUCT_ID => {}
                        ARRAY_ID => {}
                        BOOL_ID => {}
                        DOUBLE_ID => {}
                        DATETIME_ID => {}
                        _ => return Err("Invalid type id"),
                    }

                    self.buffer.reset();
                }

                States::Integer3(octects) => {
                    if !self.buffer.consume(*octects, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    let run = cb.integer(zigzag_decode(&self.buffer.data[0..*octects]));
                    if !run {
                        return Err("Invalid integer value");
                    }
                    *state = States::ValueComplete;
                }

                States::Integer1((negative, octects)) => {
                    if !self.buffer.consume(*octects, &mut src) {
                        assert!(src.is_all_consumed());
                        return Ok(src.consumed());
                    }

                    let mut v = read_i64(&self.buffer.data[0..*octects]);
                    if *negative {
                        v *= -1;
                    }

                    let run = cb.integer(v);
                    if !run {
                        return Err("Invalid integer value");
                    }
                    *state = States::ValueComplete;
                }

                States::Response => {}

                States::FaultInit => {}

                States::ValueComplete => {
                    self.buffer.reset();
                    if src.is_all_consumed() {
                        return Ok(src.consumed());
                    }
                }

                _ => {
                    return Err("Invalid state");
                }
            }
        }

        return Ok(0);
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
