import { useId } from "react";
import "./editors.css";

interface TextFieldProps {
  label: string;
  value: string;
  onChange: (v: string) => void;
  error?: string | null;
  disabled?: boolean;
  placeholder?: string;
}

export function TextField({ label, value, onChange, error, disabled, placeholder }: TextFieldProps) {
  const id = useId();
  return (
    <div className={`cgfx-field${error ? " cgfx-field--invalid" : ""}`}>
      <label htmlFor={id}>{label}</label>
      <div className="cgfx-field__control">
        <input
          id={id}
          type="text"
          value={value}
          disabled={disabled}
          placeholder={placeholder}
          onChange={(e) => onChange(e.target.value)}
        />
      </div>
      {error ? (
        <span className="cgfx-field__error" role="alert">
          {error}
        </span>
      ) : null}
    </div>
  );
}

interface NumberFieldProps {
  label: string;
  value: string;
  onChange: (v: string) => void;
  error?: string | null;
  disabled?: boolean;
}

/** Numeric input surfaced as a string so malformed values survive keystroke-by-keystroke. */
export function NumberField({ label, value, onChange, error, disabled }: NumberFieldProps) {
  const id = useId();
  return (
    <div className={`cgfx-field${error ? " cgfx-field--invalid" : ""}`}>
      <label htmlFor={id}>{label}</label>
      <div className="cgfx-field__control">
        <input
          id={id}
          type="text"
          inputMode="numeric"
          value={value}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value)}
        />
      </div>
      {error ? (
        <span className="cgfx-field__error" role="alert">
          {error}
        </span>
      ) : null}
    </div>
  );
}

interface SelectFieldProps {
  label: string;
  value: string;
  options: readonly string[];
  onChange: (v: string) => void;
  disabled?: boolean;
}

export function SelectField({ label, value, options, onChange, disabled }: SelectFieldProps) {
  const id = useId();
  return (
    <div className="cgfx-field">
      <label htmlFor={id}>{label}</label>
      <div className="cgfx-field__control">
        <select
          id={id}
          value={value}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value)}
        >
          {options.map((opt) => (
            <option key={opt} value={opt}>
              {opt}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}