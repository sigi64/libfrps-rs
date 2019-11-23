use std::cmp;

use crate::constants::*;

enum States {
    Init,
    MessageType,
}

pub trait Callback {
    /** Parsing always stop after this callback return. */
    fn error();

    /* Stop on false, continue on true */
    fn version() -> bool;

    /* Stop on false, continue on true */
    fn call() -> bool;

    /* Stop on false, continue on true */
    fn response() -> bool;

    /* Stop on false, continue on true */
    fn fault() -> bool;

    /* Stop on false, continue on true */
    fn stream_data() -> bool;

    /* Stop on false, continue on true */
    fn null() -> bool;

    /* Stop on false, continue on true */
    fn boolean() -> bool;
    fn double_number() -> bool;
    fn datetime() -> bool;
    fn binary() -> bool;
    fn push_array() -> bool;
    fn push_struct() -> bool; // pushMap
    fn map_key() -> bool;

    fn pop_context();
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
}

impl Tokenizer {
    pub fn new() -> Tokenizer {
        Tokenizer {
            state: States::Init,
            buffer: Buffer::new(),
        }
    }

    pub fn parse<T: Callback>(&mut self, src: &[u8], cb: T) -> Result<usize, &'static str> {
        let mut src = SourcePtr::new(src);

        loop {
            match self.state {
                States::Init => {
                    if !self.buffer.consume(4, &mut src) {
                        return Ok(src.consumed());
                    }

                    // check FRPC magic and version
                    if self.buffer.data[0] != 0xca || self.buffer.data[0] != 0x11 {
                        return Err("Invalid magic expected 0xca 0x11");
                    }

                    self.state = States::MessageType;
                    self.buffer.reset();
                }

                States::MessageType => {}
            }
        }

        Ok(0)
    }
}
