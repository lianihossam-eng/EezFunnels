# Funnel themes

Every funnel starts with one of five built-in presets. In the typed V2 editor, choose a preset, then customize its colors, typography, spacing, content width, card corners, button shape, and shadow. Each setting can return to the selected preset's value. Changes update the responsive preview and become part of the next immutable revision.

| Preset ID | Style |
|:---|:---|
| `minimal` | Light slate colors, system font, rounded cards, compact shadow |
| `bold` | Dark surfaces, orange accents, larger type and spacing, square cards |
| `elegant` | Warm neutrals, Georgia serif font, soft shadow |
| `playful` | Pink accents, rounded cards and buttons, larger spacing |
| `corporate` | Cool blue palette, Arial font, compact corners and shadow |

The color controls cover the page background, cards and forms, main and secondary text, buttons and accents, text on buttons, and borders. The editor checks the main-text, secondary-text, and button-text combinations against a 4.5:1 contrast target at desktop, tablet, and mobile widths, and reports each combination that falls below it. The warning updates as colors change; it does not prevent a deliberate brand choice.

Style controls include four text scales, five system font families, five section-spacing scales, five card-corner scales, three content widths, four button shapes, and three card shadows. The settings apply across the desktop, tablet, and mobile preview sizes.

Save a revision when the preview is ready. The V2 editor can publish the latest saved revision or unpublish the live revision. V1 funnels keep their existing theme and publication flows.

## Current limits

Per-device theme overrides, user-defined presets, custom CSS, uploaded fonts, and theme import/export are not available in the dashboard. Fonts use system stacks and do not require an external font request. Theme settings belong to each funnel revision; they are not shared account preferences.

For the exact snapshot examples and supported JSON shape, see [`examples/`](../examples/). The example files show revision data for reference; Eez Funnel does not currently import them.
