use crate::check::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SimdKind {
    Signed,
    Unsigned,
    Float,
    Mask,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SimdType {
    pub bits: u16,
    pub kind: SimdKind,
}

impl SimdType {
    pub fn named(name: &str) -> Option<Self> {
        let (element, lanes) = name.split_once('x')?;
        let (kind, bits) = if let Some(bits) = element.strip_prefix("mask") {
            (SimdKind::Mask, bits.parse::<u16>().ok()?)
        } else if let Some(bits) = element.strip_prefix('f') {
            (SimdKind::Float, bits.parse::<u16>().ok()?)
        } else {
            let bits = element.strip_prefix('i')?;
            (
                if bits.ends_with('u') {
                    SimdKind::Unsigned
                } else {
                    SimdKind::Signed
                },
                bits.trim_end_matches('u').parse::<u16>().ok()?,
            )
        };
        if !matches!(bits, 8 | 16 | 32 | 64) || (kind == SimdKind::Float && bits < 32) {
            return None;
        }
        let ty = Self { bits, kind };
        (lanes.parse::<u16>().ok()? == ty.lanes() && ty.name() == name).then_some(ty)
    }

    pub fn lanes(self) -> u16 {
        128 / self.bits
    }

    pub fn element(self) -> Type {
        match self.kind {
            SimdKind::Signed => Type::Integer(self.bits, true),
            SimdKind::Unsigned => Type::Integer(self.bits, false),
            SimdKind::Float => Type::Binary(self.bits),
            SimdKind::Mask => Type::Bool,
        }
    }

    pub fn mask(self) -> Self {
        Self {
            kind: SimdKind::Mask,
            ..self
        }
    }

    pub fn name(self) -> String {
        let element = match self.kind {
            SimdKind::Signed => format!("i{}", self.bits),
            SimdKind::Unsigned => format!("i{}u", self.bits),
            SimdKind::Float => format!("f{}", self.bits),
            SimdKind::Mask => format!("mask{}", self.bits),
        };
        format!("{element}x{}", self.lanes())
    }
}
