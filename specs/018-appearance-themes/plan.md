# Implementation Plan: Appearance, Brightness Levels and Accent Palettes

**Branch**: `018-appearance` | **Date**: 2026-10-13 | **Spec**: [spec.md](spec.md) | **Code**: PR #26

## Summary

Six brightness steps and six accent palettes as blocks in the existing token file, selected by data attributes on `<html>`. A toolbar popover with a vertical slider, an Appearance page in the settings sheet, and the choice kept per machine in the UI state. Design decisions are in [research.md](research.md) R2 to R6.

## Technical Context

- UI only, plus one Tauri capability (`core:window:allow-set-theme`). No change to the engine, config crate or CLI.
- React 18, Base UI 1.9 (Popover, Switch), Tailwind v4 with tokens only, Vitest + Testing Library.
- Constitution: Principle III kept (engine untouched); Principle VII through the token test and UI tests; Principle VIII one token file.

## Structure

| File | Change |
|---|---|
| `ui/src/styles/tokens.css` | Soft, Island, Deep, Midnight blocks; five palette blocks (light and dark tone, plus Island chrome); swatch and preview values; `--frame`, `--ladder` |
| `ui/src/styles/app.css` | `bg-frame`, `bg-ladder`, `bg-swatch`, preview utilities; Island inset panel |
| `ui/src/shell/appearance.ts` | Step and palette lists, resolve and choose rules, apply to `<html>` |
| `ui/src/shell/storage.ts` | UI state v2, v1 migration, per-field fallback |
| `ui/src/shell/useUiState.ts` | `useAppearance` (system follow, window tone), `applyStoredAppearance` |
| `ui/src/main.tsx` | Apply stored appearance before first render |
| `ui/src/shell/BrightnessPopover.tsx`, `levelIcons.tsx` | Toolbar control |
| `ui/src/settings/AppearanceSettings.tsx`, `categories.tsx`, `SettingsSheet.tsx` | Appearance page in an App group |
| `ui/src/shell/AppShell.tsx`, `Toolbar.tsx`, `panels.tsx` | Chrome regions, Island wrapper, commands |
| `app/src-tauri/capabilities/default.json` | `core:window:allow-set-theme` |

## Testing

- `tokens.test.ts`: every step and palette pair, plus Island's two regions, against the spec 017 contrast rules; status hue distance; swatch and preview values match their blocks; palette and step lists match the app.
- `appearance.test.ts`: resolve and choose rules, v1 migration, unknown values, first-paint apply.
- `BrightnessPopover.test.tsx`, `AppearanceSettings.test.tsx`, `AppShell.test.tsx`: the user stories end to end against the fake transport.
