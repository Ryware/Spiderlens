import { render, screen } from "@testing-library/react";
import { App } from "./App";
import type { Transport } from "./transport";

const fake: Transport = {
  buildInfo: async () => ({
    product: "SpiderLens",
    product_version: "0.1.0",
    engine_version: "0.1.0",
    build_id: "abc1234.7",
    commit: "abc1234def",
  }),
};

test("shows_identity", async () => {
  render(<App transport={fake} />);
  expect(screen.getByRole("heading").textContent).toContain("SpiderLens");
  expect((await screen.findByTestId("build-id")).textContent).toContain("abc1234.7");
  expect(screen.getByTestId("product-version").textContent).toContain("0.1.0");
  expect(screen.getByTestId("engine-version").textContent).toContain("0.1.0");
});

test("shows_error_when_engine_unreachable", async () => {
  const broken: Transport = { buildInfo: () => Promise.reject(new Error("no ipc")) };
  render(<App transport={broken} />);
  expect((await screen.findByRole("alert")).textContent).toContain("no ipc");
});
