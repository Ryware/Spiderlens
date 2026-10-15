// Serialises the rendered page (spec 014 plan D1 "Output"). Evaluated as an
// expression; returns the doctype plus the document's HTML.
//
// When the page has open shadow roots or same-origin iframes with content,
// they are flattened into the serialisation, as Google and Screaming Frog
// do: a shadow host's children are its shadow tree with each <slot>
// replaced by the nodes assigned to it, and a same-origin iframe is followed
// by its body's children in a <div data-spiderlens-frame>. Otherwise the
// browser's own outerHTML is used, which is faster.
(() => {
  const VOID = new Set([
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta",
    "source", "track", "wbr", "param", "keygen", "basefont", "bgsound", "frame",
  ]);
  const RAW = new Set(["script", "style", "xmp", "iframe", "noembed", "noframes", "plaintext", "noscript"]);

  const frameBody = (el) => {
    try {
      const d = el.contentDocument;
      return d && d.body && d.body.childNodes.length ? d.body : null;
    } catch (e) {
      return null;
    }
  };

  const needsFlattening = () => {
    for (const el of document.querySelectorAll("*")) {
      if (el.shadowRoot) return true;
      if ((el.localName === "iframe" || el.localName === "frame") && frameBody(el)) return true;
    }
    return false;
  };

  const escText = (s) =>
    s.replace(/&/g, "&amp;").replace(/ /g, "&nbsp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const escAttr = (s) => s.replace(/&/g, "&amp;").replace(/ /g, "&nbsp;").replace(/"/g, "&quot;");

  const out = [];
  const children = (nodes, raw) => {
    for (const n of nodes) node(n, raw);
  };
  const node = (n, raw) => {
    if (n.nodeType === 3) {
      out.push(raw ? n.data : escText(n.data));
      return;
    }
    if (n.nodeType === 8) {
      out.push("<!--" + n.data + "-->");
      return;
    }
    if (n.nodeType !== 1) return;
    const name = n.localName;
    if (name === "slot" && n.getRootNode() instanceof ShadowRoot) {
      const assigned = n.assignedNodes({ flatten: true });
      children(assigned.length ? assigned : n.childNodes, false);
      return;
    }
    out.push("<" + name);
    for (const a of n.attributes) out.push(" " + a.name + '="' + escAttr(a.value) + '"');
    out.push(">");
    if (VOID.has(name)) return;
    if (name === "template") children(n.content.childNodes, false);
    else if (n.shadowRoot) children(n.shadowRoot.childNodes, false);
    else children(n.childNodes, RAW.has(name));
    out.push("</" + name + ">");
    if (name === "iframe") {
      const body = frameBody(n);
      if (body) {
        out.push('<div data-spiderlens-frame="' + escAttr(n.getAttribute("src") || "") + '">');
        children(body.childNodes, false);
        out.push("</div>");
      }
    }
  };

  const dt = document.doctype;
  let doctype = "";
  if (dt) {
    doctype = "<!DOCTYPE " + dt.name;
    if (dt.publicId) doctype += ' PUBLIC "' + dt.publicId + '"';
    if (dt.systemId) doctype += (dt.publicId ? "" : " SYSTEM") + ' "' + dt.systemId + '"';
    doctype += ">";
  }
  const root = document.documentElement;
  if (!root) return doctype;
  if (!needsFlattening()) return doctype + root.outerHTML;
  node(root, false);
  return doctype + out.join("");
})()
