/*
======================================================================
The FastRPC protocol 1.0 specification
FastRPC uses 8 data types: 6 scalar (integer, boolean, double, string, date and binary) and 2 structured (struct and array). It also has 3 non-data types (method call, method response and fault response). These types precisely mirror XML-RPC'2 data types.
Every (non-)data type begins with just one octet. This octet identifies the type and also can hold some additional type info and it can even hold the value itself. Integer values (integer data type and size of multioctet types) are encoded in 1, 2 3 or 4 octets and the additional type info says the number of octets used.
Type: 5b 	Add: 3b
**** Integer
00001 	XXX 	data (1-4 means 1-4 octets)
Signed integer value stored in little endian convention. Additional type info says how much octets follows (1, 2, 3 or 4).
**** Boolean
00010 	00V
Value is stored in the least significant bit of type definition octet (bit marked as V).
**** Double
00011 	000 	data (8 octets)
Floating-point number in double precision as specified by the IEEE 754 standard. The additional type info should by always zero.
**** String
00100 	XXX 	data-size (1-4 means 1-4 octets) 	data (data-size octets)
UTF-8 encoded not null terminated strings without any escaping. Additonal info specifies length of data-size field. Data-size field is stored in little endian octet order.
**** Datetime
00101 	000 	zone (8b) 	unix timestamp (32b) 	week day (3b) 	sec (6b) 	min (6b) 	hour (5b) 	day (5b) 	month (4b) 	year (11b)
year is offset (0-2047) from AD 1600 month is 1-12 hour is 0-23 min is 0-59 sec is 0-59 (leap seconds not implemented) week day is 0 (sunday) - 6 (saturday) unix timestamp is number of seconds from the Epox (1970-Jan-01 00:00:00 UTC) zone is specified as relative number of localtime hour quarters ((-128..+12) * 15 min) added to UTC
Binary octet order:
0 	1 	2 	3 	4 	5 	6 	7 	8 	9 	10
0010 1000 	ZZZZ ZZZZ 	UUUU UUUU 	UUUU UUUU 	UUUU UUUU 	UUUU UUUU 	SSSS SWWW 	HMMM MMMS 	dddd HHHH 	yyym mmmd 	yyyy yyyy
Legend (bit index numbers start at 0, ranges are inclusive):
    Z - timezone
    U - unix timestamp
    S - seconds - split: first lowest bits 4..0, then bit 5
    W - weekday
    H - hours - split: first lowest bit 0, then bits 5..1
    M - minutes
    d - day of month - split: first lowest bits 3..0, then bit 4
    m - month
    y - year - split: first the lowest bits 2..0, then the rest 10..3
All values are stored in little endian order. Where split, the lowest bits take precedence. Whole object always holds local time of specified timezone except the unix timestamp which is always in UTC. Unix timestamp is set to -1 (invalid) for dates outside Epoch.
**** Binary
00110 	XXX 	data-size (1-4 means 1-4 octets) 	data (data-size octets)
Binary data are sent as they are. Exact number of octets is stored data-size field. No encoding is used. This type maps to Base64 data type of XML-RPC. Data-size field is stored in little endian octet order.
**** Struct
01010 	XXX 	num-members (1-4 means 1-4 octets)
Followed by num-members times:
name-size (1 octet) 	name (1-255 octets) 	VALUE
Member name is encoded in the UTF-8 encoding.
**** Array
01011 	XXX 	num-items (1-4 means 1-4 octets)
Followed by num-items times VALUE
Non-data types
Every non-data type represents complex data structure returned by the server or sent by the client. Data always start with magic ("CALL" in hex) and protocol version (major/minor octet pair)
0xCA 	0x11 	version major 	version minor
**** Method call
01101 	000 	name-size (1 octet) 	name (1-255 octets) 	PARAMETERS
Method parameters are encoded as a series of values although XML-RPC has a distinct data type for them. Method name should be in the UTF-8 encoding. Additional type info should be zero.
**** Method reponse
01110 	000 	VALUE
Method is allowed to return just one value. Additional type info should be zero.
Fault reponse
01111 	000 	INT (fault number) 	STRING (fault message)
Additional type info should be zero.
======================================================================
The FastRPC protocol 2.1 extension specification
FastRPC 2.1 uses two new integer types (Integer8 positive and Integer8 negative)
instead old integer.
The main change, compared to protocol 1.0, is the use of Add field to encode integer size - Integer values (integer data type and size of multioctet types) are encoded in 1 - 8 octets and the additional type info (000 - 111 [0-7]) says the number of octets used (1 - 8), as depicted here:
Type: 5 bits 	NLEN (3 bits) 	LEN (NLEN+1 octets)
LEN is stored as an integer with all the zero bytes starting from MSB being cut off.
Example: number 256 is stored as 0x39 0x00 0x01 in stream.
**** Integer values
    (integer data type and size of multioctet types) are encoded
    in 1 - 8 octets and the additional type info (000 - 111 [0-7])
    says the number of octets used (1 - 8).
    This is the main change.
Type: 5b Add: 3b
**** Integer8 positive
     [00111 XXX]  [data (0-7 means 1-8 octets)]
Absolute integer value stored in little endian convention.
Additional type info says how much octets follows (1 - 8).
**** Integer8 negative
     [01000 XXX] [data (0-7 means 1-8 octets)]
Absolute integer value stored in little endian convention.
Additional type info says how much octets follows (1 - 8).
**** String
    [00100 XXX] [data-size (0-7 means 1-8 octets) data (data-size octets)]
UTF-8 encoded not null terminated strings without any escaping.
Additonal info specifies length of data-size field.
**** Binary
    [00110 XXX] [data-size] (0-7 means 1-8 octets)data (data-size octets)
Binary data are sent as they are. Exact number of octets is stored
data-size field. No encoding is used. This type maps to Base64 data type
of XML-RPC.
**** Null
    [01100 000]
Null data type, carrying no value.
**** Boolean
    [00010 00V]
Value is stored in the least significant bit of type definition octet
(bit marked as V).
**** Double
    [00011 000] [data (8 octets)]
Floating-point number in double precision as specified by the IEEE 754 standard.
The additional type info should by always zero.
**** Datetime
    [00101 000] [zone (8b)] [unix timestamp (32b)] [week day (3b)]
    [sec (6b)] [min (6b)] [hour (5b)]
    [day (5b)] [month (4b)] [year (11b)]
    year is offset (0-2047) from AD 1600
    month is 1-12
    hour is 0-23
    min is 0-59
    sec is 0-59 (leap seconds not implemented)
    week dat is 0 (sunday) - 6 (saturday)
    unix timestamp is number of seconds from the Epox (1970-Jan-01 00:00:00 UTC)
    zone is specified as relative number of localtime hour quarters
    ((-128..+12) * 15 min) added to UTC
Unix timestamp is stored in little endian order like integer.
Other values are in the network order. Whole object always holds local time
of specified timezone except the unix timestamp which is always in UTC.
Unix timestamp is set to -1 (invalid) for dates outside Epoch.
**** Struct
    [01010 XXX] [num-members (0-7 means 1-8 octets)]
    num-members * [name-size (1 octet)name (1-255 octets)] [DATATYPE]
Member name is encoded in the UTF-8 encoding.
**** Array
    [01011 XXX] [num-items (0-7 means 1-8 octets)] num-items * [DATATYPE]
It also has 3 non-data types (method call, method response and fault response).
These types precisely mirrors XML-RPC'2 data types.
Every non-data type represents complex data structure returned by the server
or sent by the client.
Data always start with magic ("CALL" in hex) and protocol version
(major/minor octet pair)
    [0xCA 0x11 version_major version_minor]
**** Method call
    [01101 000] [name-size (1 octet)] [name (1-255 octets)] [PARAMETERS]
Method parameters are encoded as a series of values although XML-RPC
has a distinct data type for them. Method name should be in the UTF-8 encoding.
Additional type info should be zero.
**** Method reponse
    [01110 000] [DATATYPE]
Method is allowed to return just one value. Additional type info should be zero.
**** Fault reponse
    [01111 000] [INT (fault number)] [STRING (fault message)]
Additional type info should be zero.
======================================================================
FastRPC 3.0
  introduces two changes to the protocol - integer unification and timestamp field enlargement in DateTime type.
The integer value is unified again under the originally used type 00001b, but uses zig-zag encoding for the value to avoid long octet streams for negative values. The Integer8 types are deprecated, still handled, but should not be present in the stream.
ZigZag encoding uses LSB for sign storage, the value of the signed integer is shifted left by one bit, and bitwise negated in case of negative integers.
since protocol version 3, we use full (64bit) timestamp time_t in the packed data
**** Datetime
    [00101 000] [zone (8b)] [unix timestamp (64b)] [week day (3b)]
-    [sec (6b)] [min (6b)] [hour (5b)]
    [day (5b)] [month (4b)] [year (11b)]
    year is offset (0-2047) from AD 1600
    month is 1-12
    hour is 0-23
    min is 0-59
    sec is 0-59 (leap seconds not implemented)
    week dat is 0 (sunday) - 6 (saturday)
    unix timestamp is number of seconds from the Epox (1970-Jan-01 00:00:00 UTC)
    zone is specified as relative number of localtime hour quarters
    ((-128..+12) * 15 min) added to UTC
Unix timestamp is stored in little endian order like integer.
Other values are in the network order. Whole object always holds local time
of specified timezone except the unix timestamp which is always in
****  Integer
00001 	NLEN 	data (zigzag encoded signed integer, NLEN + 1 octets)
Value 	Encoded Value
    0 	0
    1 	2
    -1 	1
    2 	4
    -2 	3
    3 	6
    -3 	5
    ... 	...
*/

