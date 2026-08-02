"""
WordPress Publisher - Settings Schema (self-contained, zero dependencies)
"""

import json

_UNSET = object()


class Field:
    """Dinh nghia mot setting field.
    Self-contained - khong phu thuoc vao CrawlFlow internal modules.
    """

    def __init__(self, key, title, type="string", default=_UNSET,
                 description="", placeholder="", required=False, order=999,
                 options=None, fields=None, item_field=None, language="",
                 height="", rows=4, unit="", minimum=None, maximum=None,
                 step=None, min_length=None, max_length=None, pattern=None,
                 min_items=None, max_items=None, conditions=None):
        self.key = key
        self.title = title
        self.type = type
        self.default = default
        self.description = description
        self.placeholder = placeholder
        self.required = required
        self.order = order
        self.options = options or []
        self.fields = fields or []
        self.item_field = item_field
        self.language = language
        self.height = height
        self.rows = rows
        self.unit = unit
        self.minimum = minimum
        self.maximum = maximum
        self.step = step
        self.min_length = min_length
        self.max_length = max_length
        self.pattern = pattern
        self.min_items = min_items
        self.max_items = max_items
        self.conditions = conditions or []

    def _build_validation(self):
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

    def to_dict(self):
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
        if self.required:
            d["required"] = self.required
        if self.options:
            d["options"] = self.options
        if self.fields:
            d["fields"] = [f.to_dict() if hasattr(f, "to_dict") else f for f in self.fields]
        if self.item_field is not None:
            item = self.item_field
            d["item_field"] = item.to_dict() if hasattr(item, "to_dict") else item
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


class SettingsSchema:
    """Base class cho settings schema.
    Self-contained - khong phu thuoc vao CrawlFlow internal modules.
    """

    settings = []

    @classmethod
    def get_schema(cls):
        props = {}
        for f in cls.settings:
            key = f.key if hasattr(f, "key") else f.get("key")
            props[key] = f.to_dict() if hasattr(f, "to_dict") else f
        return {"type": "object", "properties": props}

    @classmethod
    def get_defaults(cls):
        result = {}
        for f in cls.settings:
            key = f.key if hasattr(f, "key") else f.get("key")
            if hasattr(f, "default") and f.default is not _UNSET:
                result[key] = f.default
            elif isinstance(f, dict) and "default" in f:
                result[key] = f["default"]
            if hasattr(f, "type") and f.type == "group":
                for sub in f.fields:
                    sub_key = sub.key if hasattr(sub, "key") else sub.get("key")
                    if hasattr(sub, "default") and sub.default is not _UNSET:
                        result[f"{key}.{sub_key}"] = sub.default
                    elif isinstance(sub, dict) and "default" in sub:
                        result[f"{key}.{sub_key}"] = sub["default"]
        return result

    @classmethod
    def validate(cls, values):
        errors = []
        props = cls.get_schema().get("properties", {})
        for f in cls.settings:
            key = f.key if hasattr(f, "key") else f.get("key")
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
                if v.get("minLength") and len(val) < v["minLength"]:
                    errors.append(f"{field_def.get('title', key)} must be at least {v['minLength']} characters")
                if v.get("maxLength") and len(val) > v["maxLength"]:
                    errors.append(f"{field_def.get('title', key)} must be at most {v['maxLength']} characters")
                if v.get("pattern"):
                    import re
                    if not re.match(v["pattern"], val):
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
    def to_json(cls):
        return json.dumps(cls.get_schema(), ensure_ascii=False)


CATEGORY_SOURCE_OPTIONS = [
    {"value": "fixed", "label": "Fixed category ID"},
    {"value": "from_data", "label": "From CrawlFlow data field"},
    {"value": "by_name", "label": "Auto-create by name"},
]

TAG_SOURCE_OPTIONS = [
    {"value": "fixed", "label": "Fixed tag IDs"},
    {"value": "from_data", "label": "From CrawlFlow data field"},
    {"value": "split", "label": "Split string by delimiter"},
]


