// FUNCTIONAL-CORE
// Pure ADTs for compliance tier enforcement.
// No I/O, no mutable global state, no unsafe.

use serde::{Deserialize, Serialize};

/// Data tier classification. Tier::C (protected-class) variants
/// require explicit PermittedUse acknowledgement at call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tier {
    /// Public / non-sensitive features
    A,
    /// Sensitive-but-permissible features (e.g. prior case outcomes)
    B,
    /// Protected-class features — Tier VII / ADA / ADEA exposure
    C,
}

/// Fine-grained access reason required for Tier-C reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermittedUse {
    /// Auditing and disparate-impact analysis (read-only, never model input)
    DisparateImpactAudit,
    /// Explicit operator override with human-in-the-loop review log
    OperatorOverrideWithAuditLog,
}

/// Sensitivity classification orthogonal to Tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sensitivity {
    Public,
    Internal,
    Confidential,
    Restricted,
}

/// A typed feature value carrying its compliance metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieredFeature<T> {
    pub value: T,
    pub tier: Tier,
    pub sensitivity: Sensitivity,
}

impl<T> TieredFeature<T> {
    pub fn new(value: T, tier: Tier, sensitivity: Sensitivity) -> Self {
        Self { value, tier, sensitivity }
    }

    /// Read the inner value only if the caller supplies a valid PermittedUse for Tier-C.
    /// Non-Tier-C features are always readable.
    pub fn read(&self, permitted_use: Option<PermittedUse>) -> Option<&T> {
        match self.tier {
            Tier::C => permitted_use.map(|_| &self.value),
            _ => Some(&self.value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_c_requires_permitted_use() {
        let f = TieredFeature::new(42u32, Tier::C, Sensitivity::Restricted);
        assert!(f.read(None).is_none(), "Tier-C must be gated without PermittedUse");
        assert!(
            f.read(Some(PermittedUse::DisparateImpactAudit)).is_some(),
            "Tier-C readable with valid PermittedUse"
        );
    }

    #[test]
    fn tier_a_always_readable() {
        let f = TieredFeature::new("win_rate", Tier::A, Sensitivity::Internal);
        assert!(f.read(None).is_some());
    }
}
