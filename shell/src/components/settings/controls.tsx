import {
  ArrowDown,
  ArrowUp,
  ChevronRight,
  RotateCcw,
  TriangleAlert,
} from "lucide-react";
import { useId, useRef, useState, type ReactNode } from "react";

import { t } from "../../i18n/t";
import { reportError } from "../../ipc/log";

/** Shared building blocks of the settings tabs (sb-* kit classes only). */

/** Inline yes/no row — the bar is an X11 DOCK window, a native dialog would
 *  open behind it, so every confirmation is inline (never window.confirm).
 *  The question wraps rather than truncating: a store install names the
 *  version, the repository and that smabar has not reviewed the code, and
 *  every word of that has to be readable before the button is. */
export function ConfirmRow({
  label,
  question,
  action,
  onCancel,
  onConfirm,
  returnFocus,
}: {
  label: string;
  question: string;
  action: string;
  onCancel: () => void;
  onConfirm: () => void;
  returnFocus?: () => void;
}) {
  const questionId = useId();
  return (
    <div
      className="sb-row"
      data-confirm-row
      role="group"
      aria-label={label}
      aria-describedby={questionId}
    >
      <TriangleAlert size="1em" className="sb-crit" aria-hidden="true" />
      <span id={questionId}>{question}</span>
      <button
        className="sb-btn sb-btn-ghost"
        autoFocus
        onClick={() => {
          onCancel();
          if (returnFocus !== undefined) queueMicrotask(returnFocus);
        }}
      >
        {t("settings.cancel")}
      </button>
      <button
        className="sb-btn sb-btn-danger"
        onClick={() => {
          onConfirm();
        }}
      >
        {action}
      </button>
    </div>
  );
}

/**
 * One whole section of the panel: its heading, its reset, its blocks. A
 * section without `onReset` has nothing to reset (the legal texts) and shows
 * no reset control.
 */
export function SettingsSection({
  title,
  onReset,
  resetQuestion,
  children,
}: {
  title: string;
  onReset?: () => Promise<void>;
  /** When set, the reset asks this question inline before running. */
  resetQuestion?: string;
  children: ReactNode;
}) {
  const [resetting, setResetting] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const resetButton = useRef<HTMLButtonElement>(null);
  const reset = async (run: () => Promise<void>) => {
    setConfirming(false);
    setResetting(true);
    try {
      await run();
    } catch (error: unknown) {
      reportError(error);
    } finally {
      setResetting(false);
      requestAnimationFrame(() => resetButton.current?.focus());
    }
  };
  return (
    <section className="settings-group" aria-label={title}>
      <div className="settings-group-header">
        <h2>{title}</h2>
        {onReset !== undefined && (
          <button
            ref={resetButton}
            type="button"
            className="sb-btn sb-btn-ghost"
            disabled={resetting}
            onClick={() => {
              if (resetQuestion !== undefined) {
                setConfirming(true);
              } else {
                void reset(onReset);
              }
            }}
          >
            <RotateCcw size="1em" aria-hidden="true" />
            {t("settings.reset")}
          </button>
        )}
      </div>
      {confirming && resetQuestion !== undefined && onReset !== undefined && (
        <ConfirmRow
          label={title}
          question={resetQuestion}
          action={t("settings.reset")}
          onCancel={() => {
            setConfirming(false);
          }}
          onConfirm={() => {
            void reset(onReset);
          }}
          returnFocus={() => {
            resetButton.current?.focus();
          }}
        />
      )}
      {children}
    </section>
  );
}

/**
 * A titled box of related settings — the box IS the grouping signal, so it
 * goes around a GROUP and never around a single setting. Its title is also
 * what the sub-navigation lists and scrolls to (see useSubsections).
 */
/**
 * `collapsible` renders the block closed with its title as the toggle — for
 * the per-tile forms, where a dozen open blocks would bury the page. The
 * element keeps its own open state, so a re-render never snaps it shut.
 */
export function SettingGroup({
  title,
  children,
  collapsible = false,
  updateKey,
}: {
  title: string;
  children: ReactNode;
  collapsible?: boolean;
  updateKey?: string;
}) {
  if (collapsible) {
    return (
      <details
        className="settings-block"
        aria-label={title}
        data-update-key={updateKey}
      >
        <summary className="settings-block-title">
          <ChevronRight size="1em" aria-hidden="true" />
          {title}
        </summary>
        <div className="settings-box">{children}</div>
      </details>
    );
  }
  return (
    <section
      className="settings-block"
      aria-label={title}
      data-update-key={updateKey}
    >
      <h3 className="settings-block-title">{title}</h3>
      <div className="settings-box">{children}</div>
    </section>
  );
}

/** Keeps unavailable settings mounted while smoothly removing them from use. */
export function SettingReveal({
  visible,
  children,
}: {
  visible: boolean;
  children: ReactNode;
}) {
  return (
    <div
      className={`sb-reveal settings-reveal ${visible ? "sb-active" : ""}`}
      aria-hidden={visible ? undefined : true}
      inert={visible ? undefined : true}
    >
      <div>{children}</div>
    </div>
  );
}

