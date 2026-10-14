// Spec 016 FR-019: the detail pane's "AI agents" section shows a page's
// forms with their fields' accessible names, actions that are not links and
// agent-facing links, and stays out of the way for older crawls.
import { render, screen, within } from "@testing-library/react";
import { fakeTransport } from "../testing/fakeTransport";
import { Inspector } from "./Inspector";

const PATH = "/crawls/example.com.spiderlens";

function withAgents() {
  const t = fakeTransport();
  t.results.features = ["base", "extraction", "agents"];
  return t;
}

const section = async () => {
  const h = await screen.findByRole("heading", { name: "AI agents" });
  return h.closest("section")!;
};

test("a_page_with_a_form_lists_fields_and_what_an_agent_cannot_use", async () => {
  render(<Inspector transport={withAgents()} path={PATH} id={2} />);
  const s = await section();
  const form = within(s).getByRole("group", { name: "Form 1" });
  expect(form.textContent).toContain("post");
  expect(within(form).getByText("https://example.com/contact")).toBeTruthy();
  expect(within(form).getByText("Agent tool: contact_us")).toBeTruthy();
  expect(within(form).getByText("CAPTCHA: recaptcha")).toBeTruthy();
  expect(within(form).queryByText("No submit button")).toBeNull();
  expect(within(form).getByText("Email")).toBeTruthy();
  expect(within(form).getByText("placeholder only")).toBeTruthy();
  expect(within(form).getByText("No label")).toBeTruthy();
  expect(within(form).getByText("required")).toBeTruthy();
  const actions = within(s).getByRole("list", { name: "Actions without a link" });
  expect(actions.textContent).toContain("Open menu");
  expect(actions.textContent).toContain("Click handler");
  expect(within(s).getByText("Markdown version")).toBeTruthy();
  expect(within(s).getByText("https://example.com/contact.md")).toBeTruthy();
  const unlabelled = within(s).getByText("Fields without a label").nextElementSibling!;
  expect(unlabelled.textContent).toBe("1");
  expect(unlabelled.querySelector(".text-warn")).toBeTruthy();
});

test("a_plain_page_says_it_has_nothing_for_agents", async () => {
  render(<Inspector transport={withAgents()} path={PATH} id={3} />);
  const s = await section();
  expect(within(s).getByText("No forms or actions on this page.")).toBeTruthy();
});

test("older_crawls_without_agent_data_show_no_section", async () => {
  render(<Inspector transport={fakeTransport()} path={PATH} id={2} />);
  await screen.findByRole("heading", { level: 2 });
  await new Promise((r) => setTimeout(r, 20));
  expect(screen.queryByRole("heading", { name: "AI agents" })).toBeNull();
});

test("selecting_another_page_replaces_the_findings", async () => {
  const t = withAgents();
  const { rerender } = render(<Inspector transport={t} path={PATH} id={2} />);
  await within(await section()).findByRole("group", { name: "Form 1" });
  rerender(<Inspector transport={t} path={PATH} id={3} />);
  expect(await screen.findByText("No forms or actions on this page.")).toBeTruthy();
  expect(screen.queryByRole("group", { name: "Form 1" })).toBeNull();
});
