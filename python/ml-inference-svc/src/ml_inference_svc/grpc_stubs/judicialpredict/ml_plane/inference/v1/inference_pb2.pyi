from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class ModelVariant(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    MODEL_VARIANT_UNSPECIFIED: _ClassVar[ModelVariant]
    MODEL_VARIANT_XGBOOST: _ClassVar[ModelVariant]
    MODEL_VARIANT_BAYESIAN_JUDGE: _ClassVar[ModelVariant]
    MODEL_VARIANT_RF_ATTORNEY: _ClassVar[ModelVariant]
    MODEL_VARIANT_META_LEARNER: _ClassVar[ModelVariant]
MODEL_VARIANT_UNSPECIFIED: ModelVariant
MODEL_VARIANT_XGBOOST: ModelVariant
MODEL_VARIANT_BAYESIAN_JUDGE: ModelVariant
MODEL_VARIANT_RF_ATTORNEY: ModelVariant
MODEL_VARIANT_META_LEARNER: ModelVariant

class ShapValue(_message.Message):
    __slots__ = ("feature_id", "shap_contribution", "rank")
    FEATURE_ID_FIELD_NUMBER: _ClassVar[int]
    SHAP_CONTRIBUTION_FIELD_NUMBER: _ClassVar[int]
    RANK_FIELD_NUMBER: _ClassVar[int]
    feature_id: str
    shap_contribution: float
    rank: int
    def __init__(self, feature_id: _Optional[str] = ..., shap_contribution: _Optional[float] = ..., rank: _Optional[int] = ...) -> None: ...

class ConformalInterval(_message.Message):
    __slots__ = ("lower", "upper", "coverage")
    LOWER_FIELD_NUMBER: _ClassVar[int]
    UPPER_FIELD_NUMBER: _ClassVar[int]
    COVERAGE_FIELD_NUMBER: _ClassVar[int]
    lower: float
    upper: float
    coverage: float
    def __init__(self, lower: _Optional[float] = ..., upper: _Optional[float] = ..., coverage: _Optional[float] = ...) -> None: ...

class PredictCaseOutcomeRequest(_message.Message):
    __slots__ = ("case_id", "feature_ids", "model_variant", "conformal_coverage", "trace_id")
    CASE_ID_FIELD_NUMBER: _ClassVar[int]
    FEATURE_IDS_FIELD_NUMBER: _ClassVar[int]
    MODEL_VARIANT_FIELD_NUMBER: _ClassVar[int]
    CONFORMAL_COVERAGE_FIELD_NUMBER: _ClassVar[int]
    TRACE_ID_FIELD_NUMBER: _ClassVar[int]
    case_id: str
    feature_ids: _containers.RepeatedScalarFieldContainer[str]
    model_variant: ModelVariant
    conformal_coverage: float
    trace_id: str
    def __init__(self, case_id: _Optional[str] = ..., feature_ids: _Optional[_Iterable[str]] = ..., model_variant: _Optional[_Union[ModelVariant, str]] = ..., conformal_coverage: _Optional[float] = ..., trace_id: _Optional[str] = ...) -> None: ...

class PredictCaseOutcomeResponse(_message.Message):
    __slots__ = ("case_id", "p_win", "conformal_interval", "shap_values", "model_variant_used", "mlflow_run_id", "predicted_at_unix")
    CASE_ID_FIELD_NUMBER: _ClassVar[int]
    P_WIN_FIELD_NUMBER: _ClassVar[int]
    CONFORMAL_INTERVAL_FIELD_NUMBER: _ClassVar[int]
    SHAP_VALUES_FIELD_NUMBER: _ClassVar[int]
    MODEL_VARIANT_USED_FIELD_NUMBER: _ClassVar[int]
    MLFLOW_RUN_ID_FIELD_NUMBER: _ClassVar[int]
    PREDICTED_AT_UNIX_FIELD_NUMBER: _ClassVar[int]
    case_id: str
    p_win: float
    conformal_interval: ConformalInterval
    shap_values: _containers.RepeatedCompositeFieldContainer[ShapValue]
    model_variant_used: ModelVariant
    mlflow_run_id: str
    predicted_at_unix: int
    def __init__(self, case_id: _Optional[str] = ..., p_win: _Optional[float] = ..., conformal_interval: _Optional[_Union[ConformalInterval, _Mapping]] = ..., shap_values: _Optional[_Iterable[_Union[ShapValue, _Mapping]]] = ..., model_variant_used: _Optional[_Union[ModelVariant, str]] = ..., mlflow_run_id: _Optional[str] = ..., predicted_at_unix: _Optional[int] = ...) -> None: ...
