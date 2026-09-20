use rustc_apfloat::ieee::{Double, Half, Quad, Single};
use rustc_apfloat::{Float, Round, Status};

use crate::check::Type;
use crate::diagnostic::{Diagnostic, Span};

pub const NUMERIC_NAMES: [&str; 19] = [
    "i8", "i16", "i32", "i64", "i128", "i8u", "i16u", "i32u", "i64u", "i128u", "f16", "f32", "f64",
    "f128", "d32", "d64", "d128", "ubyte", "byte",
];

pub fn is_numeric_name(name: &str) -> bool {
    NUMERIC_NAMES.contains(&name)
}

pub fn primitive(name: &str) -> Option<Type> {
    Some(match name {
        "bool" => Type::Bool,
        "unit" => Type::Unit,
        "string" => Type::String,
        "byte" => Type::Integer(8, true),
        "ubyte" => Type::Integer(8, false),
        _ if name.starts_with('i') && is_numeric_name(name) => Type::Integer(
            name[1..].trim_end_matches('u').parse().unwrap(),
            !name.ends_with('u'),
        ),
        _ if name.starts_with('f') && is_numeric_name(name) => {
            Type::Binary(name[1..].parse().unwrap())
        }
        _ if name.starts_with('d') && is_numeric_name(name) => {
            Type::Decimal(name[1..].parse().unwrap())
        }
        _ => return None,
    })
}

pub fn literal_parts(text: &str) -> (&str, Option<&str>) {
    for name in NUMERIC_NAMES {
        if text.starts_with("0x") && name.starts_with(['f', 'd']) {
            continue;
        }
        if let Some(number) = text.strip_suffix(name) {
            return (number, Some(name));
        }
    }
    (text, None)
}

fn binary_bits<F: Float>(text: &str, span: Span) -> Result<u128, Diagnostic> {
    let value = F::from_str_r(text, Round::NearestTiesToEven)
        .map_err(|_| Diagnostic::new("E1009", "invalid floating-point literal", span))?;
    if value.status.contains(Status::OVERFLOW) || !value.value.is_finite() {
        return Err(Diagnostic::new(
            "E1009",
            "floating-point literal is out of range",
            span,
        ));
    }
    Ok(value.value.to_bits())
}

pub fn float_literal(text: &str, ty: &Type, span: Span) -> Result<String, Diagnostic> {
    Ok(match ty {
        Type::Binary(16) => binary_bits::<Half>(text, span)?.to_string(),
        Type::Binary(32) => {
            let bits = binary_bits::<Single>(text, span)? as u32;
            format!("0x{:016X}", (f32::from_bits(bits) as f64).to_bits())
        }
        Type::Binary(64) => format!("0x{:016X}", binary_bits::<Double>(text, span)?),
        Type::Binary(128) => binary_bits::<Quad>(text, span)?.to_string(),
        Type::Decimal(bits) => decimal_literal(text, *bits, span)?.to_string(),
        _ => unreachable!("numeric literal type checked"),
    })
}

pub fn decimal_format(bits: u16) -> (usize, i32, i32, u32) {
    match bits {
        32 => (7, -101, 90, 23),
        64 => (16, -398, 369, 53),
        128 => (34, -6176, 6111, 113),
        _ => unreachable!("decimal width checked"),
    }
}

fn decimal_literal(text: &str, bits: u16, span: Span) -> Result<u128, Diagnostic> {
    let error = || Diagnostic::new("E1009", "decimal literal is out of range", span);
    let (precision, minimum, maximum, coefficient_bits) = decimal_format(bits);
    let (mantissa, exponent) = match text.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().map_err(|_| error())?),
        None => (text, 0),
    };
    let fractional = mantissa.split_once('.').map_or(0, |(_, part)| part.len());
    let digits = mantissa.replace('.', "");
    let digits = digits.trim_start_matches('0');
    let mut exponent = exponent.checked_sub(fractional as i32).ok_or_else(error)?;
    if digits.is_empty() {
        return Ok(((-minimum) as u128) << coefficient_bits);
    }
    let discard = (digits.len() as i64 - precision as i64)
        .max(i64::from(minimum) - i64::from(exponent))
        .max(0);
    let kept = (digits.len() as i64 - discard).max(0) as usize;
    let mut coefficient = if kept == 0 {
        0
    } else {
        digits[..kept].parse::<u128>().map_err(|_| error())?
    };
    if discard > 0 && discard <= digits.len() as i64 {
        let round = digits.as_bytes()[kept];
        let sticky = digits[kept + 1..].bytes().any(|byte| byte != b'0');
        if round > b'5' || (round == b'5' && (sticky || coefficient & 1 != 0)) {
            coefficient += 1;
        }
    }
    exponent = (i64::from(exponent) + discard)
        .try_into()
        .map_err(|_| error())?;
    if coefficient == 10u128.pow(precision as u32) {
        coefficient /= 10;
        exponent += 1;
    }
    if coefficient == 0 {
        exponent = exponent.clamp(minimum, maximum);
    }
    while exponent > maximum && coefficient < 10u128.pow(precision as u32 - 1) {
        coefficient *= 10;
        exponent -= 1;
    }
    if exponent > maximum {
        return Err(error());
    }
    let biased = (exponent - minimum) as u128;
    Ok(
        if bits != 128 && coefficient >= (1u128 << coefficient_bits) {
            (3u128 << (bits - 3))
                | (biased << (coefficient_bits - 2))
                | (coefficient & ((1u128 << (coefficient_bits - 2)) - 1))
        } else {
            (biased << coefficient_bits) | coefficient
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_decimal_and_binary_literals_without_binary64_intermediates() {
        assert_eq!(literal_parts("230ubyte"), ("230", Some("ubyte")));
        let span = Span::default();
        assert_eq!(
            float_literal(
                "1.0000000000000000000000000000000002",
                &Type::Binary(128),
                span
            )
            .unwrap(),
            ((16383u128 << 112) + 1).to_string()
        );
        assert_eq!(
            float_literal("1.0004882812500000000001", &Type::Binary(16), span).unwrap(),
            "15361"
        );
        assert_eq!(
            decimal_literal("1.2345665", 32, span).unwrap(),
            decimal_literal("1.234566", 32, span).unwrap()
        );
        assert!(decimal_literal("1e96", 32, span).is_ok());
        assert!(decimal_literal("1e97", 32, span).is_err());
        assert!(float_literal("65536.0", &Type::Binary(16), span).is_err());
    }
}
