//! The bytes of a WebAssembly module: LEB128, the value types and the instructions that the
//! translator writes.

/// A WebAssembly value type.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Vt {
    I32,
    I64,
    F32,
    F64,
}

impl Vt {
    pub(crate) fn byte(self) -> u8 {
        match self {
            Vt::I32 => 0x7f,
            Vt::I64 => 0x7e,
            Vt::F32 => 0x7d,
            Vt::F64 => 0x7c,
        }
    }
}

pub(crate) fn uleb(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

pub(crate) fn sleb(out: &mut Vec<u8>, mut value: i64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        let done = (value == 0 && byte & 0x40 == 0) || (value == -1 && byte & 0x40 != 0);
        if done {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// A `u32` in exactly five bytes, which a relocation can overwrite in place.
pub(crate) fn uleb_padded(out: &mut Vec<u8>, value: u32) {
    let mut value = value;
    for index in 0..5 {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        out.push(if index < 4 { byte | 0x80 } else { byte });
    }
}

/// An `i32` in exactly five bytes.
pub(crate) fn sleb_padded(out: &mut Vec<u8>, value: i32) {
    let mut value = i64::from(value);
    for index in 0..5 {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        out.push(if index < 4 { byte | 0x80 } else { byte & 0x7f });
    }
}

pub(crate) fn name(out: &mut Vec<u8>, text: &str) {
    uleb(out, text.len() as u64);
    out.extend_from_slice(text.as_bytes());
}

/// A section: its id, the length of its payload and the payload.
pub(crate) fn section(out: &mut Vec<u8>, id: u8, payload: &[u8]) {
    out.push(id);
    uleb(out, payload.len() as u64);
    out.extend_from_slice(payload);
}

pub(crate) mod op {
    pub const UNREACHABLE: u8 = 0x00;
    pub const BLOCK: u8 = 0x02;
    pub const LOOP: u8 = 0x03;
    pub const IF: u8 = 0x04;
    pub const ELSE: u8 = 0x05;
    pub const END: u8 = 0x0b;
    pub const BR: u8 = 0x0c;
    pub const BR_TABLE: u8 = 0x0e;
    pub const CALL: u8 = 0x10;
    pub const CALL_INDIRECT: u8 = 0x11;
    pub const SELECT: u8 = 0x1b;
    pub const LOCAL_GET: u8 = 0x20;
    pub const LOCAL_SET: u8 = 0x21;
    pub const LOCAL_TEE: u8 = 0x22;
    pub const GLOBAL_GET: u8 = 0x23;
    pub const GLOBAL_SET: u8 = 0x24;
    pub const I32_LOAD: u8 = 0x28;
    pub const I64_LOAD: u8 = 0x29;
    pub const F32_LOAD: u8 = 0x2a;
    pub const F64_LOAD: u8 = 0x2b;
    pub const I32_LOAD8_U: u8 = 0x2d;
    pub const I32_LOAD16_U: u8 = 0x2f;
    pub const I32_STORE: u8 = 0x36;
    pub const I64_STORE: u8 = 0x37;
    pub const F32_STORE: u8 = 0x38;
    pub const F64_STORE: u8 = 0x39;
    pub const I32_STORE8: u8 = 0x3a;
    pub const I32_STORE16: u8 = 0x3b;
    pub const MEMORY_SIZE: u8 = 0x3f;
    pub const MEMORY_GROW: u8 = 0x40;
    pub const I32_CONST: u8 = 0x41;
    pub const I64_CONST: u8 = 0x42;
    pub const F32_CONST: u8 = 0x43;
    pub const F64_CONST: u8 = 0x44;
    pub const I32_EQZ: u8 = 0x45;
    // The comparisons of an integer type follow `EQ` in the order eq, ne, lt_s, lt_u, gt_s, gt_u,
    // le_s, le_u, ge_s, ge_u; `icmp` adds the offset of the predicate.
    pub const I32_EQ: u8 = 0x46;
    pub const I32_NE: u8 = 0x47;
    pub const I32_LT_S: u8 = 0x48;
    pub const I32_LT_U: u8 = 0x49;
    pub const I32_GT_S: u8 = 0x4a;
    pub const I32_GT_U: u8 = 0x4b;
    pub const I64_EQZ: u8 = 0x50;
    pub const I64_EQ: u8 = 0x51;
    pub const I64_NE: u8 = 0x52;
    pub const I64_LT_S: u8 = 0x53;
    pub const I64_LT_U: u8 = 0x54;
    pub const I64_GT_S: u8 = 0x55;
    pub const I64_GT_U: u8 = 0x56;
    pub const F32_EQ: u8 = 0x5b;
    pub const F32_NE: u8 = 0x5c;
    pub const F32_LT: u8 = 0x5d;
    pub const F32_GT: u8 = 0x5e;
    pub const F32_LE: u8 = 0x5f;
    pub const F32_GE: u8 = 0x60;
    pub const F64_EQ: u8 = 0x61;
    pub const F64_NE: u8 = 0x62;
    pub const F64_LT: u8 = 0x63;
    pub const F64_GT: u8 = 0x64;
    pub const F64_LE: u8 = 0x65;
    pub const F64_GE: u8 = 0x66;
    pub const I32_CLZ: u8 = 0x67;
    pub const I32_CTZ: u8 = 0x68;
    pub const I32_POPCNT: u8 = 0x69;
    pub const I32_ADD: u8 = 0x6a;
    pub const I32_SUB: u8 = 0x6b;
    pub const I32_MUL: u8 = 0x6c;
    pub const I32_DIV_S: u8 = 0x6d;
    pub const I32_DIV_U: u8 = 0x6e;
    pub const I32_REM_S: u8 = 0x6f;
    pub const I32_REM_U: u8 = 0x70;
    pub const I32_AND: u8 = 0x71;
    pub const I32_OR: u8 = 0x72;
    pub const I32_XOR: u8 = 0x73;
    pub const I32_SHL: u8 = 0x74;
    pub const I32_SHR_S: u8 = 0x75;
    pub const I32_SHR_U: u8 = 0x76;
    pub const I32_ROTL: u8 = 0x77;
    pub const I32_ROTR: u8 = 0x78;
    pub const I64_CLZ: u8 = 0x79;
    pub const I64_CTZ: u8 = 0x7a;
    pub const I64_POPCNT: u8 = 0x7b;
    pub const I64_ADD: u8 = 0x7c;
    pub const I64_SUB: u8 = 0x7d;
    pub const I64_MUL: u8 = 0x7e;
    pub const I64_DIV_S: u8 = 0x7f;
    pub const I64_DIV_U: u8 = 0x80;
    pub const I64_REM_S: u8 = 0x81;
    pub const I64_REM_U: u8 = 0x82;
    pub const I64_AND: u8 = 0x83;
    pub const I64_OR: u8 = 0x84;
    pub const I64_XOR: u8 = 0x85;
    pub const I64_SHL: u8 = 0x86;
    pub const I64_SHR_S: u8 = 0x87;
    pub const I64_SHR_U: u8 = 0x88;
    pub const I64_ROTL: u8 = 0x89;
    pub const I64_ROTR: u8 = 0x8a;
    pub const F32_ABS: u8 = 0x8b;
    pub const F32_NEG: u8 = 0x8c;
    pub const F32_CEIL: u8 = 0x8d;
    pub const F32_FLOOR: u8 = 0x8e;
    pub const F32_TRUNC: u8 = 0x8f;
    pub const F32_NEAREST: u8 = 0x90;
    pub const F32_SQRT: u8 = 0x91;
    pub const F32_ADD: u8 = 0x92;
    pub const F32_SUB: u8 = 0x93;
    pub const F32_MUL: u8 = 0x94;
    pub const F32_DIV: u8 = 0x95;
    pub const F32_MIN: u8 = 0x96;
    pub const F32_MAX: u8 = 0x97;
    pub const F32_COPYSIGN: u8 = 0x98;
    pub const F64_ABS: u8 = 0x99;
    pub const F64_NEG: u8 = 0x9a;
    pub const F64_CEIL: u8 = 0x9b;
    pub const F64_FLOOR: u8 = 0x9c;
    pub const F64_TRUNC: u8 = 0x9d;
    pub const F64_NEAREST: u8 = 0x9e;
    pub const F64_SQRT: u8 = 0x9f;
    pub const F64_ADD: u8 = 0xa0;
    pub const F64_SUB: u8 = 0xa1;
    pub const F64_MUL: u8 = 0xa2;
    pub const F64_DIV: u8 = 0xa3;
    pub const F64_MIN: u8 = 0xa4;
    pub const F64_MAX: u8 = 0xa5;
    pub const F64_COPYSIGN: u8 = 0xa6;
    pub const I32_WRAP_I64: u8 = 0xa7;
    pub const I64_EXTEND_I32_S: u8 = 0xac;
    pub const I64_EXTEND_I32_U: u8 = 0xad;
    pub const F32_CONVERT_I32_S: u8 = 0xb2;
    pub const F32_CONVERT_I32_U: u8 = 0xb3;
    pub const F32_CONVERT_I64_S: u8 = 0xb4;
    pub const F32_CONVERT_I64_U: u8 = 0xb5;
    pub const F32_DEMOTE_F64: u8 = 0xb6;
    pub const F64_CONVERT_I32_S: u8 = 0xb7;
    pub const F64_CONVERT_I32_U: u8 = 0xb8;
    pub const F64_CONVERT_I64_S: u8 = 0xb9;
    pub const F64_CONVERT_I64_U: u8 = 0xba;
    pub const F64_PROMOTE_F32: u8 = 0xbb;
    pub const I32_REINTERPRET_F32: u8 = 0xbc;
    pub const I64_REINTERPRET_F64: u8 = 0xbd;
    pub const F32_REINTERPRET_I32: u8 = 0xbe;
    pub const F64_REINTERPRET_I64: u8 = 0xbf;
    /// The prefix of the saturating conversions, `memory.copy` and `memory.fill`.
    pub const PREFIX: u8 = 0xfc;
    pub const MEMORY_COPY: u8 = 0x0a;
    pub const MEMORY_FILL: u8 = 0x0b;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leb128_matches_the_specification() {
        let mut out = Vec::new();
        uleb(&mut out, 624485);
        assert_eq!(out, [0xe5, 0x8e, 0x26]);
        out.clear();
        sleb(&mut out, -123456);
        assert_eq!(out, [0xc0, 0xbb, 0x78]);
        out.clear();
        sleb(&mut out, 63);
        sleb(&mut out, -64);
        sleb(&mut out, 64);
        assert_eq!(out, [0x3f, 0x40, 0xc0, 0x00]);
        out.clear();
        uleb_padded(&mut out, 3);
        sleb_padded(&mut out, -2);
        assert_eq!(
            out,
            [0x83, 0x80, 0x80, 0x80, 0x00, 0xfe, 0xff, 0xff, 0xff, 0x7f]
        );
    }
}
