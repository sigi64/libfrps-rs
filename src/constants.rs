/*

The FastRPC protocol 2.1 extension specification

FastRPC 2.1 uses two new integer types (Integer8 positive and Integer8 negative)
instead old integer.

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

***** Integer are zigzag encoded
*/

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
pub const INT_ID: u8 = 0b00001000;
pub const BOOL_ID: u8 = 0b00010000;
pub const DOUBLE_ID: u8 = 0b00011000;
pub const DATETIME_ID: u8 = 0b00101000;
pub const CALL_ID: u8 = 0b01101000;
pub const RESPOSE_ID: u8 = 0b01110000;
pub const FAULT_RESPOSE_ID: u8 = 0b01111000;