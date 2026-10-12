// jsdom lacks a few browser APIs that Base UI's positioning relies on.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
// cmdk scrolls the active item into view.
Element.prototype.scrollIntoView ??= function () {};
globalThis.ResizeObserver ??= ResizeObserverStub as unknown as typeof ResizeObserver;
// Base UI's switch builds a PointerEvent when clicked; jsdom has none.
globalThis.PointerEvent ??= class extends MouseEvent {} as unknown as typeof PointerEvent;

beforeEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset.theme;
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 1400 });
});
