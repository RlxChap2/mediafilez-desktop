# MediaFilez Desktop design

A locked design system for this app. Every product view follows this file.
Amend it intentionally; do not override it per component.

## System

- Genre · modern-minimal
- App macrostructure · Workbench
- Theme · custom: quiet technical, local-first, dependable
- Axes · cool-tinted paper / geometric sans / teal-green
- Identity · MediaFilez Desktop; the cloud-download mark is the only decorative asset

## Typography

- Display · Sora Variable 600 to 700, roman, tight tracking
- Body · Inter Variable 400 to 650
- Numeric data · Inter with tabular figures
- No uppercase decorative eyebrows, italic headings, or marketing hero copy

## Colour and spacing

- Brand anchors · `#00857E` and `#25CF7D`; interface colours use their OKLCH roles
- `tokens.css` is canonical; components never introduce one-off colours or fonts
- Spacing follows a 4 px scale; controls share a 44 px minimum height
- One containment layer, fine rules, restrained shadows, 8 to 12 px corner radii

## App structure

- Header · compact identity, engine state, theme, settings, source
- Composer · visible URL label, paste/clear actions, Video · Image · Audio modes
- Workbench · download controls beside the live queue; one column when space breaks
- Advanced settings · muted video, authentication, authorized Cobalt pool, experimental proxy
- Footer · version and local/privacy status in one line

## Interaction

- Motion · progress movement, button press, and dialog transition only
- Success is visible in the queue; no celebratory toast
- Errors name the failed engine and a recovery action
- Focus is instant; every pointer action has a keyboard equivalent
- Reduced motion keeps state changes and removes spatial movement

## Voice

- Calm, technical, and literal. Name the file, engine, state, or next action.
- Say “Try this link”, never claim that every site is guaranteed.
- Primary actions use a teal fill and a direct verb; secondary actions use a quiet outline.

## Source

`src/styles/tokens.css` is the only design-token source. `src/styles/app.css` owns component and layout rules.
