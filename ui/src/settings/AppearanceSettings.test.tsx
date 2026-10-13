// Spec 018 US2 and US3: Settings › Appearance.
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "../App";
import { fakeTransport } from "../testing/fakeTransport";

const root = () => document.documentElement.dataset;

async function openAppearance(transport = fakeTransport()) {
  render(<App transport={transport} />);
  await screen.findByLabelText("Start URL");
  fireEvent.click(screen.getByRole("button", { name: "Configure" }));
  const nav = await screen.findByRole("navigation", { name: "Settings categories" });
  fireEvent.click(within(nav).getByRole("tab", { name: "Appearance" }));
  return { transport, nav, panel: await screen.findByRole("tabpanel", { name: "Appearance" }) };
}

test("appearance_sits_in_its_own_group_without_issue_counts", async () => {
  // US2-1
  const { nav, panel } = await openAppearance();
  const tab = within(nav).getByRole("tab", { name: "Appearance" });
  expect(tab.closest("ul")?.textContent).toMatch(/^App/);
  expect(tab.textContent).toBe("Appearance");
  expect(within(panel).getByText("Saved on this computer")).toBeTruthy();
});

test("picking_an_accent_applies_at_once_and_is_marked", async () => {
  // US2-2
  const { panel } = await openAppearance();
  const accents = within(panel).getByRole("radiogroup", { name: "Accent" });
  expect(within(accents).getAllByRole("radio").map((r) => r.textContent)).toEqual(["Teal", "Cobalt", "Lavender", "Orchid", "Celery", "Cloud"]);
  fireEvent.click(within(accents).getByRole("radio", { name: "Orchid" }));
  await waitFor(() => expect(root().accent).toBe("orchid"));
  expect(within(accents).getByRole("radio", { name: "Orchid" }).getAttribute("aria-checked")).toBe("true");
  expect(within(accents).getByRole("radio", { name: "Teal" }).getAttribute("aria-checked")).toBe("false");
});

test("brightness_previews_pick_the_step", async () => {
  const { panel } = await openAppearance();
  const steps = within(panel).getByRole("radiogroup", { name: "Brightness" });
  expect(within(steps).getAllByRole("radio").map((r) => r.textContent)).toEqual(["Light", "Soft", "Island", "Dark", "Deep", "Midnight"]);
  fireEvent.click(within(steps).getByRole("radio", { name: "Deep" }));
  await waitFor(() => expect(root().level).toBe("deep"));
  expect(within(steps).getByRole("radio", { name: "Deep" }).getAttribute("aria-checked")).toBe("true");
});

test("follow_system_shows_light_and_dark_step_choices", async () => {
  // US3-1: on by default, mapped to Light and Dark.
  const { panel } = await openAppearance();
  const light = within(panel).getByLabelText("When the system is light") as HTMLSelectElement;
  const dark = within(panel).getByLabelText("When the system is dark") as HTMLSelectElement;
  expect([light.value, dark.value]).toEqual(["light", "dark"]);
  expect([...light.options].map((o) => o.textContent)).toEqual(["Light", "Soft", "Island"]);
  expect([...dark.options].map((o) => o.textContent)).toEqual(["Dark", "Deep", "Midnight"]);
  fireEvent.change(light, { target: { value: "soft" } });
  await waitFor(() => expect(root().level).toBe("soft"));
  fireEvent.click(within(panel).getByRole("switch", { name: "Follow system" }));
  await waitFor(() => expect(within(panel).queryByLabelText("When the system is light")).toBeNull());
  expect(root().level).toBe("soft");
});

test("profile_actions_leave_appearance_alone", async () => {
  // US2-4, FR-012
  const { transport, panel } = await openAppearance();
  fireEvent.click(within(panel).getByRole("radio", { name: "Celery" }));
  fireEvent.click(within(within(panel).getByRole("radiogroup", { name: "Brightness" })).getByRole("radio", { name: "Midnight" }));
  await waitFor(() => expect(root().level).toBe("midnight"));
  fireEvent.click(screen.getByRole("button", { name: "Profile actions" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "Save as new profile…" }));
  fireEvent.change(await screen.findByLabelText("New profile name"), { target: { value: "Night" } });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(transport.saved.has("Night")).toBe(true));
  expect(JSON.stringify(transport.saved.get("Night"))).not.toMatch(/midnight|celery|accent/);
  fireEvent.click(screen.getByRole("button", { name: "Profile actions" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "Reset to defaults" }));
  expect(root()).toMatchObject({ level: "midnight", accent: "celery" });
});
