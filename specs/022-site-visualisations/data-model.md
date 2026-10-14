# Data Model: Site Visualisations

## Derived tables (stage 1 `temp`, stage 2 `main`)

### viz_crawl_tree

| Column | Type | Meaning |
|--------|------|---------|
| url_id | INTEGER PK | `urls.id` |
| parent_id | INTEGER NULL | tree parent (plan D2); NULL for the start URL |
| group_source | TEXT NULL | discovery source when the URL hangs under "Found outside links" |
| child_count | INTEGER | direct children |
| subtree_count | INTEGER | all descendants |
| child_rank | INTEGER | order among siblings (Link Score desc, then id) |

Index `(parent_id, child_rank)`.

### viz_dirs

| Column | Type | Meaning |
|--------|------|---------|
| id | INTEGER PK | folder or page node |
| parent_id | INTEGER NULL | parent folder; NULL for host nodes |
| host | TEXT | host |
| path | TEXT | path prefix (folders end in `/`) |
| label | TEXT | last segment, readable form |
| kind | INTEGER | 0 host, 1 folder, 2 page |
| urls, ok, redirect, client_error, server_error, blocked, other | INTEGER | counts for the subtree |

Index `(parent_id, path)`.

### viz_dir_urls

| Column | Type | Meaning |
|--------|------|---------|
| url_id | INTEGER PK | `urls.id` |
| dir_id | INTEGER | page node in `viz_dirs` |

## Scene types (Rust, exported to TypeScript with ts-rs)

- `ViewState { view, layout, colour, size, expanded: NodeId[], pages: {NodeId: number}, focus?: { url_id, distance }, pinned: {NodeId: [x, y]}, options: { nofollow, external_domains, followed_only, stop_words, words_mode, position? }, budget? }`
- `Scene`, `SceneNode`, `SceneLink`, `Legend { colour: [{category, label, count}], size: {by, min, max, fallback?} }`, `Omitted { count, reason, action }` (plan D1)
- `NodeId` = `{ url: number } | { dir: number } | { summary: { parent: NodeId, page: number } } | { group: string } | { domain: string } | { term: number }`
- `AnchorTerm { term, count }`, term links reuse `ListPage<LinkRow>` from spec 007.

## UI state (persisted, `useUiState`)

`viz: { [view]: { layout, colour, size } }`, `vizBudget: { tree, force }`.
