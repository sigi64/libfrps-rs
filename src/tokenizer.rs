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
    Params,
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
        assert!(self.pos + cnt < self.src.len());
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
    state: States,
    buffer: Buffer,

    version_major: u8,
    version_minor: u8,
}

impl Tokenizer {
    pub fn new() -> Tokenizer {
        Tokenizer {
            state: States::Init,
            buffer: Buffer::new(),

            version_major: 0,
            version_minor: 0,
        }
    }

    pub fn parse<T: Callback>(&mut self, src: &[u8], cb: &mut T) -> Result<usize, &'static str> {
        let mut src = SourcePtr::new(src);

        loop {
            match self.state {
                States::Init => {
                    // first 4 bytes is header with magic and version
                    if !self.buffer.consume(4, &mut src) {
                        return Ok(src.consumed());
                    }

                    // check FRPC magic and version
                    if self.buffer.data[0] != 0xca || self.buffer.data[0] != 0x11 {
                        return Err("Invalid magic expected 0xca 0x11");
                    }

                    self.version_major = self.buffer.data[2];
                    self.version_minor = self.buffer.data[3];

                    if !cb.version(self.version_major, self.version_minor) {
                        return Err("Invalid version")
                    }

                    self.state = States::MessageType;
                    self.buffer.reset();
                }

                States::MessageType => {
                    // first byte is message type
                    if !self.buffer.consume(1, &mut src) {
                        return Ok(src.consumed());
                    }

                    match self.buffer.data[0] & TYPE_MASK {
                        CALL_ID => {
                            self.state = States::CallNameSize;
                        }
                        RESPOSE_ID => {
                            self.state = States::Response;
                        }
                        FAULT_RESPOSE_ID => {
                            self.state = States::Response;
                        }
                        _ => return Err("Invalid message type"),
                    }

                    self.buffer.reset();
                }

                States::CallNameSize => {
                    // first byte is method name lenght
                    if !self.buffer.consume(1, &mut src) {
                        return Ok(src.consumed());
                    }

                    let length: usize = self.buffer.data[0] as usize;
                    if length == 0 {
                        return Err("Invalid lenght of method name");
                    }

                    self.state = States::CallName((length, 0));
                    self.buffer.reset();
                }

                States::CallName((lenght, mut procesed)) => {
                    let avail = cmp::min(lenght - procesed, src.available());
                    if avail == 0 {
                        return Ok(src.available());
                    }

                    let run = cb.call(str::from_utf8(src.data(avail)).unwrap(), avail, lenght);

                    procesed += avail;
                    src.advance(avail);

                    if !run || lenght != procesed {
                        self.state = States::CallName((lenght, procesed));
                        return Ok(src.available());
                    }

                    self.state = States::Params;
                }
                States::Params => {}

                States::Response => {}

                States::FaultInit => {}
            }
        }

        Ok(0)
    }
}
