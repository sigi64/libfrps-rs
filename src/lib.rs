
use std::convert::TryInto;
use std::convert::TryFrom;
use byteorder::{ByteOrder, LittleEndian};
use std::collections::HashMap;

// Binary format type's ids
static TYPE_MASK:u8        = 0b11111000;
static OCTET_CNT_MASK:u8   = 0b00000111;
static U_VINT_ID:u8        = 0b00111000;
static VINT_ID:u8          = 0b01000000;
static STRING_ID:u8        = 0b00100000;
static BIN_ID:u8           = 0b00110000;
static NULL_ID:u8          = 0b01100000;
static STRUCT_ID:u8        = 0b01010000;
static ARRAY_ID:u8         = 0b01011000;
static INT32_ID:u8         = 0b00001000;
static BOOL_ID:u8          = 0b00010000;
static DOUBLE_ID:u8        = 0b00011000;
static DATETIME_ID:u8      = 0b00101000;
static CALL_ID:u8          = 0b01101000;
static RESPOSE_ID:u8       = 0b01110000;
static FAULT_RESPOSE_ID:u8 = 0b01111000;

static ZERO:u64 = 0;
static ALLONES:u64 = !ZERO;
static INT8_MASK:u64 = ALLONES << 8;
static INT16_MASK:u64 = ALLONES << 16;
static INT24_MASK:u64 = ALLONES << 24;
static INT32_MASK:u64 = ALLONES << 32;
static INT40_MASK:u64 = ALLONES << 40;
static INT48_MASK:u64 = ALLONES << 48;
static INT56_MASK:u64 = ALLONES << 56;

fn get_octets(number:u64) -> usize {
    if (number & INT8_MASK)  == 0 { return 0 } // one byte will be enough
    if (number & INT16_MASK) == 0 { return 1 }
    if (number & INT24_MASK) == 0 { return 2 }
    if (number & INT32_MASK) == 0 { return 3 }
    if (number & INT40_MASK) == 0 { return 4 }
    if (number & INT48_MASK) == 0 { return 5 }
    if (number & INT56_MASK) == 0 { return 6 }
    return 7;
}

struct DateTime {
    unix_time: u64,
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    min: u8,
    sec: u8,
    week_day: u8,
    time_zone: i16 // as difference between UTC and localtime in seconds
}

/** Writes protocol header and message type
 * @return Number of bytes written or zero when there is not enough room
 */
fn write_magic(msg_type:u8, dst: &mut [u8]) -> Result<usize, &str> {
    if dst.len() < 5 { return Err("not enought space") }
    
    dst[0] = 0xCA;
    dst[1] = 0x11;
    dst[2] = 2; // FRPC_MAJOR_VERSION
    dst[3] = 1; // FRPC_MINOR_VERSION
    dst[4] = msg_type;
    Ok(5)
}

/** Writes tag and bool value */
fn write_bool(val:bool, dst: &mut [u8]) -> Result<usize, &str> {
    if dst.len() < 1 { return Err("not enought space") }

    dst[0] = BOOL_ID | (if val { 1u8 } else { 0u8 });
    Ok(1)
}

/** Writes tag and null value */
fn write_null(dst: &mut [u8]) -> Result<usize, &str> {
    if dst.len() < 1 { return Err("not enought space") }
    dst[0] = NULL_ID;
    Ok(1)
}

/** Writes tag and integer value */
fn write_int(val:i64, dst: &mut[u8]) -> Result<usize, &str> {
    let octets = if val >= 0 { 
        get_octets(u64::try_from(val).unwrap()) 
    } else { 
        get_octets(u64::try_from(-val).unwrap()) 
    };

    if (dst.len() < octets + 2) { return Err("not enought space") }
    
    if val >= 0 {
        dst[0] = U_VINT_ID | u8::try_from(octets).unwrap();
        LittleEndian::write_i64(&mut dst[1 ..], val);
    } else {
        dst[0] = VINT_ID | u8::try_from(octets).unwrap();
        LittleEndian::write_i64(&mut dst[1 ..], -val);
    }
    Ok(octets + /*header*/ 1 + /*first byte*/1)
}

/** Writes tag and double value */
fn write_double(val:f64, dst: &mut [u8]) -> Result<usize, &str> {
    if dst.len() < 9 { return Err("not enought space") }

    dst[0] = DOUBLE_ID;
    LittleEndian::write_f64(&mut dst[1 ..], val);
    Ok(9)
}

/** Writes tag and datetime value */
fn write_datetime(val:DateTime, dst: &mut [u8]) -> Result<usize, &str> {
    if dst.len() < 11 { return Err("not enought space") }

    dst[0] = DATETIME_ID;
    dst[1] = (val.time_zone / 60i16 / 15i16).try_into().unwrap();
     
    // For backward compatibility with fastrpc unixtime was 32 bit only
    let unix_time = if val.unix_time & INT32_MASK > 0 {
        u32::max_value()
    } else { 
        val.unix_time.try_into().unwrap() 
    };

    LittleEndian::write_u32(&mut dst[2 ..], unix_time);

    // 6
    dst[6] = val.week_day;
    dst[7] = val.sec;
    dst[8] = val.min;
    dst[9] = val.hour;
    dst[10] = val.day;
    dst[11] = val.month;

    let year = if (val.year <= 1600) { 0 } else {val.year - 1600};

    LittleEndian::write_u16(&mut dst[12 ..], year);
    Ok(14)
}

/** Writes tag and length for string, binary, array and struct types */
fn write_head(frpsType:u8, size: usize, dst: &mut [u8]) -> Result<usize, &str> {
    let octets = get_octets(size.try_into().unwrap());

    if dst.len() < (octets + 2) { return Err("not enought space") }
    
    dst[0] = frpsType | u8::try_from(octets).unwrap();;

    // this works only for little-endian systems
    LittleEndian::write_u64(&mut dst[1 ..], size.try_into().unwrap());

    Ok(octets + /*header*/ 1 + /*first byte*/1)
}

/** Writes head of struct key */
fn write_key_head(size:usize, dst: &mut Vec<u8>) -> Result<usize, &str> {
    if dst.len() < 1 { return Err("not enought space") }
    dst[0] = size.try_into().unwrap();
    Ok(1)
}

enum Value {
    Int(i64),
    Str(String),
    Null,
    DateTime(DateTime),
    Struct(HashMap<String, Value>),
    Array(Vec<Value>),
    Double(f64),
    Bool(bool)
}

fn serialize(buf: &mut [u8], val: &Value) -> Result<usize, String> {
    
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze_slice(slice: &[u8]) {
        println!("first element of the slice: {}", slice[0]);
        println!("the slice has {} elements", slice.len());
    }

    #[test]
    fn it_works() {

        let mut buffer:[u8; 256] = [0; 256];

        let cnt = write_magic(1u8, &mut buffer);

        analyze_slice(&buffer[0 .. 5]);
        assert_eq!(cnt, 5);

        let cnt = write_bool(false, &mut buffer[cnt .. ]);
        
        analyze_slice(&buffer[0 .. 6]);
        assert_eq!(cnt, 1);

        let cnt = write_int(1024123123, &mut buffer[cnt ..]);
        assert_eq!(cnt, 5);

        let cnt = write_double(1024123.123, &mut buffer[cnt ..]);
        assert_eq!(cnt, 9);
    }
}
 

