// Search preview in the detail pane (spec 027 US A2): drawn snippet,
// desktop and mobile cuts, live width while editing, missing description.
import { fireEvent, render, screen, within } from "@testing-library/react";
import type { UrlDetail } from "../generated/UrlDetail";
import { fakeTransport } from "../testing/fakeTransport";
import { Inspector } from "./Inspector";
import { breadcrumb, SearchPreview } from "./SearchPreview";

const LONG_TITLE = "The Complete Beginner Guide To Choosing The Right Running Shoes For Every Kind Of Runner";
const DESC =
  "Shop lightweight running shoes for road and trail with free delivery, easy returns within thirty days and a price match on every pair we sell";

const previewTitle = () => screen.getByTestId("preview-title").textContent ?? "";
const previewDesc = () => screen.getByTestId("preview-description").textContent ?? "";

test("breadcrumb_shows_host_and_path_parts", () => {
  expect(breadcrumb("https://example.com/blog/%D7%A9%D7%9C%D7%95%D7%9D/")).toBe("example.com › blog › שלום");
  expect(breadcrumb("https://example.com/")).toBe("example.com");
  expect(breadcrumb("not a url")).toBe("not a url");
});

test("long_title_is_cut_with_an_ellipsis_on_desktop", () => {
  render(<SearchPreview url="https://example.com/shoes" snippet={{ title: LONG_TITLE, description: DESC }} />);
  expect(previewTitle().endsWith("…")).toBe(true);
  expect(LONG_TITLE.startsWith(previewTitle().slice(0, -1))).toBe(true);
  // Fits on desktop, cut on mobile.
  expect(previewDesc()).toBe(DESC);
  expect(screen.getByRole("figure", { name: "Desktop search result" })).toBeTruthy();
});

test("mobile_cuts_the_description_that_fits_on_desktop", () => {
  render(<SearchPreview url="https://example.com/shoes" snippet={{ title: "Running shoes", description: DESC }} />);
  fireEvent.click(screen.getByRole("radio", { name: "Mobile" }));
  expect(screen.getByRole("figure", { name: "Mobile search result" })).toBeTruthy();
  expect(previewDesc().endsWith("…")).toBe(true);
  expect(screen.getAllByText("Cut")).toHaveLength(1);
});

test("missing_description_says_search_engines_choose", () => {
  render(<SearchPreview url="https://example.com/" snippet={{ title: "Home", description: null }} />);
  expect(previewDesc()).toMatch(/search engines will pick text from the page/);
});

test("editing_updates_width_and_preview_while_typing", () => {
  render(<SearchPreview url="https://example.com/" snippet={{ title: "Shoes", description: null }} />);
  expect(screen.getByText(/· 5 chars/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Try other wording" }));
  const title = screen.getByRole("textbox", { name: "Title" });
  fireEvent.change(title, { target: { value: LONG_TITLE } });
  expect(previewTitle().endsWith("…")).toBe(true);
  expect(screen.getByText(new RegExp(`· ${LONG_TITLE.length} chars`))).toBeTruthy();
  expect(screen.getByRole("button", { name: "Reset to the page's text" })).toHaveProperty("disabled", false);
  fireEvent.click(screen.getByRole("button", { name: "Reset to the page's text" }));
  expect(previewTitle()).toBe("Shoes");
});

test("inspector_shows_the_preview_for_html_pages_only", async () => {
  const t = fakeTransport();
  const base = t.urlDetail;
  let patch: Partial<UrlDetail> = { snippet: { title: "Red shoes", description: "Shoes in red." } };
  t.urlDetail = async (p, id) => ({ ...(await base(p, id)), ...patch });
  const { unmount } = render(<Inspector transport={t} path="/c.spiderlens" id={2} />);
  const details = await screen.findByRole("complementary", { name: "Details" });
  expect(await within(details).findByText("Search preview")).toBeTruthy();
  expect(previewTitle()).toBe("Red shoes");
  unmount();
  patch = { snippet: null, content_type: "image/png" };
  render(<Inspector transport={t} path="/c.spiderlens" id={2} />);
  await screen.findByRole("heading", { level: 2 });
  expect(screen.queryByText("Search preview")).toBeNull();
});
