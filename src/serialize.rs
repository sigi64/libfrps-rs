use byteorder::{ByteOrder, LittleEndian};
use std::cmp;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::{i64, u64, u16};

use crate::constants::*;

static ZERO: u64 = 0;
static ALLONES: u64 = !ZERO;
static INT8_MASK: u64 = ALLONES << 8;
static INT16_MASK: u64 = ALLONES << 16;
static INT24_MASK: u64 = ALLONES << 24;
static INT32_MASK: u64 = ALLONES << 32;
static INT40_MASK: u64 = ALLONES << 40;
static INT48_MASK: u64 = ALLONES << 48;
static INT56_MASK: u64 = ALLONES << 56;

fn get_octets(number: u64) -> usize {
    if (number & INT8_MASK) == 0 {
        return 0;
    } // one byte will be enough
    if (number & INT16_MASK) == 0 {
        return 1;
    }
    if (number & INT24_MASK) == 0 {
        return 2;
    }
    if (number & INT32_MASK) == 0 {
        return 3;
    }
    if (number & INT40_MASK) == 0 {
        return 4;
    }
    if (number & INT48_MASK) == 0 {
        return 5;
    }
    if (number & INT56_MASK) == 0 {
        return 6;
    }
    return 7;
}

/** Encodes signed integer as unsigned,
 * with positive values even and negative values odd
 * starting around zero.
 * This saves transfer space and unifies integer encoding.
 * 0 -> 0
 * -1 -> 1
 * 1 -> 2
 * -2 -> 3
 * 2 -> 4
 * ...
 */
fn zigzag_encode(n: i64) -> u64 {
    // the right shift has to be arithmetic
    // negative numbers become all binary 1s
    // positive numbers become all binary 0s
    // effectively inverting bits of the result in
    // case of negative number
    let tmp = (n << 1) ^ (n >> 63);

    return u64::from_le_bytes(tmp.to_le_bytes());
}

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

/** Writes protocol header and message type
 * @return Number of bytes written or zero when there is not enough room
 */
fn write_magic(msg_type: u8, dst: &mut [u8]) -> Result<usize, &'static str> {
    if dst.len() < 5 {
        return Err("not enought space");
    }
    dst[0] = 0xCA;
    dst[1] = 0x11;
    dst[2] = 3; // FRPC_MAJOR_VERSION
    dst[3] = 0; // FRPC_MINOR_VERSION
    dst[4] = msg_type;
    Ok(5)
}

/** Writes tag and bool value */
fn write_bool(val: bool, dst: &mut [u8]) -> Result<usize, &'static str> {
    if dst.len() < 1 {
        return Err("not enought space");
    }

    dst[0] = BOOL_ID | (if val { 1u8 } else { 0u8 });
    Ok(1)
}

/** Writes tag and null value */
fn write_null(dst: &mut [u8]) -> Result<usize, &'static str> {
    if dst.len() < 1 {
        return Err("not enought space");
    }
    dst[0] = NULL_ID;
    Ok(1)
}

/** Writes tag and integer value */
fn write_int(val: i64, dst: &mut [u8]) -> Result<usize, &'static str> {
    let val = zigzag_encode(val);
    let octets = get_octets(val);
    if dst.len() < (octets + 2) {
        return Err("not enought space");
    }
    dst[0] = INT_ID | u8::try_from(octets).unwrap();
    LittleEndian::write_u64(&mut dst[1..], val);

    Ok(octets + /*header*/ 1 + /*first byte*/1)
}

/** Writes tag and double value */
fn write_double(val: f64, dst: &mut [u8]) -> Result<usize, &'static str> {
    if dst.len() < 9 {
        return Err("not enought space");
    }

    dst[0] = DOUBLE_ID;
    LittleEndian::write_f64(&mut dst[1..], val);
    Ok(9)
}

