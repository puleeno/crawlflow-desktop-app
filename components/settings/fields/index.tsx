import React from 'react';
import type { FieldDef } from '../../../types/settings';

const baseInput = 'w-full p-2 border border-gray-300 rounded-md text-sm focus:ring-2 focus:ring-blue-500 focus:border-blue-500 outline-none transition-colors disabled:opacity-50 disabled:bg-gray-100';
const baseLabel = 'block text-sm font-medium text-gray-700 mb-1';
const baseError = 'text-xs text-red-500 mt-1';
const baseHint = 'text-xs text-gray-400 mt-1';

interface RendererProps {
  field: FieldDef;
  value: unknown;
  onChange: (v: unknown) => void;
  error?: string;
  disabled?: boolean;
}

export function StringField({ field, value, onChange, error, disabled }: RendererProps) {
  return (
    <div>
      <label className={baseLabel}>{field.title}</label>
      <input
        type="text"
        value={(value as string) ?? ''}
        onChange={e => onChange(e.target.value)}
        placeholder={field.placeholder}
        className={`${baseInput} ${error ? 'border-red-500' : ''}`}
        disabled={disabled}
      />
      {error && <p className={baseError}>{error}</p>}
      {field.description && !error && <p className={baseHint}>{field.description}</p>}
    </div>
  );
}

export function NumberField({ field, value, onChange, error, disabled }: RendererProps) {
  return (
    <div>
      <label className={baseLabel}>
        {field.title}
        {field.unit && <span className="text-gray-400 ml-1">({field.unit})</span>}
      </label>
      <div className="flex gap-2 items-center">
        <input
          type="number"
          value={value ?? ''}
          onChange={e => {
            const v = e.target.value === '' ? '' : Number(e.target.value);
            if (v !== '') onChange(v);
          }}
          placeholder={field.placeholder}
          className={`${baseInput} ${error ? 'border-red-500' : ''}`}
          min={field.validation?.minimum}
          max={field.validation?.maximum}
          step={field.validation?.step}
          disabled={disabled}
        />
        {field.unit && <span className="text-sm text-gray-500">{field.unit}</span>}
      </div>
      {error && <p className={baseError}>{error}</p>}
      {field.description && !error && <p className={baseHint}>{field.description}</p>}
    </div>
  );
}

export function BooleanField({ field, value, onChange, disabled }: RendererProps) {
  return (
    <div className="flex items-center justify-between p-3 bg-gray-50 rounded-lg border">
      <div>
        <label className="font-medium text-sm text-gray-700">{field.title}</label>
        {field.description && <p className="text-xs text-gray-500">{field.description}</p>}
      </div>
      <button
        type="button"
        onClick={() => !disabled && onChange(!value)}
        className={`relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 ${disabled ? 'opacity-50 cursor-not-allowed' : ''} ${value ? 'bg-blue-600' : 'bg-gray-300'}`}
        role="switch"
        aria-checked={!!value}
        disabled={disabled}
      >
        <span className={`pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${value ? 'translate-x-5' : 'translate-x-0'}`} />
      </button>
    </div>
  );
}

export function SelectField({ field, value, onChange, error, disabled }: RendererProps) {
  return (
    <div>
      <label className={baseLabel}>{field.title}</label>
      <div className="relative w-full">
        <select
          value={(value as string) ?? ''}
          onChange={e => onChange(e.target.value)}
          className={`${baseInput} pr-9 appearance-none cursor-pointer ${error ? 'border-red-500' : ''}`}
          disabled={disabled}
        >
          {field.placeholder && <option value="">{field.placeholder}</option>}
          {field.options?.map(opt => (
            <option key={opt.value} value={opt.value}>{opt.label}</option>
          ))}
        </select>
        <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-2.5 text-gray-400">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" className="w-4 h-4">
            <path fillRule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clipRule="evenodd" />
          </svg>
        </div>
      </div>
      {error && <p className={baseError}>{error}</p>}
      {field.description && !error && <p className={baseHint}>{field.description}</p>}
    </div>
  );
}

export function MultiSelectField({ field, value, onChange, disabled }: RendererProps) {
  const selected = Array.isArray(value) ? (value as string[]) : [];
  const toggle = (opt: string) => {
    if (disabled) return;
    onChange(selected.includes(opt) ? selected.filter(s => s !== opt) : [...selected, opt]);
  };
  return (
    <div>
      <label className={baseLabel}>{field.title}</label>
      <div className="flex flex-wrap gap-2">
        {field.options?.map(opt => {
          const active = selected.includes(opt.value);
          return (
            <button
              key={opt.value}
              type="button"
              onClick={() => toggle(opt.value)}
              disabled={disabled}
              className={`px-3 py-1 rounded-full text-xs font-medium border transition-colors ${active ? 'bg-blue-600 text-white border-blue-600' : 'bg-white text-gray-600 border-gray-300 hover:bg-gray-50'} ${disabled ? 'opacity-50 cursor-not-allowed' : ''}`}
            >
              {opt.label}
            </button>
          );
        })}
      </div>
      {field.description && <p className={baseHint}>{field.description}</p>}
    </div>
  );
}

export function TextareaField({ field, value, onChange, error, disabled }: RendererProps) {
  return (
    <div>
      <label className={baseLabel}>{field.title}</label>
      <textarea
        value={(value as string) ?? ''}
        onChange={e => onChange(e.target.value)}
        placeholder={field.placeholder}
        rows={field.rows ?? 4}
        className={`${baseInput} ${error ? 'border-red-500' : ''}`}
        disabled={disabled}
      />
      {error && <p className={baseError}>{error}</p>}
      {field.description && !error && <p className={baseHint}>{field.description}</p>}
    </div>
  );
}

export function CodeField({ field, value, onChange, disabled }: RendererProps) {
  return (
    <div>
      <label className={baseLabel}>{field.title}</label>
      <textarea
        value={(value as string) ?? ''}
        onChange={e => onChange(e.target.value)}
        placeholder={field.placeholder}
        className={`${baseInput} font-mono text-xs`}
        style={{ height: field.height ?? '200px' }}
        spellCheck={false}
        disabled={disabled}
      />
      {field.description && <p className={baseHint}>{field.description}</p>}
    </div>
  );
}

export function SecretField({ field, value, onChange, disabled }: RendererProps) {
  return (
    <div>
      <label className={baseLabel}>{field.title}</label>
      <input
        type="password"
        value={(value as string) ?? ''}
        onChange={e => onChange(e.target.value)}
        placeholder={field.placeholder}
        className={baseInput}
        disabled={disabled}
      />
      {field.description && <p className={baseHint}>{field.description}</p>}
    </div>
  );
}

export const FIELD_RENDERERS: Record<string, React.FC<RendererProps>> = {
  string: StringField,
  number: NumberField,
  boolean: BooleanField,
  select: SelectField,
  multi_select: MultiSelectField,
  textarea: TextareaField,
  code: CodeField,
  secret: SecretField,
};
