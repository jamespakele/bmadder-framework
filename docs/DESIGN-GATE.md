# Design Gate: Google Stitch Workflow

The design gate sits between planning and development. It produces visual design artifacts that dev agents reference during frontend implementation. Without it, agents invent their own visual styles — which is fine for prototypes but terrible for production.

---

## Why Design Before Code

Agents are good at implementing designs. They're bad at inventing them. Without a design reference:
- Every frontend story produces a different visual style
- Color palettes, spacing, and typography are inconsistent
- QA can't verify visual correctness (no spec to compare against)
- You end up rewriting UI after the fact

The design gate gives agents a visual contract: "build this, not whatever you imagine."

---

## The Stitch Workflow

### Step 1: Open Google Stitch

Navigate to [Google Stitch](https://stitch.google.com) (or your design tool of choice — the workflow is tool-agnostic, Stitch is just what we tested with).

### Step 2: Generate Designs

Use natural language prompts that reference your PRD. Good prompts:

```
Design a dashboard for [app name] that shows:
- A summary card row at the top (3-4 metrics)
- A data table below with sortable columns
- A sidebar with navigation links
- Use a clean, professional style with blue as the primary color

The app is [brief description from your PRD].
```

```
Design a settings page with:
- A tabbed layout (General, Notifications, Account)
- Form fields for each setting described in the PRD
- Save/Cancel buttons
- Consistent with the dashboard design
```

### Step 3: Iterate

Generate multiple variations. Pick the one that fits your PRD. Make sure:
- All pages share a consistent visual language
- Colors, fonts, and spacing are uniform
- Component patterns are reusable (buttons, cards, tables look the same everywhere)

### Step 4: Export Design Artifacts

From your finalized Stitch designs, extract:

1. **Design tokens** — colors, typography, spacing, shadows, borders
2. **Layout templates** — page-level layout structures
3. **Component patterns** — reusable UI component shapes

---

## Creating the Scaffolding Directory

```bash
mkdir -p src/scaffolding/layouts src/scaffolding/components
```

### tokens.md

Create `src/scaffolding/tokens.md` using the template at `.deprecated/templates/tokens-template.md`. Fill in actual values from your Stitch export:

```markdown
# Design Tokens

## Colors

### Primary
| Token | Value | Usage |
|-------|-------|-------|
| `--color-primary` | `#2563EB` | Primary actions, links |
| `--color-primary-hover` | `#1D4ED8` | Hover state |
...
```

This file is the single source of truth for visual design. Dev agents read it before writing any CSS.

### Layout Templates

Create layout descriptions in `src/scaffolding/layouts/`. Example:

**src/scaffolding/layouts/dashboard.md:**
```markdown
# Dashboard Layout

## Structure
- Fixed sidebar (240px) on the left
- Main content area fills remaining width
- Top bar (64px) with app title and user menu
- Content area has 24px padding

## Grid
- Summary cards: 4-column grid, 16px gap
- Data table: full width below cards
- Responsive: cards stack to 2-col at tablet, 1-col at mobile
```

### Component Templates

Create component descriptions in `src/scaffolding/components/`. Example:

**src/scaffolding/components/data-table.md:**
```markdown
# Data Table Component

## Structure
- Header row with sortable column labels
- Data rows with hover highlight
- Pagination at bottom (10/25/50 per page)

## Styling
- Border: 1px solid var(--color-border)
- Header bg: var(--color-surface)
- Row hover: var(--color-primary-light)
- Font size: var(--font-size-sm)
- Cell padding: var(--space-3) var(--space-4)
```

---

## How Agents Use Scaffolding

The dev agent is routed to standard workflows that instruct it to search for and read any existing `src/scaffolding/` files before generating user interface code. Specifically, the agent is instructed:
1. To look for `src/scaffolding/tokens.md` for design tokens (colors, fonts, spacing).
2. To look for layout templates in `src/scaffolding/layouts/`.
3. To look for reusable UI components in `src/scaffolding/components/`.

With these rules, agents are explicitly aligned to reference the design system scaffolding and match the style tokens instead of inventing new visual parameters on the fly.

---

## Best Practices

1. **Design all pages before coding any.** Consistency comes from designing holistically, not page by page.

2. **Keep tokens.md as the single source.** If you change a color, change it in tokens.md. Agents will pick up the change on the next iteration.

3. **Describe layouts structurally, not pixel-perfectly.** Agents need to understand the layout intent (sidebar + main content + top bar) more than exact pixel positions.

4. **Component descriptions should reference token values.** Use `var(--color-primary)` not `#2563EB` in component docs. This ensures consistency even if tokens change.

5. **Don't over-specify.** Give agents enough to build consistently, but let them handle implementation details. You're defining the design language, not writing the CSS yourself.

6. **Commit scaffolding before running dev.** The pre-dev worktree commit captures scaffolding in git, ensuring agents see the latest version.

---

## Skipping the Design Gate

For backend-only projects, API services, or CLI tools — skip this entirely. The design gate is only for projects with user-facing UI.

For rapid prototyping with UI — you can skip Stitch and let agents invent their own styles. Just know that visual consistency will suffer and you'll likely redo the UI later.