/** Writes tag and datetime value */
fn write_datetime_v30(val: &DateTimeVer30, dst: &mut [u8]) -> Result<usize, &'static str> {
    if dst.len() < 15 {
        return Err("not enought space");
    }

    dst[0] = DATETIME_ID;

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

    dst[1] = (val.time_zone / 60i16 / 15i16).try_into().unwrap();
    
    LittleEndian::write_u64(&mut dst[2..], val.unix_time);
    // serialize
    let mut byte: u8 = (val.sec & 0x1f) << 3; 
    byte |= val.week_day & 0x07;
    dst[10] = byte;
    let mut byte: u8 = (val.min & 0x3f) << 1;
    byte |= (val.sec & 0x20) >> 5;
    byte |= (val.hour & 0x01) << 7;
    dst[11] = byte;
    let mut byte: u8 = (val.hour & 0x1e) >> 1;
    byte |= (val.day & 0x0f) << 4;
    dst[12] = byte;
    let mut byte: u8 = (val.day & 0x1f) >> 4;
    byte |= (val.month & 0x0f) << 1;
    let year: u16 = if val.year < 1600  {0 } else {val.year - 1600};
    byte |= ((year & 0x07) << 5).to_le_bytes()[1];
    dst[13] = byte;
    let byte: u8 = ((year & 0x07f8) >> 3).to_le_bytes()[0];
    dst[14] = byte;

    Ok(15)
}

/** Writes tag and length for string, binary, array and struct types */
fn write_head(frps_type: u8, size: usize, dst: &mut [u8]) -> Result<usize, &'static str> {
    let octets = get_octets(size.try_into().unwrap());

    if dst.len() < (octets + 2) {
        return Err("not enought space");
    }

    dst[0] = frps_type | u8::try_from(octets).unwrap();
    // this works only for little-endian systems
    LittleEndian::write_u64(&mut dst[1..], size as u64);

    Ok(octets + /*header*/ 1 + /*first byte*/1)
}

/** Writes head of struct key */
fn write_key_head(size: usize, dst: &mut [u8]) -> Result<usize, &'static str> {
    if dst.len() < 1 {
        return Err("not enought space");
    }
    dst[0] = size.try_into().unwrap();
    Ok(1)
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

