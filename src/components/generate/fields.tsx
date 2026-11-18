"use client";

import type { ReactNode } from "react";
import { useState } from "react";

import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";

export function Field({
  label,
  htmlFor,
  error,
  children,
  hint,
}: {
  label: string;
  htmlFor?: string;
  error?: string | null;
  children: ReactNode;
  hint?: string;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
      {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
    </div>
  );
}

/** 入力途中(空文字など)を許容する数値入力。フォーカス中だけ下書きの文字列を表示する。 */
export function NumberInput({
  id,
  value,
  onChange,
  min,
  max,
  step,
  invalid,
  disabled,
}: {
  id?: string;
  value: number;
  onChange: (n: number) => void;
  min?: number;
  max?: number;
  step?: number;
  invalid?: boolean;
  disabled?: boolean;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  return (
    <Input
      id={id}
      type="number"
      inputMode="decimal"
      value={draft ?? String(value)}
      min={min}
      max={max}
      step={step}
      aria-invalid={invalid || undefined}
      disabled={disabled}
      onChange={(e) => {
        setDraft(e.target.value);
        const n = Number(e.target.value);
        if (e.target.value.trim() !== "" && Number.isFinite(n)) onChange(n);
      }}
      onBlur={() => setDraft(null)}
    />
  );
}

/** スライダー + 数値入力。 */
export function SliderNumber({
  id,
  value,
  onChange,
  min,
  max,
  step,
  invalid,
}: {
  id?: string;
  value: number;
  onChange: (n: number) => void;
  min: number;
  max: number;
  step: number;
  invalid?: boolean;
}) {
  return (
    <div className="flex items-center gap-3">
      <Slider
        className="flex-1"
        value={[Math.min(Math.max(value, min), max)]}
        min={min}
        max={max}
        step={step}
        onValueChange={(v) => onChange(Array.isArray(v) ? v[0] : v)}
      />
      <div className="w-20 shrink-0">
        <NumberInput id={id} value={value} onChange={onChange} min={min} max={max} step={step} invalid={invalid} />
      </div>
    </div>
  );
}
