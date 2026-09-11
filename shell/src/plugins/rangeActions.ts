import { useCallback, useEffect, useRef, useState } from "react";

import { syncKit } from "./behaviour";
import {
  findActionRange,
  rangeActionIntent,
  type RangeActionIntent,
} from "./fields";
import { sendPluginAction } from "./pluginActions";

const RANGE_ACK_TIMEOUT_MS = 5_000; // Longer than the core action deadline plus IPC.

function useRangeRenderHold(html: string) {
  const pointer = useRef<number | null>(null);
  const key = useRef<string | null>(null);
  const [heldHtml, setHeldHtml] = useState<string | null>(null);

  useEffect(() => {
    let releaseTimer = 0;
    const release = (event: Event) => {
      if (event instanceof PointerEvent) {
        if (event.pointerId !== pointer.current) return;
        pointer.current = null;
      } else if (event instanceof KeyboardEvent && event.key === key.current) {
        key.current = null;
      } else {
        return;
      }
      releaseTimer = window.setTimeout(() => {
        if (pointer.current === null && key.current === null) {
          setHeldHtml(null);
        }
      }, 0);
    };
    const releaseAll = () => {
      pointer.current = null;
      key.current = null;
      setHeldHtml(null);
    };
    window.addEventListener("pointerup", release);
    window.addEventListener("pointercancel", release);
    window.addEventListener("keyup", release);
    window.addEventListener("blur", releaseAll);
    return () => {
      window.clearTimeout(releaseTimer);
      window.removeEventListener("pointerup", release);
      window.removeEventListener("pointercancel", release);
      window.removeEventListener("keyup", release);
      window.removeEventListener("blur", releaseAll);
    };
  }, []);

  const startPointer = useCallback((pointerId: number, markup: string) => {
    pointer.current = pointerId;
    setHeldHtml(markup);
  }, []);
  const startKey = useCallback((pressedKey: string, markup: string) => {
    key.current = pressedKey;
    setHeldHtml(markup);
  }, []);
  const isHolding = useCallback(
    () => pointer.current !== null || key.current !== null,
    [],
  );

  return {
    renderedHtml: heldHtml ?? html,
    startPointer,
    startKey,
    isHolding,
  };
}

/** Owns hover/gesture holds and optimistic acknowledgement for action ranges. */
export function useRangeActions(
  html: string,
  pluginId: string,
  tileId: string,
  popupInstanceId?: number,
) {
  const pending = useRef<RangeActionIntent | null>(null);
  const pendingTimer = useRef(0);
  const hovered = useRef<{
    intent: RangeActionIntent;
    element: HTMLInputElement;
  } | null>(null);
  const { renderedHtml, startPointer, startKey, isHolding } =
    useRangeRenderHold(html);

  useEffect(
    () => () => {
      window.clearTimeout(pendingTimer.current);
    },
    [],
  );

  const applyPending = useCallback((root: ParentNode) => {
    const intent = pending.current;
    if (intent === null) return;
    const range = findActionRange(root, intent);
    if (range === undefined || range.value === intent.value) {
      pending.current = null;
      window.clearTimeout(pendingTimer.current);
    } else {
      range.value = intent.value;
    }
  }, []);

  const preserveHovered = useCallback((root: ParentNode) => {
    const current = hovered.current;
    if (current === null) return;
    const replacement = findActionRange(root, current.intent);
    if (replacement === undefined) {
      hovered.current = null;
      return;
    }
    const oldBlock =
      current.element.closest("[data-sb-range]") ?? current.element;
    const newBlock = replacement.closest("[data-sb-range]") ?? replacement;
    newBlock.replaceWith(oldBlock);
  }, []);

  const bind = useCallback(
    (root: ShadowRoot, wrapper: HTMLElement, renderedHtml: string) => {
      const actionRanges = wrapper.querySelectorAll<HTMLInputElement>(
        'input[type="range"][data-action]',
      );
      const onEnter = (event: Event) => {
        const element = event.currentTarget;
        const intent = rangeActionIntent(element);
        if (intent !== null && element instanceof HTMLInputElement) {
          hovered.current = { intent, element };
        }
      };
      const onLeave = (event: Event) => {
        if (hovered.current?.element === event.currentTarget) {
          hovered.current = null;
        }
      };
      for (const range of actionRanges) {
        range.addEventListener("pointerenter", onEnter);
        range.addEventListener("pointerleave", onLeave);
      }

      const onStart = (event: Event) => {
        if (rangeActionIntent(event.composedPath()[0] ?? null) === null) return;
        if (event instanceof PointerEvent) {
          startPointer(event.pointerId, renderedHtml);
        } else if (
          event instanceof KeyboardEvent &&
          /^(?:Arrow(?:Down|Left|Right|Up)|End|Home|Page(?:Down|Up))$/.test(
            event.key,
          )
        ) {
          startKey(event.key, renderedHtml);
        }
      };
      const onChange = (event: Event) => {
        const intent = rangeActionIntent(event.composedPath()[0] ?? null);
        if (intent === null) return;
        event.stopPropagation();
        pending.current = intent;
        window.clearTimeout(pendingTimer.current);
        const expire = () => {
          if (pending.current !== intent) return;
          if (isHolding()) {
            pendingTimer.current = window.setTimeout(
              expire,
              RANGE_ACK_TIMEOUT_MS,
            );
            return;
          }
          pending.current = null;
          const range = findActionRange(root, intent);
          if (range !== undefined) {
            range.value = range.getAttribute("value") ?? "";
            syncKit(root);
          }
        };
        pendingTimer.current = window.setTimeout(expire, RANGE_ACK_TIMEOUT_MS);
        sendPluginAction(
          pluginId,
          tileId,
          intent.action,
          intent.value,
          popupInstanceId,
        );
      };
      root.addEventListener("pointerdown", onStart);
      root.addEventListener("keydown", onStart);
      root.addEventListener("change", onChange);

      return () => {
        root.removeEventListener("pointerdown", onStart);
        root.removeEventListener("keydown", onStart);
        root.removeEventListener("change", onChange);
        for (const range of actionRanges) {
          range.removeEventListener("pointerenter", onEnter);
          range.removeEventListener("pointerleave", onLeave);
        }
      };
    },
    [isHolding, pluginId, startKey, startPointer, tileId, popupInstanceId],
  );

  return { renderedHtml, applyPending, preserveHovered, bind };
}