enum States<'a> {
    Init,

    Value(&'a Value),

    FlushBuffer,
    StackPop,

    StrInit(&'a str),
    StrHead(&'a str),
    StrValue(&'a str),
    BinInit(&'a [u8]),

    BinHead(&'a [u8]),
    BinValue(&'a [u8]),
    StructInit(&'a HashMap<String, Value>),
    StructHead(&'a HashMap<String, Value>),
    StructItem(std::collections::hash_map::Iter<'a, String, Value>),
    StructItemKey(&'a String),

    ArrayInit(&'a Vec<Value>),
    ArrayHead(&'a Vec<Value>),
    ArrayItem(std::slice::Iter<'a, Value>),

    CallHead,
    CallMethod,

    ResponseHead,

    FaultHead,
    FaultCode,
    FaultMsg,
    FaultMsgData,
}

/** Represent either temporary buffer
 *   or keep state how many bytes was copied from value parametr source
 *  (string values arrays eg.)
 */
struct Source {
    len: usize,
    pos: usize,
    buffer: [u8; 15],
}

impl Source {
    /** Set how many bytes to copy either from source or buffer */
    fn prepare(&mut self, len: usize) {
        self.len = len;
        self.pos = 0;
    }

    /** Write internal buffer to dst */
    fn flush(&mut self, dst: &mut [u8], written: usize) -> usize {
        let write = cmp::min(dst.len() - written, self.len - self.pos);

        if write > 0 {
            // make slices same size for copy_from_slice
            let d = &mut dst[written..(written + write)];
            let s = &self.buffer[self.pos..(self.pos + write)];
            d.copy_from_slice(s);
            self.pos += write;
        }
        write
    }

    /** return true when all data from source or internal buffer was flushed */
    fn is_empty(&self) -> bool {
        self.pos == self.len
    }
}

pub struct Serializer<'a> {
    stack: Vec<States<'a>>,
    source: Source, // colecting buffer
}

impl<'a> Serializer<'a> {
    pub fn new() -> Serializer<'a> {
        Serializer {
            stack: vec![States::Init],
            source: Source {
                len: 0,
                pos: 0,
                buffer: [0; 15],
            },
        }
    }

    // Set Serializer to initial state. must be used bettween calling
    // writeCall, writeValue, writeResponse
    pub fn reset(&mut self) {
        self.stack.clear();
        self.stack.push(States::Init);
    }

    fn write_v(&mut self, dst: &mut [u8], written: usize) -> Result<usize, &'static str> {
        let mut written = written;

        while let Some(state) = self.stack.last_mut() {
            match state {
                States::Value(value) => match value {
                    Value::Str(x) => *state = States::StrInit(&x.as_str()),
                    Value::Binary(x) => *state = States::BinInit(&x),
                    Value::Struct(x) => *state = States::StructInit(&x),
                    Value::Array(x) => *state = States::ArrayInit(&x),
                    Value::Bool(x) => {
                        if written == dst.len() {
                            return Ok(written); // dst buffer is full
                        }
                        written += write_bool(*x, &mut dst[written..]).unwrap();
                        *state = States::StackPop;
                    }
                    Value::Null => {
                        if written == dst.len() {
                            return Ok(written); // dst buffer is full
                        }
                        written += write_null(&mut self.source.buffer).unwrap();
                        *state = States::StackPop;
                    }
                    Value::Int(x) => {
                        let cnt = write_int(*x, &mut self.source.buffer).unwrap();
                        self.source.prepare(cnt);
                        *state = States::FlushBuffer;
                    }
                    Value::Double(x) => {
                        let cnt = write_double(*x, &mut self.source.buffer).unwrap();
                        self.source.prepare(cnt);
                        *state = States::FlushBuffer;
                    }
                    Value::DateTime(x) => {
                        let cnt = write_datetime_v30(x, &mut self.source.buffer).unwrap();
                        self.source.prepare(cnt);
                        *state = States::FlushBuffer;
                    }
                },

                States::FlushBuffer => {
                    written += self.source.flush(dst, written);
                    if !self.source.is_empty() {
                        return Ok(written); // dst buffer is full
                    }
                    *state = States::StackPop;
                }
                States::StackPop => {
                    self.stack.pop();
                }

                // String
                States::StrInit(x) => {
                    let cnt = write_head(STRING_ID, x.len(), &mut self.source.buffer).unwrap();
                    self.source.prepare(cnt);
                    *state = States::StrHead(&x);
                }
                States::StrHead(x) => {
                    written += self.source.flush(dst, written);
                    if !self.source.is_empty() {
                        return Ok(written); // dst buffer is full
                    }
                    // prepare string value itself
                    self.source.prepare(x.len());
                    *state = States::StrValue(&x);
                }
                States::StrValue(x) => {
                    written +=
                        Serializer::copy_next_chunk(dst, written, &mut self.source, x.as_bytes());
                    *state = States::StackPop;
                }

                // Binary
                States::BinInit(x) => {
                    let cnt = write_head(BIN_ID, x.len(), &mut self.source.buffer).unwrap();
                    self.source.prepare(cnt);
                    *state = States::BinHead(&x);
                }
                States::BinHead(x) => {
                    written += self.source.flush(dst, written);
                    if !self.source.is_empty() {
                        return Ok(written); // dst buffer is full
                    }
                    // prepare string value itself
                    self.source.prepare(x.len());
                    *state = States::BinValue(&x);
                }
                States::BinValue(x) => {
                    written += Serializer::copy_next_chunk(dst, written, &mut self.source, x);
                    *state = States::StackPop;
                }

                // Array
                States::ArrayInit(v) => {
                    let cnt = write_head(ARRAY_ID, v.len(), &mut self.source.buffer).unwrap();
                    self.source.prepare(cnt);
                    *state = States::ArrayHead(&v);
                }
                States::ArrayHead(v) => {
                    written += self.source.flush(dst, written);
                    if !self.source.is_empty() {
                        return Ok(written); // dst buffer is full
                    }
                    *state = States::ArrayItem(v.iter());
                }
                States::ArrayItem(iter) => match iter.next() {
                    None => *state = States::StackPop,
                    Some(x) => self.stack.push(States::Value(&x)),
                },

                // Struct
                States::StructInit(v) => {
                    let cnt = write_head(STRUCT_ID, v.len(), &mut self.source.buffer).unwrap();
                    self.source.prepare(cnt);
                    *state = States::StructHead(&v);
                }
                States::StructHead(v) => {
                    written += self.source.flush(dst, written);
                    if !self.source.is_empty() {
                        return Ok(written); // dst buffer is full
                    }
                    *state = States::StructItem(v.iter());
                }
                States::StructItem(iter) => {
                    match iter.next() {
                        None => *state = States::StackPop,
                        Some((key, x)) => {
                            // check key length
                            if key.len() > 255 {
                                return Err("Key is too long");
                            }
                            if written == dst.len() {
                                return Ok(written); // dst buffer is full
                            }
                            written += write_key_head(key.len(), &mut dst[written..]).unwrap();
                            self.source.prepare(key.len());

                            self.stack.push(States::Value(x));
                            self.stack.push(States::StructItemKey(key));
                        }
                    }
                }
                States::StructItemKey(key) => {
                    written +=
                        Serializer::copy_next_chunk(dst, written, &mut self.source, key.as_bytes());
                    if !self.source.is_empty() {
                        return Ok(written); // dst buffer is full
                    }
                    *state = States::StackPop;
                }
                _ => return Err("Invalid state"),
            } // states match
        } // stack iteration
        Ok(written)
    }

    // create FRPC/S method call with method name.
    pub fn write_call(&mut self, dst: &mut [u8], name: &str) -> Result<usize, &'static str> {
        let mut written: usize = 0;

        while let Some(state) = self.stack.last_mut() {
            match state {
                States::Init => {
                    let cnt = write_magic(CALL_ID, &mut self.source.buffer).unwrap();
                    self.source.prepare(cnt);
                    *state = States::CallHead;
                }
                States::CallHead => {
                    written += self.source.flush(dst, written);
                    if !self.source.is_empty() {
                        return Ok(written);
                    }
                    if name.len() > 255 {
                        return Err("method name too long");
                    }
                    if written == dst.len() {
                        return Ok(written);
                    }

                    // prepare method name lenght in the buffer
                    dst[written] = name.len() as u8;
                    written += 1;
                    self.source.prepare(name.len());
                    *state = States::CallMethod;
                }
                States::CallMethod => {
                    written += Serializer::copy_next_chunk(
                        dst,
                        written,
                        &mut self.source,
                        &name.as_bytes(),
                    );
                    *state = States::StackPop;
                }
                States::StackPop => {
                    self.stack.pop();
                }
                _ => return Err("Invalid state"),
            }
        }
        Ok(written)
    }

    // create FRPC/S method call with method name.
    pub fn write_value(&mut self, dst: &mut [u8], value: &'a Value) -> Result<usize, &'static str> {
        while let Some(state) = self.stack.last_mut() {
            match state {
                States::Init => *state = States::Value(&value),
                States::Value(_) => return self.write_v(dst, 0),
                _ => return Err("Invalid state"),
            }
        }
        return Err("serializer is not initialized");
    }

    pub fn write_response(
        &mut self,
        dst: &mut [u8],
        value: &'a Value,
    ) -> Result<usize, &'static str> {
        let mut written: usize = 0;

        while let Some(state) = self.stack.last_mut() {
            match state {
                States::Init => {
                    let cnt = write_magic(RESPOSE_ID, &mut self.source.buffer).unwrap();
                    self.source.prepare(cnt);
                    *state = States::ResponseHead;
                }
                States::ResponseHead => {
                    written += self.source.flush(dst, written);
                    if !self.source.is_empty() {
                        return Ok(written);
                    }
                    *state = States::Value(&value);
                }
                States::Value(_) => return self.write_v(dst, 0),
                _ => return Err("Invalid state"),
            }
        }
        return Err("serializer is not initialized");
    }

    pub fn write_fault(
        &mut self,
        dst: &mut [u8],
        code: i64,
        msg: &str,
    ) -> Result<usize, &'static str> {
        let mut written: usize = 0;

        while let Some(state) = self.stack.last_mut() {
            match state {
                States::Init => {
                    // Write response header
                    let cnt = write_magic(FAULT_RESPOSE_ID, &mut self.source.buffer).unwrap();
                    self.source.prepare(cnt);
                    *state = States::FaultHead;
                }
                States::FaultHead => {
                    written += self.source.flush(dst, written);
                    if !self.source.is_empty() {
                        return Ok(written);
                    }
                    // push status code into the buffer
                    let cnt = write_int(code, &mut self.source.buffer).unwrap();
                    self.source.prepare(cnt);

                    *state = States::FaultCode;
                }
                States::FaultCode => {
                    written += self.source.flush(dst, written);
                    if !self.source.is_empty() {
                        return Ok(written);
                    }

                    // push status message head into the buffer
                    let cnt = write_head(STRING_ID, msg.len(), &mut self.source.buffer).unwrap();
                    self.source.prepare(cnt);

                    *state = States::FaultMsg;
                }
                States::FaultMsg => {
                    // put fault mesage lenght
                    written += self.source.flush(dst, written);
                    if !self.source.is_empty() {
                        return Ok(written);
                    }

                    // prepare string value itself
                    self.source.prepare(msg.len());
                    *state = States::FaultMsgData;
                }
                States::FaultMsgData => {
                    written += Serializer::copy_next_chunk(
                        dst,
                        written,
                        &mut self.source,
                        &msg.as_bytes(),
                    );
                    *state = States::StackPop;
                }
                States::StackPop => {
                    self.stack.pop();
                }
                _ => return Err("Invalid state"),
            }
        }
        Ok(written)
    }

    fn copy_next_chunk(
        dst: &mut [u8],
        written: usize,
        src_state: &mut Source,
        src: &[u8],
    ) -> usize {
        let write = cmp::min(dst.len() - written, src_state.len - src_state.pos);
        if write > 0 {
            // make slices same size for copy_from_slice
            let d = &mut dst[written..(written + write)];
            let s = &src[src_state.pos..src_state.pos + write];
            d.copy_from_slice(s);
            src_state.pos += write;
        }

        write
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze_slice(slice: &[u8]) {
        println!("first element of the slice: {}", slice[0]);
        println!("the slice has {} elements", slice.len());
    }

    // #[test]
    // fn wire_format() {
    //     let mut buffer: [u8; 256] = [0; 256];

    //     let cnt = write_magic(1u8, &mut buffer).unwrap();

    //     analyze_slice(&buffer[0..5]);
    //     assert_eq!(cnt, 5);

    //     let cnt = write_bool(false, &mut buffer[cnt..]).unwrap();

    //     analyze_slice(&buffer[0..6]);
    //     assert_eq!(cnt, 1);

    //     let cnt = write_int(1024123123, &mut buffer[cnt..]).unwrap();
    //     assert_eq!(cnt, 5);

    //     let cnt = write_double(1024123.123, &mut buffer[cnt..]).unwrap();
    //     assert_eq!(cnt, 9);
    // }

    // #[test]
    // fn serializer() {
    //     let mut serializer = Serializer::new();
    //     let mut buffer: [u8; 256] = [0; 256];

    //     let mut written = 0;
    //     let cnt = serializer.write_call(&mut buffer, "server.stat");
    //     assert_eq!(cnt.is_ok(), true);
    //     written += cnt.unwrap();

    //     serializer.reset();
    //     let val = Value::Int(1224);
    //     let cnt = serializer.write_value(&mut buffer[written..], &val);
    //     assert_eq!(cnt.is_ok(), true);
    //     written += cnt.unwrap();

    //     serializer.reset();
    //     let val = Value::Double(12.24);
    //     let cnt = serializer.write_value(&mut buffer[written..], &val);
    //     assert_eq!(cnt.is_ok(), true);
    //     written += cnt.unwrap();

    //     serializer.reset();
    //     let val = Value::Str(String::from("Ahoj tady string"));
    //     let cnt = serializer.write_value(&mut buffer[written..], &val);
    //     assert_eq!(cnt.is_ok(), true);
    //     written += cnt.unwrap();

    //     serializer.reset();
    //     let val = Value::Array(vec![
    //         Value::Int(1),
    //         Value::Str(String::from("Ahoj tady string")),
    //     ]);
    //     let cnt = serializer.write_value(&mut buffer[written..], &val);
    //     assert_eq!(cnt.is_ok(), true);
    //     written += cnt.unwrap();
    //     println!("Serialized data len: {}", written);
    // }
}
