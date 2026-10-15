# Themes and editor settings

Every funnel starts with one of five built-in theme presets. Open a funnel from **Funnels**, then use the editor's **Theme** controls to choose a preset, set a primary color, or toggle square button corners. Save the change as a new revision when it is ready.

| Preset ID | Included visual tokens |
|:---|:---|
| `minimal` | Light slate colors, system font, rounded cards, compact shadow |
| `bold` | Dark surfaces, orange primary color, larger type and spacing, square cards and buttons |
| `elegant` | Warm neutral colors, Georgia serif font, rounded cards, soft shadow |
| `playful` | Pink accents, rounded cards and buttons, larger spacing |
| `corporate` | Cool blue palette, Arial font, compact corners and shadow |

The preset defines the page background, surfaces, text and border colors, font family and scale, button/card corners, card shadow, and section spacing. The editor can override the primary color and button-corner toggle while retaining the selected preset. Reset the color control to restore the preset's primary color.

## Edit section content

Select a section in the left outline to expose its settings in the inspector. Available fields depend on the section. Current controls include text, color, select, toggle, and image fields. Examples include the hero headline and subheading, feature and testimonial text, call-to-action label and URL, footer links, and hero image URL and alternative text.

The preview refreshes as you edit. It offers Mobile (375 px), Tablet (768 px), and Desktop (1280 px) widths. A hero image appears as a safe placeholder in the editor preview; a saved published page loads the configured image URL. The browser will make that request to the image host when a visitor opens the page.

Use **Undo** or **Redo** before saving to move through editor changes. **Save revision** stores an immutable revision. To make it public, publish the saved revision from the funnel workflow.

## Current limits

You can select presets and adjust the supported primary-color and square-button overrides. User-defined presets, custom CSS, custom font uploads, and theme import/export are not available. The theme settings are part of each saved revision; they are not global account settings.

For the exact snapshot examples and the supported JSON shape, see [`examples/`](../examples/). The example files show revision data for reference; Eez Funnel does not currently import them.