class WordPressPublisherSettings(SettingsSchema):
    settings = [
        Field("wp_url", "WordPress URL", type="string", required=True,
              description="URL goc cua WordPress site",
              placeholder="https://example.com",
              order=1),
        Field("wp_username", "Username", type="string", required=True,
              description="WordPress username (co quyen admin/editor)",
              placeholder="admin",
              order=2),
        Field("wp_password", "Application Password", type="secret", required=True,
              description="Application Password (tao o Users > Application Passwords)",
              placeholder="xxxx xxxx xxxx xxxx xxxx xxxx",
              order=3),

        Field("content_type", "Content Type", type="select", required=True,
              description="Loai content se tao tren WordPress",
              options=[
                  {"value": "post", "label": "Post"},
                  {"value": "page", "label": "Page"},
                  {"value": "product", "label": "Product (WooCommerce)"},
                  {"value": "custom", "label": "Custom Post Type"},
              ],
              default="post",
              order=4),
        Field("custom_post_type", "Custom Post Type Slug", type="string",
              description="Slug cua custom post type (vd: book, listing)",
              placeholder="book",
              conditions=[{"field": "content_type", "operator": "eq", "value": "custom"}],
              order=5),

        Field("post_status", "Post Status", type="select",
              description="Trang thai bai viet sau khi tao",
              options=[
                  {"value": "publish", "label": "Publish (Cong khai)"},
                  {"value": "draft", "label": "Draft (Ban nhap)"},
                  {"value": "pending", "label": "Pending (Cho duyet)"},
                  {"value": "private", "label": "Private (Rieng tu)"},
              ],
              default="publish",
              order=6),
        Field("update_existing", "Cap nhat bai co san", type="boolean",
              default=False,
              description="Neu bai da ton tai (cung title), se cap nhat thay vi tao moi",
              order=7),

        Field("field_mappings", "Field Mapping", type="array",
              description="Map field tu CrawlFlow sang WordPress. De trong de dung mapping mac dinh.",
              item_field=Field("", "", type="group", fields=[
                  Field("crawlflow_field", "CrawlFlow Field", type="string", required=True,
                        description="Ten field trong du lieu dau vao",
                        placeholder="name"),
                  Field("wordpress_field", "WordPress Field", type="select", required=True,
                        options=[
                            {"value": "title", "label": "Title"},
                            {"value": "content", "label": "Content"},
                            {"value": "excerpt", "label": "Excerpt"},
                            {"value": "slug", "label": "Slug"},
                            {"value": "status", "label": "Status"},
                            {"value": "meta", "label": "Custom Field (meta)"},
                        ]),
                  Field("custom_meta_key", "Meta Key", type="string",
                        description="Ten meta key (neu WordPress Field = Custom Field)",
                        placeholder="_my_custom_key",
                        conditions=[{"field": "wordpress_field", "operator": "eq", "value": "meta"}]),
              ]),
              order=8),

        Field("category_source", "Category Mode", type="select",
              description="Cach thuc gan danh muc",
              options=CATEGORY_SOURCE_OPTIONS,
              default="fixed",
              order=9),
        Field("category_ids", "Category IDs", type="string",
              description="Danh sach category ID, phan cach bang dau phay (vd: 1,2,3)",
              placeholder="1,2,3",
              conditions=[{"field": "category_source", "operator": "eq", "value": "fixed"}],
              order=10),
        Field("category_data_field", "Category Data Field", type="string",
              description="Ten field trong CrawlFlow data chua category",
              placeholder="category_id",
              conditions=[{"field": "category_source", "operator": "in", "value": ["from_data", "by_name"]}],
              order=11),

        Field("tag_source", "Tag Mode", type="select",
              description="Cach thuc gan tag",
              options=TAG_SOURCE_OPTIONS,
              default="fixed",
              order=12),
        Field("tag_ids", "Tag IDs", type="string",
              description="Danh sach tag ID, phan cach bang dau phay (vd: 4,5,6)",
              placeholder="4,5,6",
              conditions=[{"field": "tag_source", "operator": "eq", "value": "fixed"}],
              order=13),
        Field("tag_data_field", "Tag Data Field", type="string",
              description="Ten field trong CrawlFlow data chua tag",
              placeholder="tags",
              conditions=[{"field": "tag_source", "operator": "in", "value": ["from_data", "split"]}],
              order=14),
        Field("tag_delimiter", "Tag Delimiter", type="string",
              description="Ky tu phan cach tag (mac dinh: dau phay)",
              default=",",
              placeholder=",",
              conditions=[{"field": "tag_source", "operator": "eq", "value": "split"}],
              order=15),

        Field("featured_image_field", "Featured Image Field", type="string",
              description="Ten field chua URL anh dai dien. De trong neu khong co.",
              placeholder="image",
              order=16),

        Field("batch_size", "Batch Size", type="number",
              default=5, minimum=1, maximum=50,
              description="So bai viet tao moi lan goi API",
              order=17),
        Field("skip_on_error", "Skip khi loi", type="boolean",
              default=True,
              description="Neu True, bo qua item loi va tiep tuc. Neu False, dung lai.",
              order=18),

        Field("timeout", "Timeout (giay)", type="number",
              default=30, minimum=5, maximum=120,
              description="Timeout cho moi request API",
              order=19),
        Field("verify_ssl", "Verify SSL", type="boolean",
              default=True,
              description="Kiem tra SSL certificate",
              order=20),
    ]
