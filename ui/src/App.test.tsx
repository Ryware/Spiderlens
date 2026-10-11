import { render, screen } from "@testing-library/react";
import { App } from "./App";
import { fakeTransport } from "./testing/fakeTransport";
import type { Transport } from "./transport";

test("shows_identity", async () => {
  render(<App transport={fakeTransport()} />);
  expect(screen.getByRole("heading", { level: 1 }).textContent).toContain("SpiderLens");
  expect((await screen.findByTestId("build-id")).textContent).toContain("abc1234.7");
  expect(screen.getByTestId("product-version").textContent).toContain("0.1.0");
  expect(screen.getByTestId("engine-version").textContent).toContain("0.1.0");
});

test("shows_error_when_engine_unreachable", async () => {
  const broken: Transport = { ...fakeTransport(), buildInfo: () => Promise.reject(new Error("no ipc")) };
  render(<App transport={broken} />);
  expect((await screen.findByText(/Could not read build information/)).textContent).toContain("no ipc");
});
