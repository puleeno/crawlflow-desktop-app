import json
from typing import Any

from .fields import Field, _UNSET


class SettingsSchema:
    """Base class cho dinh nghia settings cua processor.

    Moi processor dinh nghia settings rieng bang cach ke thua class nay
    va khai bao cac field trong `settings` list.

    Usage:
        class OrekaShopSettings(SettingsSchema):
            settings = [
                Field("shop_url", "Shop URL", type="string", required=True,
                      description="URL cua shop can crawl"),
                Field("max_pages", "Max Pages", type="number", default=50,
                      minimum=1, order=2),
            ]
    """

    settings: list = []

    @classmethod
    def get_schema(cls) -> dict:
        """Tra ve JSON schema cua settings."""
        props = {}
        for f in cls.settings:
            key = f.key if isinstance(f, Field) else f.get("key")
            props[key] = f.to_dict() if hasattr(f, "to_dict") else f
        return {"type": "object", "properties": props}

    @classmethod
    def get_defaults(cls) -> dict:
        """Tra ve default values."""
        result = {}
        for f in cls.settings:
            key = f.key if isinstance(f, Field) else f.get("key")
            if isinstance(f, Field):
                if f.default is not _UNSET:
                    result[key] = f.default
                if f.type == "group":
                    for sub in f.fields:
                        sub_key = sub.key if isinstance(sub, Field) else sub.get("key")
                        if isinstance(sub, Field) and sub.default is not _UNSET:
                            result[f"{key}.{sub_key}"] = sub.default
                        elif isinstance(sub, dict) and "default" in sub:
                            result[f"{key}.{sub_key}"] = sub["default"]
            elif "default" in f:
                result[key] = f["default"]
        return result

    @classmethod
    def validate(cls, values: dict) -> list:
        """Validate settings values theo schema. Tra ve list error messages."""
        import re as _re
        errors = []
        props = cls.get_schema().get("properties", {})

        for f in cls.settings:
            key = f.key if isinstance(f, Field) else f.get("key")
            field_def = props.get(key, {})
            is_required = field_def.get("required", False) or field_def.get("validation", {}).get("required", False)

            if is_required and (key not in values or values[key] is None or values[key] == ""):
                errors.append(f"{field_def.get('title', key)} is required")
                continue

            if key not in values:
                continue

            val = values[key]
            v = field_def.get("validation", {})

            if isinstance(val, str):
                if v.get("minLength") is not None and len(val) < v["minLength"]:
                    errors.append(f"{field_def.get('title', key)} must be at least {v['minLength']} characters")
                if v.get("maxLength") is not None and len(val) > v["maxLength"]:
                    errors.append(f"{field_def.get('title', key)} must be at most {v['maxLength']} characters")
                if v.get("pattern") and not _re.match(v["pattern"], val):
                    errors.append(f"{field_def.get('title', key)} format is invalid")
            if isinstance(val, (int, float)):
                if v.get("minimum") is not None and val < v["minimum"]:
                    errors.append(f"{field_def.get('title', key)} must be >= {v['minimum']}")
                if v.get("maximum") is not None and val > v["maximum"]:
                    errors.append(f"{field_def.get('title', key)} must be <= {v['maximum']}")
            if v.get("enum") and val not in v["enum"]:
                errors.append(f"{field_def.get('title', key)} must be one of: {', '.join(str(e) for e in v['enum'])}")

        return errors

    @classmethod
    def to_json(cls) -> str:
        """Xuat schema ra JSON string cho frontend."""
        return json.dumps(cls.get_schema(), ensure_ascii=False)
