use crate::constants::*;

enum States {
    Init,
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

pub struct Tokenizer {
    state: States,
}

impl Tokenizer {
    pub fn new() -> Tokenizer {
        Tokenizer {
            state: States::Init,
        }
    }
    
}
