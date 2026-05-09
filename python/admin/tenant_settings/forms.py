"""
Custom ModelForm for TenantSetting.

Replaces the raw JSONField textarea with a structured UI:
  - ``disabled_features``: CheckboxSelectMultiple over the 7 known Tier-A/B
    feature names.
  - ``tier_override_<feature>``: per-feature ChoiceField showing only valid
    downgrade targets (TIER_B, TIER_C — never TIER_A, which would be an
    upgrade or a no-op that signals intent incorrectly).

The form packs these fields back into the ``feature_tier_overrides`` JSON
shape on clean().  The ModelAdmin's save_model() reads
``form.cleaned_data["_packed_overrides"]`` to update the instance before
calling super().save_model().

FEATURE_ORDER mirrors python/ml-inference-svc/src/ml_inference_svc/predict.py.
These two lists must be kept in sync; a Sprint-4 follow-up will expose this
constant via a shared library.
"""

from django import forms

from .models import TenantSetting

# ---------------------------------------------------------------------------
# Feature universe — mirrors FEATURE_ORDER in ml_inference_svc/predict.py
# ---------------------------------------------------------------------------

FEATURE_ORDER: list[str] = [
    "judge_severity",
    "attorney_win_rate",
    "ideology_distance",
    "materiality_score",
    "procedural_motion_count",
    "case_type",
    "jurisdiction",
]

FEATURE_ORDER_SET: frozenset[str] = frozenset(FEATURE_ORDER)

# ---------------------------------------------------------------------------
# Tier constants
# ---------------------------------------------------------------------------

# Tier rank: lower number = more permissive.
# TIER_A (1) > TIER_B (2) > TIER_C (3)
TIER_RANK: dict[str, int] = {"TIER_A": 1, "TIER_B": 2, "TIER_C": 3}

# Only downgrade targets are valid override choices.
# TIER_A is excluded: setting TIER_A as an override either changes nothing
# (feature already Tier-A) or constitutes an illegal upgrade (Tier-B/C → A).
TIER_OVERRIDE_CHOICES: list[tuple[str, str]] = [
    ("", "— no override —"),
    ("TIER_B", "Tier B  (downgrade from A)"),
    ("TIER_C", "Tier C  (refuse / protected-class)"),
]

# ---------------------------------------------------------------------------
# Form
# ---------------------------------------------------------------------------


class TenantSettingForm(forms.ModelForm):
    """
    Structured change form for TenantSetting.

    ``tenant_id`` is the only true model field included; all other fields are
    custom and packed into ``feature_tier_overrides`` by ``clean()``.
    """

    disabled_features = forms.MultipleChoiceField(
        choices=[(f, f) for f in FEATURE_ORDER],
        widget=forms.CheckboxSelectMultiple,
        required=False,
        help_text=(
            "Checked features are refused with PERMISSION_DENIED regardless "
            "of their global tier assignment."
        ),
    )

    # tier_override_<feature> fields declared statically so Django's
    # modelform_factory can resolve admin.py fieldset references at class-build
    # time. Pre-population happens in __init__ via self.initial[...].
    locals().update({
        f"tier_override_{_f}": forms.ChoiceField(
            choices=TIER_OVERRIDE_CHOICES,
            required=False,
            label=_f.replace("_", " ").title(),
            help_text=f"Tier override for feature '{_f}'.",
        )
        for _f in FEATURE_ORDER
    })

    class Meta:
        model = TenantSetting
        # feature_tier_overrides is handled entirely via the custom fields
        # above and the per-feature tier_override_* fields added in __init__.
        fields = ["tenant_id"]

    # ------------------------------------------------------------------
    # Dynamic per-feature tier override fields
    # ------------------------------------------------------------------

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

        # Extract current stored overrides for pre-population.
        overrides: dict = {}
        if self.instance and self.instance.pk:
            overrides = self.instance.feature_tier_overrides or {}

        stored_disabled: list[str] = overrides.get("disabled_features", [])
        stored_tier: dict[str, str] = overrides.get("tier_overrides", {})

        # Pre-populate the static disabled_features field on GET.
        if not self.is_bound:
            self.initial["disabled_features"] = stored_disabled

        # Pre-populate tier_override_<feat> fields (declared at class level).
        if not self.is_bound:
            for feat in FEATURE_ORDER:
                self.initial[f"tier_override_{feat}"] = stored_tier.get(feat, "")

    # ------------------------------------------------------------------
    # Validation + packing
    # ------------------------------------------------------------------

    def clean(self):
        data = super().clean()

        # --- validate disabled_features --------------------------------
        disabled: list[str] = list(data.get("disabled_features", []))
        invalid_disabled = [f for f in disabled if f not in FEATURE_ORDER_SET]
        if invalid_disabled:
            self.add_error(
                "disabled_features",
                f"Unknown feature names: {', '.join(sorted(invalid_disabled))}. "
                f"Allowed values: {', '.join(FEATURE_ORDER)}.",
            )

        # --- validate and collect tier_overrides -----------------------
        tier_overrides: dict[str, str] = {}
        for feat in FEATURE_ORDER:
            field_name = f"tier_override_{feat}"
            val: str = data.get(field_name, "")
            if not val:
                continue
            if val not in TIER_RANK:
                self.add_error(
                    field_name,
                    f"Invalid tier value '{val}'. Allowed: TIER_B, TIER_C.",
                )
                continue
            if val == "TIER_A":
                # TIER_A as an override target means the feature stays at (or is
                # "set to") the most-permissive tier — this is either a no-op or
                # an illegal upgrade.  Reject it to prevent intent errors.
                self.add_error(
                    field_name,
                    f"TIER_A is not a valid override target for '{feat}': "
                    "only downgrades (TIER_B → B; any feature → TIER_C) are permitted.",
                )
                continue
            tier_overrides[feat] = val

        # Stash the packed result for save_model() to consume.
        data["_packed_overrides"] = {
            "disabled_features": sorted(disabled),
            "tier_overrides": tier_overrides,
        }
        return data
