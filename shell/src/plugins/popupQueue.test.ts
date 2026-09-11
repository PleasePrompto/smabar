import { expect, test } from "vitest";

import {
  clampPopupTtl,
  dismissPopup,
  duePopups,
  EMPTY_POPUP_STACK,
  enqueuePopup,
  POPUP_QUEUED_MAX,
  POPUP_TOTAL_MAX,
  POPUP_VISIBLE_MAX,
  popupHorizontal,
  popupVertical,
  type PopupStackState,
} from "./popupQueue";

const popup = (pluginId: string, ttlMs?: number | null) => ({
  pluginId,
  tileId: "main",
  html: `<p>${pluginId}</p>`,
  ttlMs,
});

test("a managed popup updates in place without resetting its lifetime or adding a duplicate", () => {
  const request = { ...popup("todos", 5000), instanceId: 42 };
  let state = enqueuePopup(EMPTY_POPUP_STACK, request, 1000, 1);
  state = enqueuePopup(state, { ...request, html: "<p>Updated</p>" }, 4000, 2);
  expect(state.visible).toHaveLength(1);
  expect(state.visible[0]).toMatchObject({
    id: 1,
    instanceId: 42,
    shownAtMs: 1000,
    html: "<p>Updated</p>",
  });
  expect(duePopups(state, 6000)).toEqual([1]);
  state = enqueuePopup(state, { ...request, instanceId: 43 }, 4500, 3);
  state = dismissPopup(state, 1, 4600);
  expect(state.visible.map((item) => item.instanceId)).toEqual([43]);
});

test("popup ttl is sticky when missing and clamps to 1000–120000 ms", () => {
  expect(clampPopupTtl(undefined)).toBeNull();
  expect(clampPopupTtl(null)).toBeNull();
  expect(clampPopupTtl(Number.NaN)).toBeNull();
  expect(clampPopupTtl(500)).toBe(1_000);
  expect(clampPopupTtl(8_400.4)).toBe(8_400);
  expect(clampPopupTtl(500_000)).toBe(120_000);
});

test("popups stack visibly up to the cap, overflow queues in order", () => {
  let state: PopupStackState = EMPTY_POPUP_STACK;
  for (let i = 1; i <= POPUP_VISIBLE_MAX + 2; i += 1) {
    state = enqueuePopup(state, popup(`p${String(i)}`), 1_000 + i, i);
  }
  expect(state.visible.map((item) => item.pluginId)).toEqual([
    "p1",
    "p2",
    "p3",
    "p4",
    "p5",
  ]);
  expect(state.queued.map((item) => item.pluginId)).toEqual(["p6", "p7"]);
});

test("popup overflow stays bounded and preserves the newest waiting items", () => {
  let state: PopupStackState = EMPTY_POPUP_STACK;
  for (let id = 1; id <= POPUP_TOTAL_MAX + 10; id += 1) {
    state = enqueuePopup(state, popup(`p${String(id)}`), 1_000, id);
  }

  expect(state.visible.map((item) => item.id)).toEqual([1, 2, 3, 4, 5]);
  expect(state.queued).toHaveLength(POPUP_QUEUED_MAX);
  expect(state.queued[0]?.id).toBe(16);
  expect(state.queued.at(-1)?.id).toBe(POPUP_TOTAL_MAX + 10);
});

test("timed overflow eventually drains without manual dismissal", () => {
  let state: PopupStackState = EMPTY_POPUP_STACK;
  for (let i = 1; i <= 20; i += 1) {
    state = enqueuePopup(state, popup(`p${String(i)}`, 5_000), 0, i);
  }

  for (let nowMs = 5_000; nowMs <= 20_000; nowMs += 5_000) {
    for (const id of duePopups(state, nowMs)) {
      state = dismissPopup(state, id, nowMs);
    }
  }
  expect(state).toEqual(EMPTY_POPUP_STACK);
});

