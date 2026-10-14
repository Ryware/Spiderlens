// Row windowing for tables of any length (plan D7).
//
// Browsers stop growing a scroll area somewhere between 16 and 33 million
// pixels, which at 28 px a row is about 600,000 rows. Above MAX_SCROLL_PX
// the scroll area stays at that height and a scroll position maps
// proportionally onto the full table. Rows are then placed relative to the
// scroll position, so each one still moves exactly with the wheel.
export const MAX_SCROLL_PX = 1 << 24;
const fullPx = (total, row) => total * row;
export function contentHeight(total, row) {
    return Math.min(fullPx(total, row), MAX_SCROLL_PX);
}
/** Where the viewport sits in the full table for a scroll position. */
export function virtualTop(scrollTop, total, row, viewport) {
    const content = contentHeight(total, row);
    const maxScroll = Math.max(0, content - viewport);
    const maxVirtual = Math.max(0, fullPx(total, row) - viewport);
    if (maxScroll === 0)
        return 0;
    const s = Math.max(0, Math.min(scrollTop, maxScroll));
    return content === fullPx(total, row) ? s : (s / maxScroll) * maxVirtual;
}
/** The scroll position that puts the full table's `top` at the viewport top. */
export function scrollFor(top, total, row, viewport) {
    const content = contentHeight(total, row);
    const maxScroll = Math.max(0, content - viewport);
    const maxVirtual = Math.max(0, fullPx(total, row) - viewport);
    const t = Math.max(0, Math.min(top, maxVirtual));
    if (maxVirtual === 0)
        return 0;
    return content === fullPx(total, row) ? t : (t / maxVirtual) * maxScroll;
}
/** Rows to render for a scroll position, with `overscan` extra on each side. */
export function frame(scrollTop, total, row, viewport, overscan = 6) {
    const top = virtualTop(scrollTop, total, row, viewport);
    const first = Math.max(0, Math.floor(top / row) - overscan);
    const last = Math.min(total, Math.ceil((top + viewport) / row) + overscan);
    const clamped = Math.max(0, Math.min(scrollTop, Math.max(0, contentHeight(total, row) - viewport)));
    return {
        contentPx: contentHeight(total, row),
        virtualTop: top,
        first,
        last,
        firstTopPx: clamped + (first * row - top),
    };
}
/**
 * Scroll position that brings row `i` into view with the least movement,
 * or null when it is already fully visible.
 */
export function scrollToRow(i, scrollTop, total, row, viewport) {
    const top = virtualTop(scrollTop, total, row, viewport);
    const rowTop = i * row;
    if (rowTop < top)
        return scrollFor(rowTop, total, row, viewport);
    if (rowTop + row > top + viewport)
        return scrollFor(rowTop + row - viewport, total, row, viewport);
    return null;
}