/**
 * One setting: name and why it exists on the left, its control on the right —
 * the two-column form every desktop settings window uses, so a page of
 * settings reads as a table instead of a stack of cards. A name that says
 * everything needs no explanation at all.
 *
 * `control` is a suffix that sits on the name's line (a switch, a button);
 * `children` is the main control in the right column (choices, sliders,
 * inputs). `wide` gives a control the full row width below the name: lists,
 * searches, colour and font panels — anything that cannot live in a column.
 *
 * A setting that another setting has switched off stays visible and says why:
 * `disabledReason` REPLACES the description, and the row marks itself so the
 * name and control dim while the reason gets more contrast, not less — it is
 * the one thing that still has to be read. Passing the reason does not
 * disable the control; the caller still hands `disabled` to it.
 */
export function SettingRow({
  label,
  description,
  disabledReason,
  control,
  wide = false,
  children,
}: {
  label: string;
  /** Why this setting exists; omitted when the name already says it. */
  description?: string;
  /** Why this setting is inactive right now; replaces `description`. */
  disabledReason?: string;
  /** Control on the label's line — a switch or a button. */
  control?: ReactNode;
  /** The control needs the whole row: lists, searches, colour panels. */
  wide?: boolean;
  /** The main control: choices, sliders, inputs, lists. */
  children?: ReactNode;
}) {
  const help = disabledReason ?? description;
  return (
    <div
      className="settings-row"
      data-disabled={disabledReason === undefined ? undefined : ""}
      data-wide={wide ? "" : undefined}
    >
      <div className="settings-row-head">
        <div className="settings-row-label">{label}</div>
        {help !== undefined && <p className="settings-help">{help}</p>}
      </div>
      {control !== undefined && (
        <div className="settings-row-suffix">{control}</div>
      )}
      {children !== undefined && (
        <div className="settings-row-control">{children}</div>
      )}
    </div>
  );
}

/**
 * A segmented control: the choices sit in one row and the active one is
 * filled. `layout="grid"` keeps a fixed three-column grid for the one choice
 * whose options are positions on a screen (the popup corner).
 */
export function ChoiceGrid({
  label,
  layout = "segmented",
  children,
}: {
  label: string;
  layout?: "segmented" | "grid";
  children: ReactNode;
}) {
  return (
    <div
      className="sb-choice-grid"
      data-layout={layout}
      role="group"
      aria-label={label}
    >
      {children}
    </div>
  );
}

export function Choice({
  label,
  active,
  disabled = false,
  onClick,
  children,
}: {
  label: string;
  active: boolean;
  disabled?: boolean;
  onClick: () => void;
  /** Optional pictogram in front of the label. */
  children?: ReactNode;
}) {
  return (
    <button
      type="button"
      className={`sb-choice ${active ? "sb-active" : ""}`}
      aria-pressed={active}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
      {label}
    </button>
  );
}

/** The switch itself; its name is the row's, so it carries only aria. */
export function Switch({
  label,
  checked,
  disabled = false,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <input
      type="checkbox"
      className="sb-toggle"
      checked={checked}
      disabled={disabled}
      aria-label={label}
      onChange={(e) => {
        onChange(e.target.checked);
      }}
    />
  );
}

/** A range slider with a monospace value readout. */
export function SliderRow({
  label,
  min,
  max,
  step,
  value,
  display,
  disabled = false,
  onChange,
}: {
  label: string;
  min: number;
  max: number;
  step: number;
  value: number;
  /** Formatted value shown next to the slider (e.g. "32px"). */
  display: string;
  disabled?: boolean;
  onChange: (value: number) => void;
}) {
  return (
    <div className="settings-slider">
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        aria-label={label}
        onChange={(e) => {
          onChange(Number(e.target.value));
        }}
      />
      <span className="sb-mono sb-dim">{display}</span>
    </div>
  );
}

/** App icon when resolved, otherwise an initial-letter badge. */
export function IconOrInitial({
  icon,
  label,
}: {
  icon: string | null | undefined;
  label: string;
}) {
  return typeof icon === "string" ? (
    <img
      src={icon}
      alt=""
      style={{ width: 20, height: 20, objectFit: "contain", flex: "none" }}
      draggable={false}
    />
  ) : (
    <span
      className="sb-icon-badge"
      style={{ width: 20, fontSize: "var(--sb-fs-xs)" }}
    >
      {label.charAt(0).toUpperCase()}
    </span>
  );
}

/** Up/down reorder buttons for one list row; edge moves are disabled. */
export function MoveButtons({
  index,
  count,
  onMove,
}: {
  index: number;
  count: number;
  onMove: (from: number, to: number) => void;
}) {
  return (
    <>
      <MoveButton
        label={t("settings.moveUp")}
        disabled={index === 0}
        onClick={() => {
          onMove(index, index - 1);
        }}
      >
        <ArrowUp size="1em" />
      </MoveButton>
      <MoveButton
        label={t("settings.moveDown")}
        disabled={index === count - 1}
        onClick={() => {
          onMove(index, index + 1);
        }}
      >
        <ArrowDown size="1em" />
      </MoveButton>
    </>
  );
}

function MoveButton({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      className="sb-btn sb-btn-ghost sb-btn-icon"
      style={disabled ? { opacity: 0.35 } : undefined}
      aria-label={label}
      title={label}
      // aria-disabled, NOT disabled: pressing "down" repeatedly makes the
      // button under your finger disabled at the last position, focus falls
      // to <body> and the keyboard user is dropped out of the list. moveItem
      // treats an edge move as a no-op anyway, so the click is harmless.
      aria-disabled={disabled}
      onClick={() => {
        if (!disabled) onClick();
      }}
    >
      {children}
    </button>
  );
}