test("dismissing mid-stack collapses and promotes from the queue", () => {
  let state: PopupStackState = EMPTY_POPUP_STACK;
  for (let i = 1; i <= 6; i += 1) {
    state = enqueuePopup(state, popup(`p${String(i)}`), 1_000, i);
  }
  state = dismissPopup(state, 3, 9_000);
  expect(state.visible.map((item) => item.id)).toEqual([1, 2, 4, 5, 6]);
  expect(state.queued).toEqual([]);
  // The promoted popup's timer starts when it becomes visible.
  expect(state.visible[4]?.shownAtMs).toBe(9_000);
  // Everyone else keeps the original start.
  expect(state.visible[0]?.shownAtMs).toBe(1_000);

  // Dismissing an unknown id is a no-op returning the same state.
  expect(dismissPopup(state, 99, 10_000)).toBe(state);
  // A queued popup can be dismissed before it ever became visible.
  let queuedState: PopupStackState = EMPTY_POPUP_STACK;
  for (let i = 1; i <= 7; i += 1) {
    queuedState = enqueuePopup(queuedState, popup(`p${String(i)}`), 1_000, i);
  }
  queuedState = dismissPopup(queuedState, 7, 2_000);
  expect(queuedState.queued.map((item) => item.id)).toEqual([6]);
  expect(queuedState.visible).toHaveLength(5);
});

test("mixed sticky and timed toasts keep independent, non-restarting timers", () => {
  // 3 sticky + 4 timed (2s, 5s, 8s, 12s) arriving together at t=0.
  let state: PopupStackState = EMPTY_POPUP_STACK;
  const requests: [string, number | null][] = [
    ["sticky-a", null],
    ["timed-2s", 2_000],
    ["sticky-b", null],
    ["timed-5s", 5_000],
    ["sticky-c", null],
    ["timed-8s", 8_000],
    ["timed-12s", 12_000],
  ];
  requests.forEach(([pluginId, ttlMs], index) => {
    state = enqueuePopup(state, popup(pluginId, ttlMs), 0, index + 1);
  });
  // Cap of 5 visible: timed-8s and timed-12s wait in the queue.
  expect(state.visible.map((item) => item.pluginId)).toEqual([
    "sticky-a",
    "timed-2s",
    "sticky-b",
    "timed-5s",
    "sticky-c",
  ]);

  // t=2000: only the 2s toast is due; sticky toasts are never due.
  expect(duePopups(state, 2_000)).toEqual([2]);
  state = dismissPopup(state, 2, 2_000);
  // timed-8s got promoted; its timer starts NOW (due at 10_000, not 8_000).
  expect(state.visible.map((item) => item.pluginId)).toContain("timed-8s");
  expect(duePopups(state, 8_000)).toEqual([4]); // only timed-5s (id 4)

  // t=5000: timed-5s expires; its removal must NOT restart others.
  state = dismissPopup(state, 4, 5_000);
  const timed12 = state.visible.find((item) => item.pluginId === "timed-12s");
  expect(timed12?.shownAtMs).toBe(5_000); // promoted at t=5000 → due 17_000
  const timed8 = state.visible.find((item) => item.pluginId === "timed-8s");
  expect(timed8?.shownAtMs).toBe(2_000); // untouched by the neighbor's exit

  expect(duePopups(state, 9_999)).toEqual([]);
  expect(duePopups(state, 10_000)).toEqual([6]); // timed-8s at 2000+8000
  state = dismissPopup(state, 6, 10_000);
  expect(duePopups(state, 17_000)).toEqual([7]); // timed-12s at 5000+12000
  state = dismissPopup(state, 7, 17_000);

  // Only the three sticky toasts remain — forever, until the user acts.
  expect(state.visible.map((item) => item.pluginId)).toEqual([
    "sticky-a",
    "sticky-b",
    "sticky-c",
  ]);
  expect(duePopups(state, Number.MAX_SAFE_INTEGER)).toEqual([]);
  state = dismissPopup(state, 1, 20_000);
  state = dismissPopup(state, 3, 20_001);
  state = dismissPopup(state, 5, 20_002);
  expect(state.visible).toEqual([]);
  expect(state.queued).toEqual([]);
});

test("popup position maps to vertical and horizontal anchors", () => {
  expect(popupVertical("top-left")).toBe("top");
  expect(popupVertical("bottom-center")).toBe("bottom");
  expect(popupHorizontal("top-left")).toBe("left");
  expect(popupHorizontal("bottom-center")).toBe("center");
  expect(popupHorizontal("top-right")).toBe("right");
});
