import { useState } from "react";
import { X } from "lucide-react";
import { monogram } from "../lib/format";
import { useStore } from "../lib/store";
import type { Flavor } from "../lib/types";

export function AddonIcon({ name, src }: { name: string; src: string | null }) {
  const [failed, setFailed] = useState(false);
  if (src && !failed) {
    return <img className="addon-icon" src={src} alt="" loading="lazy" onError={() => setFailed(true)} />;
  }
  return (
    <span className="addon-icon" aria-hidden="true">
      {monogram(name)}
    </span>
  );
}

const FLAVOR_LETTER: Record<Flavor, string> = {
  forever: "F",
  retail: "R",
  classicEra: "E",
  classic: "C",
  anniversary: "A",
};

/** The game's own emblem when we could read it from the install, else a letter. */
export function GameIcon({ icon, flavor, size }: { icon: string | null; flavor: Flavor; size: number }) {
  const [failed, setFailed] = useState(false);
  if (icon && !failed) {
    return (
      <img
        className="game-icon"
        src={icon}
        alt=""
        width={size}
        height={size}
        draggable={false}
        onError={() => setFailed(true)}
      />
    );
  }
  return (
    <span className="game-icon game-icon--letter" style={{ width: size, height: size, fontSize: size * 0.45 }} aria-hidden="true">
      {FLAVOR_LETTER[flavor]}
    </span>
  );
}

export function Switch({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      className="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    />
  );
}

export function Option({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="option">
      <div className="option__text">
        <div className="option__label">{label}</div>
        {hint && <div className="option__hint">{hint}</div>}
      </div>
      {children}
    </div>
  );
}

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  const dismiss = useStore((s) => s.dismiss);
  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={`toast toast--${t.kind}`}>
          <div className="toast__text">{t.text}</div>
          <button className="toast__close" onClick={() => dismiss(t.id)} aria-label="Dismiss">
            <X size={16} />
          </button>
        </div>
      ))}
    </div>
  );
}
