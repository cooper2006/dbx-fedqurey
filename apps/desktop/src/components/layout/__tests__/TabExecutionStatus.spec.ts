// @vitest-environment happy-dom

import { createApp, h, nextTick, reactive } from "vue";
import { afterEach, describe, expect, it, vi } from "vitest";
import TabExecutionStatus from "../TabExecutionStatus.vue";

vi.mock("vue-i18n", () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const mountedApps: ReturnType<typeof createApp>[] = [];

function mountStatus(initial: { mode: "query" | "data"; isExecuting: boolean; isCancelling?: boolean }) {
  const state = reactive(initial);
  const root = document.createElement("div");
  document.body.appendChild(root);
  const app = createApp({
    setup: () => () => h(TabExecutionStatus, { tab: state }),
  });
  mountedApps.push(app);
  app.mount(root);
  return { root, state };
}

describe("TabExecutionStatus", () => {
  afterEach(() => {
    for (const app of mountedApps.splice(0)) app.unmount();
    document.body.innerHTML = "";
  });

  it("shows an accessible, reduced-motion-safe running indicator", async () => {
    const { root } = mountStatus({ mode: "query", isExecuting: true, isCancelling: false });
    await nextTick();

    const status = root.querySelector<HTMLElement>("[data-tab-execution-status]");
    expect(status?.getAttribute("aria-label")).toBe("common.loading");
    expect(status?.className).toContain("text-blue-600");
    expect(status?.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
    expect(status?.querySelector("svg")?.getAttribute("class")).toContain("motion-reduce:animate-none");
  });

  it("announces cancelling distinctly", async () => {
    const { root } = mountStatus({ mode: "query", isExecuting: true, isCancelling: true });
    await nextTick();

    const status = root.querySelector<HTMLElement>("[data-tab-execution-status]");
    expect(status?.getAttribute("aria-label")).toBe("common.stopping");
    expect(status?.className).toContain("text-amber-600");
  });

  it.each(["success", "error", "cancelled"])("disappears as soon as %s completes", async () => {
    const { root, state } = mountStatus({ mode: "query", isExecuting: true, isCancelling: true });
    await nextTick();
    state.isExecuting = false;
    state.isCancelling = false;
    await nextTick();

    expect(root.querySelector("[data-tab-execution-status]")).toBeNull();
  });

  it("does not show query status for idle or non-query tabs", async () => {
    const idle = mountStatus({ mode: "query", isExecuting: false });
    const data = mountStatus({ mode: "data", isExecuting: true });
    await nextTick();

    expect(idle.root.querySelector("[data-tab-execution-status]")).toBeNull();
    expect(data.root.querySelector("[data-tab-execution-status]")).toBeNull();
  });
});
