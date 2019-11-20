// Binary format type's ids
pub static TYPE_MASK: u8 = 0b11111000;
pub static OCTET_CNT_MASK: u8 = 0b00000111;
pub static U_VINT_ID: u8 = 0b00111000;
pub static VINT_ID: u8 = 0b01000000;
pub static STRING_ID: u8 = 0b00100000;
pub static BIN_ID: u8 = 0b00110000;
pub static NULL_ID: u8 = 0b01100000;
pub static STRUCT_ID: u8 = 0b01010000;
pub static ARRAY_ID: u8 = 0b01011000;
pub static INT32_ID: u8 = 0b00001000;
pub static BOOL_ID: u8 = 0b00010000;
pub static DOUBLE_ID: u8 = 0b00011000;
pub static DATETIME_ID: u8 = 0b00101000;
pub static CALL_ID: u8 = 0b01101000;
pub static RESPOSE_ID: u8 = 0b01110000;
pub static FAULT_RESPOSE_ID: u8 = 0b01111000;