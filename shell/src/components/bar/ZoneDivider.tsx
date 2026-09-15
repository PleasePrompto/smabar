import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, type PointerEvent } from "react";

import { t } from "../../i18n/t";
import { DIVIDER_DEFAULT, clampDividerRatio, useSmabar } from "../../store/bar";
import { reportError } from "../../ipc/log";

const PERSIST_DEBOUNCE_MS = 300;

interface DividerGesture {
  pointerId: number;
  row: HTMLElement;
  left: number;
  width: number;
  clientX: number;
  dirty: boolean;
  appliedRatio: number;
}

function persistRatio(value: number): void {
  // Browser dev keeps the ratio in the store only.
  if (!("__TAURI_INTERNALS__" in window)) return;
  void invoke("update_config", { path: "layout.dividerRatio", value }).catch(
    reportError,
  );
}

/**
 * The split-variant grip between the zones: pointer-drag adjusts the ratio
 * live in the store and persists it debounced through `update_config`;
 * double-click resets to the default.
 *
 * Draggable at full width only. At auto width the zones are content-sized
 * and the ratio has no share to allocate (BarShell), so the grip stays a
 * plain separator: a drag there could only squeeze a zone into scrolling
 * while the screen has room.
 */
export function ZoneDivider() {
  const draggable = useSmabar((s) => s.layout.width !== "auto");
  const setDividerRatio = useSmabar((s) => s.setDividerRatio);
  const gesture = useRef<DividerGesture | null>(null);
  const frame = useRef(0);
  const timer = useRef<number | null>(null);
  const pending = useRef<number | null>(null);

  const cancelPersist = useCallback(() => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = null;
    pending.current = null;
  }, []);
  const schedulePersist = useCallback((ratio: number) => {
    pending.current = ratio;
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      timer.current = null;
      if (pending.current !== null) persistRatio(pending.current);
      pending.current = null;
    }, PERSIST_DEBOUNCE_MS);
  }, []);

  const applyGesture = useCallback(
    (current: DividerGesture) => {
      if (!current.dirty) return;
      current.dirty = false;
      const rect = current.row.getBoundingClientRect();
      if (rect.width > 0) {
        current.left = rect.left;
        current.width = rect.width;
      }
      const ratio = clampDividerRatio(
        (current.clientX - current.left) / current.width,
      );
      if (ratio === current.appliedRatio) return;
      current.appliedRatio = ratio;
      setDividerRatio(ratio);
      schedulePersist(ratio);
    },
    [schedulePersist, setDividerRatio],
  );

  const finishGesture = useCallback(() => {
    const current = gesture.current;
    if (current === null) return;
    // Clear first because releasing capture dispatches lostpointercapture.
    gesture.current = null;
    if (frame.current !== 0) {
      cancelAnimationFrame(frame.current);
      frame.current = 0;
    }
    applyGesture(current);
  }, [applyGesture]);

  // Flush both an unpainted pointer sample and a pending persist when the
  // divider unmounts (e.g. a variant switch right after dragging).
  useEffect(
    () => () => {
      finishGesture();
      if (timer.current !== null && pending.current !== null) {
        window.clearTimeout(timer.current);
        persistRatio(pending.current);
      }
      timer.current = null;
      pending.current = null;
    },
    [finishGesture],
  );

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    // Without this the press starts a native selection across the zones,
    // WebKit turns dragging that selection into a native drag — and that
    // native drag swallows the pointerup, leaving the divider stuck in
    // "dragging" (it then followed the bare mouse; live-reported bug).
    e.preventDefault();
    if (gesture.current !== null) return;
    const row = e.currentTarget.parentElement;
    if (row === null) return;
    const rect = row.getBoundingClientRect();
    if (rect.width <= 0) return;
    gesture.current = {
      pointerId: e.pointerId,
      row,
      left: rect.left,
      width: rect.width,
      clientX: e.clientX,
      dirty: false,
      appliedRatio: clampDividerRatio(useSmabar.getState().layout.dividerRatio),
    };
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const current = gesture.current;
    if (current?.pointerId !== e.pointerId) return;
    // Self-heal: no button held means we missed pointerup/pointercancel
    // (native drag interruption) — never track a bare mouse move.
    if (e.buttons === 0) {
      finishGesture();
      if (e.currentTarget.hasPointerCapture(e.pointerId)) {
        e.currentTarget.releasePointerCapture(e.pointerId);
      }
      return;
    }
    current.clientX = e.clientX;
    current.dirty = true;
    if (frame.current !== 0) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      const pendingGesture = gesture.current;
      if (pendingGesture !== null) applyGesture(pendingGesture);
    });
  };
  const onPointerEnd = (e: PointerEvent<HTMLDivElement>) => {
    if (gesture.current?.pointerId !== e.pointerId) return;
    finishGesture();
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
  };
  // Fires whatever ends the capture (including a native drag stealing the
  // pointer) — the one reliable "drag is over" signal.
  const onLostPointerCapture = () => {
    finishGesture();
  };
  const onDoubleClick = () => {
    finishGesture();
    setDividerRatio(DIVIDER_DEFAULT);
    cancelPersist();
    persistRatio(DIVIDER_DEFAULT);
  };

  if (!draggable) {
    return (
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label={t("bar.divider")}
        className="zone-divider flex h-full w-2 shrink-0 items-center justify-center"
        data-fixed
      >
        <span className="zone-divider-grip h-2/3 w-0.5 rounded-full" />
      </div>
    );
  }
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={t("bar.divider")}
      className="zone-divider flex h-full w-2 shrink-0 cursor-col-resize touch-none items-center justify-center"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerEnd}
      onPointerCancel={onPointerEnd}
      onLostPointerCapture={onLostPointerCapture}
      onDoubleClick={onDoubleClick}
    >
      <span className="zone-divider-grip h-2/3 w-0.5 rounded-full" />
    </div>
  );
}
