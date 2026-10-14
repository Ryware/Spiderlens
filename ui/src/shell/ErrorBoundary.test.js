import { jsx as _jsx } from "react/jsx-runtime";
import { render, screen } from "@testing-library/react";
import { ErrorBoundary } from "./ErrorBoundary";
function Broken() {
    throw new Error("cell formatter failed");
}
test("a_render_error_shows_the_error_not_an_empty_window", () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => { });
    render(_jsx(ErrorBoundary, { children: _jsx(Broken, {}) }));
    expect(screen.getByRole("alert").textContent).toContain("cell formatter failed");
    expect(screen.getByRole("button", { name: "Reload window" })).toBeTruthy();
    spy.mockRestore();
});
