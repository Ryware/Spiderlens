// The canvas maths (spec 022 plan D7): camera, hit grid, arrow keys, labels.
import type { SceneNode } from "../generated/SceneNode";
import { fit, HitGrid, labelled, nearest, toScreen, toWorld, zoomAt } from "./geometry";

const node = (key: string, x: number, y: number, r = 5, depth = 1): SceneNode => ({
  key,
  label: key,
  url: null,
  url_id: null,
  x,
  y,
  r,
  category: "ok",
  depth,
  score: null,
  inlinks: null,
  count: 1,
  hidden: 0,
  expandable: false,
  expanded: false,
  parent: null,
});

test("fit_shows_the_whole_scene_centred", () => {
  const c = fit({ x: 0, y: 0, width: 1000, height: 500 }, 600, 400, 50);
  expect(c.k).toBeCloseTo(0.5);
  const a = toScreen(c, 0, 0);
  const b = toScreen(c, 1000, 500);
  expect(a.x).toBeCloseTo(50);
  expect(b.x).toBeCloseTo(550);
  expect((a.y + b.y) / 2).toBeCloseTo(200);
});

test("fit_does_not_blow_up_a_tiny_scene", () => {
  expect(fit({ x: 0, y: 0, width: 10, height: 10 }, 800, 600).k).toBeLessThanOrEqual(1.5);
});

test("zoom_keeps_the_point_under_the_pointer_still", () => {
  const c = { x: 30, y: -20, k: 0.7 };
  const before = toWorld(c, 200, 150);
  const z = zoomAt(c, 200, 150, 2);
  expect(z.k).toBeCloseTo(1.4);
  const after = toWorld(z, 200, 150);
  expect(after.x).toBeCloseTo(before.x);
  expect(after.y).toBeCloseTo(before.y);
});

test("zoom_is_clamped", () => {
  expect(zoomAt({ x: 0, y: 0, k: 1 }, 0, 0, 1e6).k).toBe(8);
  expect(zoomAt({ x: 0, y: 0, k: 1 }, 0, 0, 1e-6).k).toBe(0.02);
});

test("hit_grid_finds_the_node_under_a_point", () => {
  const nodes = [node("a", 0, 0), node("b", 100, 0), node("c", 103, 0)];
  const g = new HitGrid(nodes);
  expect(g.at(1, 1)).toBe(0);
  expect(g.at(50, 0)).toBeNull();
  expect(g.at(102, 0)).toBe(2);
  expect(g.at(-8, 0)).toBeNull();
  expect(g.at(-8, 0, 4)).toBe(0);
});

test("arrow_keys_move_to_the_nearest_node_in_that_direction", () => {
  const nodes = [node("root", 0, 50), node("a", 200, 0), node("b", 200, 100), node("a1", 400, 0)];
  expect(nearest(nodes, 0, "right")).toBe(0 + 1);
  expect(nearest(nodes, 1, "down")).toBe(2);
  expect(nearest(nodes, 1, "right")).toBe(3);
  expect(nearest(nodes, 3, "right")).toBeNull();
  expect(nearest(nodes, 2, "left")).toBe(0);
});

test("labels_need_room_and_stay_within_the_budget", () => {
  const nodes = Array.from({ length: 500 }, (_, i) => node(`n${i}`, i * 30, 0, i === 7 ? 12 : 5, i % 3));
  expect(labelled(nodes, 0.5)).toEqual([7]);
  const all = labelled(nodes, 2, 300);
  expect(all).toHaveLength(300);
  expect(all[0]).toBe(7);
  const inView = labelled(nodes, 2, 300, 4, { x0: 0, y0: -10, x1: 100, y1: 10 });
  expect(inView.sort((a, b) => a - b)).toEqual([0, 1, 2, 3]);
});
