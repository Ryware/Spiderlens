// The real app in a browser, over the in-memory transport, for the
// end-to-end tests. Never part of the desktop build (only index.html is).
// Tests drive crawl and results events through window.spiderlens.

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "../src/App";
import { fakeReport, fakeTransport } from "../src/testing/fakeTransport";
import { fakeUrls } from "../src/testing/fakeResults";
import "../src/styles/app.css";

const transport = fakeTransport(fakeUrls(5000));
transport.pickCrawlFile = async () => "/crawls/example.com.spiderlens";
const handle = Object.assign(transport, { fakeReport });
export type E2eHandle = typeof handle;
(window as unknown as { spiderlens: E2eHandle }).spiderlens = handle;

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App transport={transport} />
  </StrictMode>,
);
