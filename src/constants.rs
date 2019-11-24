// Binary format type's ids
pub const TYPE_MASK: u8 = 0b11111000;
pub const OCTET_CNT_MASK: u8 = 0b00000111;
pub const U_VINT_ID: u8 = 0b00111000;
pub const VINT_ID: u8 = 0b01000000;
pub const STRING_ID: u8 = 0b00100000;
pub const BIN_ID: u8 = 0b00110000;
pub const NULL_ID: u8 = 0b01100000;
pub const STRUCT_ID: u8 = 0b01010000;
pub const ARRAY_ID: u8 = 0b01011000;
pub const INT32_ID: u8 = 0b00001000;
pub const BOOL_ID: u8 = 0b00010000;
pub const DOUBLE_ID: u8 = 0b00011000;
pub const DATETIME_ID: u8 = 0b00101000;
pub const CALL_ID: u8 = 0b01101000;
pub const RESPOSE_ID: u8 = 0b01110000;
pub const FAULT_RESPOSE_ID: u8 = 0b01111000;