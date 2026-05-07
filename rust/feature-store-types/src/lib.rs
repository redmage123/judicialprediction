// FUNCTIONAL-CORE
// Pure ADTs for compliance tier enforcement.
// No I/O, no mutable global state, no unsafe.

use serde::{Deserialize, Serialize};

/// Data tier classification matching the proto `Tier` enum in
/// `judicialpredict.data_plane.feature_store.v1`.
///
/// Model-safety ordering (ascending): A < B < D.
/// Tier C (protected-class) is **excluded** from all model inputs — it may only
/// be accessed under an explicit `PermittedUse` for audit purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tier {
    /// Public / non-sensitive features (e.g. jurisdiction, court name).
    A,
    /// Sensitive-but-permissible features (e.g. prior case outcomes).
    B,
    /// Protected-class features — Title VII / ADA / ADEA exposure.
    /// Must never be used as model input; readable only under `PermittedUse`.
    C,
    /// Derived / computed features produced by the ML pipeline itself.
    /// Higher sensitivity than B but still permissible as model input.
    D,
}

impl Tier {
    /// Returns true if this tier may be used as model input.
    /// Tier::C is excluded; all others are permitted.
    pub fn is_safe_for_model(self) -> bool {
        self != Tier::C
    }

    /// Numeric model-safety level for ordering safe tiers.
    /// Returns `None` for Tier::C (excluded from modeling).
    /// Safe tiers are ordered A(1) < B(2) < D(3).
    pub fn model_safety_level(self) -> Option<u8> {
        match self {
            Tier::A => Some(1),
            Tier::B => Some(2),
            Tier::C => None,
            Tier::D => Some(3),
        }
    }
}

/// Fine-grained access reason required for Tier-C reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermittedUse {
    /// Auditing and disparate-impact analysis (read-only, never model input).
    DisparateImpactAudit,
    /// Explicit operator override with human-in-the-loop review log.
    OperatorOverrideWithAuditLog,
}

/// Sensitivity classification orthogonal to Tier.
/// Matches `judicialpredict.data_plane.feature_store.v1.Sensitivity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sensitivity {
    Public,
    QuasiPublic,
    Inferred,
    Protected,
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

    /// Read the inner value only if the caller supplies a valid `PermittedUse` for Tier-C.
    /// Non-Tier-C features are always readable.
    pub fn read(&self, permitted_use: Option<PermittedUse>) -> Option<&T> {
        match self.tier {
            Tier::C => permitted_use.map(|_| &self.value),
            _ => Some(&self.value),
        }
    }

    /// Returns true if this feature is safe to use as model input (i.e. not Tier-C).
    pub fn is_safe_for_model(&self) -> bool {
        self.tier.is_safe_for_model()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_c_requires_permitted_use() {
        let f = TieredFeature::new(42u32, Tier::C, Sensitivity::Protected);
        assert!(f.read(None).is_none(), "Tier-C must be gated without PermittedUse");
        assert!(
            f.read(Some(PermittedUse::DisparateImpactAudit)).is_some(),
            "Tier-C readable with valid PermittedUse"
        );
    }

    #[test]
    fn tier_a_always_readable() {
        let f = TieredFeature::new("win_rate", Tier::A, Sensitivity::Public);
        assert!(f.read(None).is_some());
    }

    #[test]
    fn tier_d_is_safe_for_model() {
        assert!(Tier::D.is_safe_for_model());
        assert!(!Tier::C.is_safe_for_model());
    }

    #[test]
    fn model_safety_ordering() {
        assert!(Tier::A.model_safety_level() < Tier::B.model_safety_level());
        assert!(Tier::B.model_safety_level() < Tier::D.model_safety_level());
        assert!(Tier::C.model_safety_level().is_none());
    }
}
