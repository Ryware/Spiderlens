// Spec 016 FR-018: the Agents tab's summary shows the crawl's score and
// parts, the rules that cost points, the checks that could not run and the
// entry points of each host, and says why when there is no score.
import { fireEvent, render, screen, within } from "@testing-library/react";
import { fakeTransport } from "../testing/fakeTransport";
import { AgentsOverview } from "./AgentsOverview";

const PATH = "/crawls/example.com.spiderlens";

function withAgents() {
  const t = fakeTransport();
  t.results.features = ["base", "extraction", "agents"];
  return t;
}

const panel = () => screen.getByRole("region", { name: "AI agents" });

test("shows_the_score_and_the_four_parts", async () => {
  render(<AgentsOverview transport={withAgents()} path={PATH} />);
  expect((await screen.findByTestId("agents-score")).textContent).toBe("72");
  expect(panel().textContent).toContain("/ 100 AI agent score");
  const parts = within(screen.getByRole("list", { name: "Agent score parts" }));
  for (const name of ["Access", "Discovery", "Reading", "Acting"]) expect(parts.getByText(name)).toBeTruthy();
  expect(parts.getByRole("meter", { name: "Access" }).getAttribute("aria-valuenow")).toBe("90");
  expect(parts.getByText("62.5")).toBeTruthy();
});

test("details_list_failed_rules_and_what_was_not_checked", async () => {
  render(<AgentsOverview transport={withAgents()} path={PATH} />);
  await screen.findByTestId("agents-score");
  expect(screen.queryByRole("region", { name: "Top rules by points lost" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Details" }));
  const rules = within(screen.getByRole("region", { name: "Top rules by points lost" }));
  expect(rules.getAllByRole("listitem")).toHaveLength(3);
  expect(rules.getByText("Form fields without a label")).toBeTruthy();
  expect(rules.getByText("14 pages")).toBeTruthy();
  expect(rules.getByText("Site-wide")).toBeTruthy();
  expect(rules.getByText("-6.4 points")).toBeTruthy();
  const skipped = within(screen.getByRole("region", { name: "Checks that could not run" }));
  expect(skipped.getByText("Challenge pages")).toBeTruthy();
  expect(skipped.getByText("Turned off in this crawl's settings.")).toBeTruthy();
});

test("entry_points_are_listed_per_host_with_their_outcome", async () => {
  render(<AgentsOverview transport={withAgents()} path={PATH} />);
  await screen.findByTestId("agents-score");
  fireEvent.click(screen.getByRole("button", { name: "Details" }));
  const host = within(await screen.findByRole("region", { name: "Entry points for example.com" }));
  const items = host.getAllByRole("listitem");
  expect(items).toHaveLength(3);
  expect(within(items[0]).getByText("AI Catalog")).toBeTruthy();
  expect(within(items[0]).getByText("Found")).toBeTruthy();
  expect(within(items[1]).getByText("Agent card")).toBeTruthy();
  expect(within(items[1]).getByText("Absent")).toBeTruthy();
  expect(within(items[2]).getByText("Invalid")).toBeTruthy();
  expect(within(items[2]).getByText("The API catalog has no `linkset` list.")).toBeTruthy();
});

test("details_show_whether_agents_acting_for_a_person_can_reach_the_site", async () => {
  render(<AgentsOverview transport={withAgents()} path={PATH} />);
  await screen.findByTestId("agents-score");
  expect(screen.queryByRole("region", { name: "AI agent access" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Details" }));
  const items = within(screen.getByRole("region", { name: "AI agent access" })).getAllByRole("listitem");
  expect(items).toHaveLength(4);
  expect(within(items[0]).getByText("ChatGPT-User")).toBeTruthy();
  expect(within(items[0]).getByText("Allowed")).toBeTruthy();
  expect(within(items[0]).getByText("Reached")).toBeTruthy();
  expect(within(items[1]).getByText("Blocked")).toBeTruthy();
  expect(within(items[1]).getByText("Not tested")).toBeTruthy();
  expect(within(items[2]).getByText("Challenged")).toBeTruthy();
  expect(within(items[3]).getByText("Not checked")).toBeTruthy();
});

test("a_crawl_without_agent_checks_says_why_there_is_no_score", async () => {
  const t = fakeTransport();
  render(<AgentsOverview transport={t} path={PATH} />);
  expect(await screen.findByText("AI agent score not available.")).toBeTruthy();
  expect(panel().textContent).toContain("Agent checks have not run on this crawl.");
  expect(screen.queryByTestId("agents-score")).toBeNull();
  expect(screen.queryByRole("button", { name: "Details" })).toBeNull();
});
