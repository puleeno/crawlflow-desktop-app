from dataclasses import dataclass, field
from typing import Any, Optional


_UNSET = object()


@dataclass
class Field:
    key: str
    title: str
    type: str = "string"
    default: Any = _UNSET
    description: str = ""
    placeholder: str = ""
    required: bool = False
    order: int = 999
    options: list = field(default_factory=list)
    fields: list = field(default_factory=list)
    item_field: Any = _UNSET
    language: str = ""
    height: str = ""
    rows: int = 4
    unit: str = ""
    minimum: Optional[float] = None
    maximum: Optional[float] = None
    step: Optional[float] = None
    min_length: Optional[int] = None
    max_length: Optional[int] = None
    pattern: Optional[str] = None
    min_items: Optional[int] = None
    max_items: Optional[int] = None
    conditions: list = field(default_factory=list)

    def _build_validation(self) -> dict:
        v = {}
        if self.required:
            v["required"] = True
        if self.minimum is not None:
            v["minimum"] = self.minimum
        if self.maximum is not None:
            v["maximum"] = self.maximum
        if self.step is not None:
            v["step"] = self.step
        if self.min_length is not None:
            v["minLength"] = self.min_length
        if self.max_length is not None:
            v["maxLength"] = self.max_length
        if self.pattern is not None:
            v["pattern"] = self.pattern
        if self.options and self.type in ("select", "multi_select"):
            v["enum"] = [o["value"] for o in self.options]
        if self.min_items is not None:
            v["minItems"] = self.min_items
        if self.max_items is not None:
            v["maxItems"] = self.max_items
        return v

    def to_dict(self) -> dict:
        d = {
            "key": self.key,
            "title": self.title,
            "type": self.type,
            "order": self.order,
            "validation": self._build_validation(),
        }
        if self.default is not _UNSET:
            d["default"] = self.default
        if self.description:
            d["description"] = self.description
        if self.placeholder:
            d["placeholder"] = self.placeholder
        if self.options:
            d["options"] = self.options
        if self.fields:
            d["fields"] = [f.to_dict() if not isinstance(f, dict) else f for f in self.fields]
        if self.item_field is not _UNSET:
            d["item_field"] = self.item_field.to_dict() if not isinstance(self.item_field, dict) else self.item_field
        if self.language:
            d["language"] = self.language
        if self.height:
            d["height"] = self.height
        if self.rows != 4:
            d["rows"] = self.rows
        if self.unit:
            d["unit"] = self.unit
        if self.conditions:
            d["conditions"] = self.conditions
        return d
