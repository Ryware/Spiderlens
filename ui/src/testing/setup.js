"use strict";
// jsdom lacks a few browser APIs that Base UI's positioning relies on.
class ResizeObserverStub {
    observe() { }
    unobserve() { }
    disconnect() { }
}
// cmdk scrolls the active item into view.
Element.prototype.scrollIntoView ??= function () { };
globalThis.ResizeObserver ??= ResizeObserverStub;
// Base UI's switch builds a PointerEvent when clicked; jsdom has none.
globalThis.PointerEvent ??= class extends MouseEvent {
};
beforeEach(() => {
    localStorage.clear();
    for (const k of ["level", "tone", "accent"])
        delete document.documentElement.dataset[k];
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1400 });
});