// Binary format type's ids
pub const TYPE_MASK: u8 = 0b11111000;
pub const OCTET_CNT_MASK: u8 = 0b00000111;
pub const U_VINT_ID: u8 = 0b00111000;  // ver 2.1
pub const VINT_ID: u8 = 0b01000000;    // ver 2.1 
pub const STRING_ID: u8 = 0b00100000;  
pub const BIN_ID: u8 = 0b00110000;
pub const NULL_ID: u8 = 0b01100000;
pub const STRUCT_ID: u8 = 0b01010000;
pub const ARRAY_ID: u8 = 0b01011000;
pub const INT_ID: u8 = 0b00001000;   // 1.0 & 3.0
pub const BOOL_ID: u8 = 0b00010000;
pub const DOUBLE_ID: u8 = 0b00011000;
pub const DATETIME_ID: u8 = 0b00101000;
pub const CALL_ID: u8 = 0b01101000;
pub const RESPOSE_ID: u8 = 0b01110000;
pub const FAULT_RESPOSE_ID: u8 = 0b01111000;
pub const FRPS_DATA_ID: u8 = 0b00000000; 

pub const MAX_STR_LENGTH: usize = 1024 * 1024 * 1024; // 1 GB
pub const MAX_BIN_LENGTH: usize = 1024 * 1024 * 1024; // 1 GB
pub const MAX_ARRAY_LENGTH: usize = 1024 * 1024; // 1 mil members
pub const MAX_STRUCT_LENGTH: usize = 1024 * 1024; // 1 mil members

