async (args) => {
  const t0 = performance.now();
  const tags = [
    'wcag2a', 'wcag2aa', 'wcag2aaa',
    'wcag21a', 'wcag21aa', 'wcag21aaa',
    'wcag22a', 'wcag22aa', 'wcag22aaa',
    'best-practice',
  ];
  const raw = await axe.run(document, {
    runOnly: { type: 'tag', values: tags },
    resultTypes: ['violations', 'incomplete'],
    iframes: false,
    elementRef: false,
    selectors: true,
    ancestry: false,
    xpath: false,
    performanceTimer: false,
  });
  const nodeCap = args.node_cap, pageCap = args.page_cap, excerptMax = args.excerpt;
  const sx = window.scrollX || window.pageXOffset || 0;
  const sy = window.scrollY || window.pageYOffset || 0;
  const round = (n) => Math.round(n);
  const startTag = (html) => {
    let s = String(html || '');
    const gt = s.indexOf('>');
    if (gt >= 0) s = s.slice(0, gt + 1);
    return s.length > excerptMax ? s.slice(0, excerptMax) : s;
  };
  const contrast = (node) => {
    const checks = [].concat(node.any || [], node.all || [], node.none || []);
    for (const c of checks) {
      const d = c && c.data;
      if (d && typeof d === 'object' && (d.fgColor || d.bgColor || d.contrastRatio !== undefined)) {
        const px = parseFloat(d.fontSize);
        return {
          fg: d.fgColor,
          bg: d.bgColor,
          ratio: d.contrastRatio,
          expected: d.expectedContrastRatio,
          font_px: Number.isFinite(px) ? px : null,
          bold: Number(d.fontWeight) >= 700 || d.fontWeight === 'bold',
        };
      }
    }
    return null;
  };
  const box = (selector) => {
    try {
      const el = document.querySelector(selector);
      if (!el) return null;
      const r = el.getBoundingClientRect();
      return [round(r.left + sx), round(r.top + sy), round(r.width), round(r.height)];
    } catch (e) {
      return null;
    }
  };
  let used = 0;
  const reduce = (list, kind) => list.map((rule) => {
    const nodes = [];
    for (const n of rule.nodes) {
      if (nodes.length >= nodeCap || used >= pageCap) break;
      const selector = Array.isArray(n.target) ? n.target.join(' ') : String(n.target);
      const node = { selector, excerpt: startTag(n.html) };
      const data = contrast(n);
      if (data) node.data = data;
      const b = box(selector);
      if (b) node.box = b;
      nodes.push(node);
      used += 1;
    }
    return {
      id: rule.id,
      kind,
      impact: rule.impact || null,
      tags: rule.tags || [],
      total: rule.nodes.length,
      nodes,
    };
  });
  const results = reduce(raw.violations || [], 'violation').concat(reduce(raw.incomplete || [], 'needs_review'));
  return { engine: axe.version, ms: round(performance.now() - t0), results };
}
