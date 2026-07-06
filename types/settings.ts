// ── Field Types ───────────────────────────────────────────

export type FieldType =
  | 'string' | 'number' | 'boolean'
  | 'select' | 'multi_select'
  | 'textarea' | 'code' | 'secret'
  | 'group' | 'array';

export interface SelectOption {
  value: string;
  label: string;
}

export interface FieldCondition {
  field: string;
  operator: 'eq' | 'neq' | 'gt' | 'gte' | 'lt' | 'lte' | 'in' | 'not_in';
  value: unknown;
}

export interface FieldValidation {
  required?: boolean;
  minimum?: number;
  maximum?: number;
  step?: number;
  minLength?: number;
  maxLength?: number;
  pattern?: string;
  enum?: string[];
  minItems?: number;
  maxItems?: number;
}

// ── Field Definition ──────────────────────────────────────

export interface FieldDef {
  key: string;
  title: string;
  type: FieldType;
  default?: unknown;
  description?: string;
  placeholder?: string;
  required?: boolean;
  order: number;
  validation?: FieldValidation;
  conditions?: FieldCondition[];
  options?: SelectOption[];
  fields?: FieldDef[];
  item_field?: FieldDef;
  language?: string;
  height?: string;
  rows?: number;
  unit?: string;
}

// ── Settings Schema ───────────────────────────────────────

export interface SettingsSchema {
  type: 'object';
  properties: Record<string, FieldDef>;
}

// ── Settings State ────────────────────────────────────────

export type SettingsValues = Record<string, unknown>;

export interface ValidationError {
  field: string;
  message: string;
}
