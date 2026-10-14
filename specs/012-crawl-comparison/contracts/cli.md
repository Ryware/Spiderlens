# CLI Contract: `spiderlens compare`

```
spiderlens compare <PREVIOUS> <CURRENT> [options]
spiderlens compare --baseline schedule <CURRENT> [options]   (with 021)
```

| Option | Meaning |
|---|---|
| `--map <FIND>=><REPLACE>` | plain-text mapping rule, repeatable, in order |
| `--map-regex <PATTERN>=><REPLACE>` | regex mapping rule with `$1` groups, repeatable |
| `--settings <FILE>` | comparison settings file saved from the app (mapping, elements, thresholds, issue settings) |
| `--issue-settings current\|previous\|app` | which issue settings both sides use (default current) |
| `--json` | print the full `ComparisonReport` as JSON instead of the text summary |
| `--export <DIR>` | write one CSV per non-empty group (names below) |
| `--fail-on-new-issues <high\|medium\|low\|opportunity>` | fail when an issue at or above that priority has newly affected URLs (with 009) |
| `--fail-on-missing <N>` | fail when N or more internal HTML pages are Missing |
| `--fail-on-lost-indexable <N>` | fail when N or more pages went from indexable to non-indexable |

`--map`, `--map-regex` and `--settings` combine: rules from the settings
file first, then command-line rules.

## Export file names

`filter-<tab>-<filter>-<new|added|removed|missing>.csv`,
`changed-<element>.csv`, `issue-<rule>-<newly-affected|fixed|gone>.csv`,
`urls-<new|missing|not-reached|collisions>.csv`, `migration-<kind>.csv`.
CSV rules as spec 007 FR-029. Columns: Address (previous), Address
(current), then previous and current values of the group's columns.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | compared; no failure condition met |
| 2 | usage error, unreadable file, same file twice, newer file it cannot read |
| `COMPARE_FAILED` (next free at merge, expected 8) | one or more `--fail-on-*` conditions met; each is named on stderr |
