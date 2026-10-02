import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

// jsdom has no layout engine, so it lacks the observers some components
// create in ngAfterViewInit. Inert stand-ins are enough for tests.
class NoopObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
  takeRecords() {
    return [];
  }
}
globalThis.ResizeObserver ??= NoopObserver as unknown as typeof ResizeObserver;
globalThis.IntersectionObserver ??= NoopObserver as unknown as typeof IntersectionObserver;

// Tests run without a Tauri backend, so answer every IPC call with an empty
// result: a known default for commands whose callers read fields off the
// result, an empty list for other getters, and null for everything else.
const DEFAULTS: Record<string, unknown> = {
  get_settings: {},
  get_network_info: { local_ips: [] },
  "plugin:app|version": "0.0.0",
};

beforeEach(() => {
  mockWindows("main");
  mockIPC((cmd) => {
    if (cmd in DEFAULTS) return DEFAULTS[cmd];
    if (cmd.startsWith("plugin:event|")) return 0;
    if (cmd.startsWith("get_") || cmd === "search") return [];
    return null;
  });
});
