# CLI contract: Site Visualisations

```text
spiderlens viz <file> --view crawl-tree|directory-tree|link-graph|anchors
    [--layout tree|radial|force]            default: tree (trees), force (link-graph)
    [--colour status|indexability|depth]    default: status
    [--size score|inlinks|uniform]          default: score (falls back to inlinks)
    [--focus <URL> [--distance 1|2]]        link-graph focus mode
    [--url <URL>]                           anchors for one URL (default: site-wide)
    [--words | --anchors] [--followed-only] [--keep-stop-words]
    [--theme light|dark]                    default: light
    [--svg <out.svg>] [--png <out.png> [--scale 1|2|4]] [--csv <out.csv>]
```

At least one output is required. The scene equals the app's initial state for the same view and options, so the SVG and CSV are byte-identical to the desktop export of that state (FR-025). Exit 0 on success, 2 on usage error, 3 when the file cannot be opened, 4 when the PNG would exceed 16,000 px on a side.
