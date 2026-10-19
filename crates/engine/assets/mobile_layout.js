(args) => {
  const MAX = 20000;
  // The layout viewport: on a phone, content wider than the screen zooms the
  // page out and grows innerWidth with it.
  const win = (document.documentElement && document.documentElement.clientWidth) || window.innerWidth;
  const content = Math.max(document.documentElement.scrollWidth, document.body ? document.body.scrollWidth : 0);
  const esc = (s) => (window.CSS && CSS.escape ? CSS.escape(s) : String(s).replace(/[^\w-]/g, '\\$&'));
  const part = (el) => {
    const tag = el.tagName.toLowerCase();
    if (el.id) return tag + '#' + esc(el.id);
    const cls = el.classList && el.classList[0];
    if (cls) return tag + '.' + esc(cls);
    let n = 1;
    for (let s = el.previousElementSibling; s; s = s.previousElementSibling) if (s.tagName === el.tagName) n += 1;
    return tag + ':nth-of-type(' + n + ')';
  };
  const selector = (el) => {
    const parts = [];
    for (let e = el, i = 0; e && e.nodeType === 1 && i < 3; e = e.parentElement, i += 1) {
      parts.unshift(part(e));
      if (e.id) break;
    }
    return parts.join(' > ');
  };
  const visible = (el, cs) => {
    if (cs.display === 'none' || cs.visibility === 'hidden' || cs.visibility === 'collapse') return false;
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  };

  // Widest element beyond the window, and the interactive targets.
  let widest = null, widestRight = win, visited = 0;
  const all = document.body ? document.body.getElementsByTagName('*') : [];
  const interactive = [];
  const targetSel = 'a[href], button, input, select, textarea, [role=button], [onclick]';
  for (let i = 0; i < all.length && visited < MAX; i += 1, visited += 1) {
    const el = all[i];
    const cs = getComputedStyle(el);
    if (!visible(el, cs)) continue;
    const r = el.getBoundingClientRect();
    if (r.right > widestRight) { widestRight = r.right; widest = el; }
    if (el.matches(targetSel) && !(el.tagName === 'INPUT' && el.type === 'hidden')) interactive.push(el);
  }

  // Small text share, by characters.
  let total = 0, small = 0, seen = 0;
  if (document.body) {
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    const cache = new Map();
    for (let n = walker.nextNode(); n && seen < MAX; n = walker.nextNode(), seen += 1) {
      const len = n.nodeValue.replace(/\s+/g, ' ').trim().length;
      if (!len) continue;
      const p = n.parentElement;
      if (!p) continue;
      let size = cache.get(p);
      if (size === undefined) {
        const cs = getComputedStyle(p);
        size = visible(p, cs) ? parseFloat(cs.fontSize) : null;
        cache.set(p, size);
      }
      if (size === null) continue;
      total += len;
      if (size < args.small_text_px) small += len;
    }
  }

  // Small tap targets, except links inside a sentence (WCAG 2.5.8).
  const inlineLink = (el) => {
    if (el.tagName !== 'A') return false;
    let b = el.parentElement;
    if (!b) return false;
    while (b.parentElement) {
      const d = getComputedStyle(b).display;
      if (d !== 'inline' && d !== 'contents') break;
      b = b.parentElement;
    }
    // Text of the line itself: not inside other targets or nested blocks
    // (a list of links next to a heading is not a sentence).
    let text = 0;
    const walk = (node) => {
      for (const c of node.childNodes) {
        if (c.nodeType === 3) text += c.nodeValue.trim().length;
        else if (c.nodeType === 1 && !c.matches(targetSel)) {
          const d = getComputedStyle(c).display;
          if (d === 'inline' || d === 'contents') walk(c);
        }
      }
    };
    walk(b);
    return text > 0;
  };
  let smallTargets = 0;
  const examples = [];
  for (const el of interactive) {
    const r = el.getBoundingClientRect();
    if (r.width >= args.tap_target_px && r.height >= args.tap_target_px) continue;
    if (inlineLink(el)) continue;
    smallTargets += 1;
    if (examples.length < args.examples) {
      examples.push({ selector: selector(el), w: Math.round(r.width), h: Math.round(r.height) });
    }
  }
  return {
    window_px: win,
    content_px: content,
    widest: widest ? selector(widest) : null,
    small_text_share: total ? Math.round((small / total) * 1000) / 10 : 0,
    small_targets: smallTargets,
    examples,
  };
}
