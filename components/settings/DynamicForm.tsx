import React, { useMemo } from 'react';
import type { FieldDef, SettingsSchema, SettingsValues, ValidationError } from '../../types/settings';
import { FIELD_RENDERERS } from './fields';

interface DynamicFormProps {
  schema: SettingsSchema;
  values: SettingsValues;
  onChange: (values: SettingsValues) => void;
  errors?: ValidationError[];
  disabled?: boolean;
}

export function DynamicForm({ schema, values, onChange, errors, disabled }: DynamicFormProps) {
  const fields = useMemo(() => {
    return Object.values(schema.properties).sort((a, b) => a.order - b.order);
  }, [schema]);

  const updateField = (key: string, newValue: unknown) => {
    onChange({ ...values, [key]: newValue });
  };

  const hasError = (key: string) => {
    return errors?.find(e => e.field === key);
  };

  const meetsConditions = (field: FieldDef): boolean => {
    if (!field.conditions || field.conditions.length === 0) return true;
    return field.conditions.every(cond => {
      const currentVal = values[cond.field];
      switch (cond.operator) {
        case 'eq': return currentVal === cond.value;
        case 'neq': return currentVal !== cond.value;
        case 'in': return Array.isArray(cond.value) && cond.value.includes(currentVal);
        case 'not_in': return Array.isArray(cond.value) && !cond.value.includes(currentVal);
        default: return true;
      }
    });
  };

  return (
    <div className="space-y-4">
      {fields.map(field => {
        if (!meetsConditions(field)) return null;

        const error = hasError(field.key);
        const Renderer = FIELD_RENDERERS[field.type];
        if (Renderer) {
          return <div key={field.key}><Renderer
              field={field}
              value={values[field.key]}
              onChange={(v: unknown) => updateField(field.key, v)}
              error={error?.message}
              disabled={disabled}
            /></div>;
        }

        if (field.type === 'group') {
          return <div key={field.key}><GroupField
              field={field}
              values={values}
              onChange={onChange}
              errors={errors}
              disabled={disabled}
            /></div>;
        }

        if (field.type === 'array') {
          return <div key={field.key}><ArrayField
              field={field}
              values={values}
              onChange={onChange}
              disabled={disabled}
            /></div>;
        }

        return null;
      })}
    </div>
  );
}

// ── Group Field ───────────────────────────────────────────

const GroupField: React.FC<{
  field: FieldDef;
  values: SettingsValues;
  onChange: (v: SettingsValues) => void;
  errors?: ValidationError[];
  disabled?: boolean;
}> = ({ field, values, onChange, errors, disabled }) => {
  const updateField = (key: string, newValue: unknown) => {
    onChange({ ...values, [key]: newValue });
  };

  const hasError = (key: string) => errors?.find(e => e.field === key);

  return (
    <div className="border border-gray-200 rounded-lg p-4 bg-white">
      <h4 className="text-sm font-semibold text-gray-700 mb-3">{field.title}</h4>
      {field.description && <p className="text-xs text-gray-500 mb-3">{field.description}</p>}
      <div className="space-y-3">
        {(field.fields ?? []).map(sub => {
          const Renderer = FIELD_RENDERERS[sub.type];
          if (!Renderer) return null;
          const error = hasError(sub.key);
          return (
            <Renderer
              key={sub.key}
              field={sub}
              value={values[sub.key]}
              onChange={(v: unknown) => updateField(sub.key, v)}
              error={error?.message}
              disabled={disabled}
            />
          );
        })}
      </div>
    </div>
  );
}

// ── Array Field ───────────────────────────────────────────

function ArrayField({ field, values, onChange, disabled }: {
  field: FieldDef;
  values: SettingsValues;
  onChange: (v: SettingsValues) => void;
  disabled?: boolean;
}) {
  const items: unknown[] = Array.isArray(values[field.key]) ? values[field.key] as unknown[] : [];

  const addItem = () => {
    onChange({ ...values, [field.key]: [...items, {}] });
  };

  const removeItem = (index: number) => {
    const next = [...items];
    next.splice(index, 1);
    onChange({ ...values, [field.key]: next });
  };

  const updateItem = (index: number, val: unknown) => {
    const next = [...items];
    next[index] = val;
    onChange({ ...values, [field.key]: next });
  };

  return (
    <div className="border border-gray-200 rounded-lg p-4 bg-white">
      <div className="flex items-center justify-between mb-3">
        <h4 className="text-sm font-semibold text-gray-700">{field.title}</h4>
        {!disabled && (
          <button
            type="button"
            onClick={addItem}
            className="text-xs px-2 py-1 bg-blue-600 text-white rounded hover:bg-blue-700"
          >
            + Add
          </button>
        )}
      </div>
      {field.description && <p className="text-xs text-gray-500 mb-3">{field.description}</p>}
      <div className="space-y-2">
        {items.map((item, i) => (
          <div key={i} className="flex gap-2 items-start bg-gray-50 p-2 rounded">
            <div className="flex-1">
              <ArrayItemRenderer field={field.item_field!} value={item} onChange={v => updateItem(i, v)} disabled={disabled} />
            </div>
            {!disabled && (
              <button type="button" onClick={() => removeItem(i)} className="text-red-500 hover:text-red-700 text-lg leading-none">&times;</button>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

function ArrayItemRenderer({ field, value, onChange, disabled }: {
  field: FieldDef;
  value: unknown;
  onChange: (v: unknown) => void;
  disabled?: boolean;
}) {
  const Renderer = FIELD_RENDERERS[field?.type ?? 'string'];
  if (Renderer) {
    return <Renderer field={field} value={value as never} onChange={onChange} disabled={disabled} />;
  }
  return <input type="text" value={String(value ?? '')} onChange={e => onChange(e.target.value)} className="w-full p-2 border rounded text-sm" disabled={disabled} />;
}

// ── Hook ──────────────────────────────────────────────────

export function useSettings(schema: SettingsSchema, initial?: SettingsValues) {
  const [values, setValues] = React.useState<SettingsValues>(() => {
    const defaults: SettingsValues = {};
    for (const [key, def] of Object.entries(schema.properties)) {
      if (def.default !== undefined) {
        defaults[key] = def.default;
      }
    }
    return { ...defaults, ...initial };
  });

  const errors = React.useMemo(() => {
    const result: ValidationError[] = [];
    for (const [key, def] of Object.entries(schema.properties)) {
      const val = values[key];
      const v = def.validation;
      if (!v) continue;
      if (v.required && (val === undefined || val === null || val === '')) {
        result.push({ field: key, message: `${def.title} is required` });
      }
      if (typeof val === 'number') {
        if (v.minimum !== undefined && val < v.minimum) {
          result.push({ field: key, message: `${def.title} must be >= ${v.minimum}` });
        }
        if (v.maximum !== undefined && val > v.maximum) {
          result.push({ field: key, message: `${def.title} must be <= ${v.maximum}` });
        }
      }
      if (typeof val === 'string') {
        if (v.minLength && val.length < v.minLength) {
          result.push({ field: key, message: `${def.title} must be at least ${v.minLength} characters` });
        }
        if (v.maxLength && val.length > v.maxLength) {
          result.push({ field: key, message: `${def.title} must be at most ${v.maxLength} characters` });
        }
      }
      if (v.enum && v.enum.length > 0 && val !== undefined && !v.enum.includes(String(val))) {
        result.push({ field: key, message: `${def.title} must be one of: ${v.enum.join(', ')}` });
      }
    }
    return result;
  }, [values, schema]);

  return { values, setValues, errors, isValid: errors.length === 0 };
}
