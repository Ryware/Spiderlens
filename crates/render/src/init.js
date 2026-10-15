// Runs in every document of a rendered page before the page's own scripts
// (spec 014 plan D1, FR-010, FR-012). Reports DOM changes for the settle
// rule and turns away everything a page could ask of a user.
(() => {
  const notify = window.__spiderlensMutation;
  if (typeof notify === "function" && window === window.top) {
    let last = 0;
    let timer = null;
    const report = () => {
      last = Date.now();
      notify("");
    };
    // At most one report per 100 ms; the last change is always reported.
    new MutationObserver(() => {
      if (Date.now() - last >= 100) report();
      else if (timer === null)
        timer = setTimeout(() => {
          timer = null;
          report();
        }, 100);
    }).observe(document, { subtree: true, childList: true, attributes: true, characterData: true });
  }
  const refuse = (what) => () => Promise.reject(new DOMException(what + " is not available", "NotAllowedError"));
  // Dialogs are dismissed, pop-ups and WebSockets refused (FR-012, research R6).
  window.alert = () => undefined;
  window.confirm = () => false;
  window.prompt = () => null;
  window.print = () => undefined;
  window.open = () => null;
  window.WebSocket = function () {
    throw new DOMException("WebSockets are not available", "SecurityError");
  };
  // Permission requests are denied.
  if (navigator.permissions) navigator.permissions.query = () => Promise.resolve({ state: "denied", onchange: null });
  if (window.Notification) window.Notification.requestPermission = () => Promise.resolve("denied");
  if (navigator.geolocation) {
    const denied = (ok, err) => err && err({ code: 1, message: "User denied Geolocation" });
    navigator.geolocation.getCurrentPosition = denied;
    navigator.geolocation.watchPosition = (ok, err) => (denied(ok, err), 0);
  }
  if (navigator.mediaDevices) navigator.mediaDevices.getUserMedia = refuse("Camera and microphone");
  for (const f of ["showOpenFilePicker", "showSaveFilePicker", "showDirectoryPicker"]) if (window[f]) window[f] = refuse("File access");
})();
