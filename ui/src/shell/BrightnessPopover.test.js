import { jsx as _jsx } from "react/jsx-runtime";
// Spec 018 US1 and US4: the brightness control in the toolbar.
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeTransport } from "../testing/fakeTransport";
const root = () => document.documentElement.dataset;
let unmountApp = () => { };
async function openPopover() {
    unmountApp = render(_jsx(App, { transport: fakeTransport() })).unmount;
    // The window focuses the start URL once settings load; let that happen first.
    await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText("Start URL")));
    // A real click focuses the button; fireEvent.click alone does not.
    const button = screen.getByRole("button", { name: /^Brightness: / });
    button.focus();
    fireEvent.click(button);
    return screen.findByRole("slider", { name: "Brightness" });
}
test("button_shows_the_current_step_and_opens_the_slider_focused", async () => {
    // US1-1
    render(_jsx(App, { transport: fakeTransport() }));
    await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText("Start URL")));
    const button = screen.getByRole("button", { name: "Brightness: Light" });
    fireEvent.click(button);
    const slider = await screen.findByRole("slider", { name: "Brightness" });
    await waitFor(() => expect(document.activeElement).toBe(slider));
    expect(slider.getAttribute("aria-valuetext")).toBe("Light");
    expect(slider.getAttribute("aria-orientation")).toBe("vertical");
    const names = within(slider.parentElement).getAllByRole("button").map((b) => b.textContent);
    expect(names).toEqual(["Light", "Soft", "Island", "Dark", "Deep", "Midnight"]);
});
test("arrow_keys_move_one_step_and_home_end_jump", async () => {
    // US1-3
    const slider = await openPopover();
    fireEvent.keyDown(slider, { key: "ArrowDown" });
    await waitFor(() => expect(root().level).toBe("soft"));
    expect(slider.getAttribute("aria-valuetext")).toBe("Soft");
    fireEvent.keyDown(slider, { key: "End" });
    await waitFor(() => expect(root().level).toBe("midnight"));
    expect(root().tone).toBe("dark");
    fireEvent.keyDown(slider, { key: "ArrowUp" });
    await waitFor(() => expect(root().level).toBe("deep"));
    fireEvent.keyDown(slider, { key: "Home" });
    await waitFor(() => expect(root().level).toBe("light"));
    fireEvent.keyDown(slider, { key: "ArrowUp" });
    expect(root().level).toBe("light");
});
test("clicking_a_step_name_rethemes_and_the_icon_follows", async () => {
    // US1-2 (by click; dragging uses the same step picker)
    const slider = await openPopover();
    fireEvent.click(within(slider.parentElement).getByRole("button", { name: "Island" }));
    await waitFor(() => expect(root().level).toBe("island"));
    expect(screen.getByRole("button", { name: "Brightness: Island" })).toBeTruthy();
});
test("dragging_the_thumb_picks_the_step_under_the_pointer", async () => {
    // US1-2: the track is six equal bands.
    const slider = await openPopover();
    slider.getBoundingClientRect = () => ({ top: 100, height: 168, bottom: 268, left: 0, right: 20, width: 20, x: 0, y: 100, toJSON() { } });
    fireEvent.pointerDown(slider, { clientY: 100 + 28 * 3 + 5, pointerId: 1 });
    await waitFor(() => expect(root().level).toBe("dark"));
    fireEvent.pointerDown(slider, { clientY: 100 + 28 * 5 + 20, pointerId: 1 });
    await waitFor(() => expect(root().level).toBe("midnight"));
});
test("choice_survives_a_restart", async () => {
    // US1-4
    const slider = await openPopover();
    fireEvent.keyDown(slider, { key: "End" });
    await waitFor(() => expect(root().level).toBe("midnight"));
    unmountApp();
    delete root().level;
    render(_jsx(App, { transport: fakeTransport() }));
    await screen.findByLabelText("Start URL");
    expect(root().level).toBe("midnight");
});
test("accent_swatches_in_the_popover_change_the_palette", async () => {
    const slider = await openPopover();
    const popup = slider.closest("[data-region]");
    fireEvent.click(within(popup).getByRole("radio", { name: "Lavender" }));
    await waitFor(() => expect(root().accent).toBe("lavender"));
    expect(within(popup).getByRole("radio", { name: "Lavender" }).getAttribute("aria-checked")).toBe("true");
});
test("island_marks_toolbar_sidebar_statusbar_and_popover_as_chrome", async () => {
    // US4-1, US4-2: these regions take the dark tokens in Island.
    const slider = await openPopover();
    expect(screen.getByRole("banner").dataset.region).toBe("chrome");
    expect(screen.getByRole("navigation", { name: "Sections" }).closest("[data-region]")?.getAttribute("data-region")).toBe("chrome");
    expect(screen.getByRole("contentinfo", { name: "Crawl status" }).dataset.region).toBe("chrome");
    expect(slider.closest("[data-region]")?.getAttribute("data-region")).toBe("chrome");
    expect(screen.getByRole("main").closest("[data-region]")).toBeNull();
    expect(screen.getByRole("main").closest(".island-content")).toBeTruthy();
});
test("escape_closes_and_returns_focus_to_the_button", async () => {
    // US1-5
    const slider = await openPopover();
    fireEvent.keyDown(slider, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("slider", { name: "Brightness" })).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: /^Brightness: / })));
});
