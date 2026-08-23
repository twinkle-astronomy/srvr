# Plan: Grayscale-Aware Dashboard Previews

**Branch:** `preview-grayscale-mode` (off `2-bit-grayscale-support`, which is
unmerged — this work depends on `render_screen_2bit_png` and the
`supports_2bit_grayscale` column)

## What & why

[2-bit grayscale support](../completed/20260823-2-bit-grayscale-support.md)
shipped with a known gap, recorded in its own Limitations section: **every
dashboard preview is still 1-bit BMP regardless of the device's mode.** An
admin who enables grayscale on a device has no way to see what that device
will actually display, and a template author has no way to check their
template reads correctly in four gray levels rather than pure black/white.

This closes that gap in all three preview surfaces:

| Surface | File | Mode should follow |
|---|---|---|
| Device detail page | `src/frontend/pages/devices.rs` | that device's configured flag |
| Manual template editor | `src/frontend/pages/template_editor/template_preview.rs` | the selected **Preview Device** |
| AI template generator | `src/frontend/pages/ai_template_generator/mod.rs` | the selected **Preview Device** (dropdown does not exist yet — add it) |

## Files to be modified

**Backend**
- `src/api/devices.rs` — `get_screen_preview`, `get_screen_preview_for_template`
- `src/api/templates.rs` — `get_template_preview`; delete `get_template_preview_png` (see Decision 2)

**Frontend**
- `src/frontend/api.rs`, `src/frontend/server_fns.rs` — drop the `_png` helper
- `src/frontend/pages/devices.rs` — data URL mime
- `src/frontend/pages/template_editor/template_preview.rs` — data URL mime
- `src/frontend/pages/template_editor/template_form.rs` — use the shared selector
- `src/frontend/pages/ai_template_generator/mod.rs` — data URL mime, device
  dropdown, per-device render context, generation bump on switch (§4),
  grayscale-aware system prompt (§4b)
- `src/frontend/components/` — new shared `PreviewDeviceSelector` (see Decision 3)

## How

### 1. Server decides the mode; the client never branches

Both preview endpoint families already have the device in hand:

- `/dashboard/devices/{id}/preview[/{template_id}]` loads the `Device` row
  from the DB, so `device.supports_2bit_grayscale` is right there.
- `/preview` receives the whole `RenderContext` in the POST body, and
  `ctx.device` now carries the flag.

So each handler branches:

```rust
let image = if ctx.device.supports_2bit_grayscale {
    renderer::render_screen_2bit_png(&ctx).await
} else {
    renderer::render_screen_png(&ctx).await
};
```

No endpoint signature changes, no new query params, and the client never has
to duplicate the server's decision.

### 2. Previews become **always PNG** (behavior change)

Today `/preview` and `/dashboard/devices/{id}/preview` return a bare base64
`String` that the client blindly wraps in `data:image/bmp;base64,…`. Once the
response can be either format, that implicit contract breaks — the client
can't know which it got without re-deriving the server's branch.

Rather than add a mime field to the response, previews standardize on PNG:
`render_screen_png` is already documented as *pixel-for-pixel identical* to
`render_screen`'s BMP, so 1-bit previews look exactly the same as today. All
three call sites change `data:image/bmp` → `data:image/png`.

This is a real contract change to two endpoints, but the response *type* is
unchanged (`Json<String>`), and every consumer is in this repo.

### 3. Consolidate `/preview` and `/preview/png`

`/preview/png` exists solely because Claude's vision input can't read BMP. If
`/preview` always returns PNG, the two are byte-identical for 1-bit devices —
and the AI generator currently calls **both** on every `render_template` tool
call, rasterizing the same SVG twice per render.

Deleting `/preview/png` therefore:
- removes a redundant full SVG rasterization + round trip per AI render,
- guarantees Claude sees exactly what the user sees, including grayscale.

`ai_generator.rs`'s tests only cover tool dispatch/parsing, so they are
unaffected. `api::templates::preview_png_returns_a_decodable_png` gets
retargeted at `/preview`.

### 4. AI generator: add a Preview Device dropdown

The page currently hardcodes `get_virtual_render_context(id)`. It gains the
same device selection the manual editor has — but this page has more moving
parts than the manual editor, and the details below were **verified against
the code**, correcting an earlier draft of this plan that asserted them from
assumption:

- Selecting a device swaps the render context via
  `get_render_context_for_template(device_id, template_id)`.

- **The proposal survives a switch.** Confirmed: `proposal` is a separate
  signal from `render_context`, and `assemble_render_context` (`api/mod.rs:74`)
  fetches queries by `template.id`, not by device — so refetching the context
  for a different device returns identical query lists. Save reads `proposal()`
  for content and `ctx` only for the template id + delete-list, both
  device-independent. Switching devices cannot corrupt a pending Save.

- **The device is snapshotted per-send, not per-render.** `mod.rs:518` does
  `let device = ctx.device.clone()` once inside `send_message` and moves it
  into the tool-call closure; `mod.rs:551` clones that captured value per
  call. So an in-flight turn keeps rendering for whichever device was
  selected when Send was pressed. The *next* turn picks up the new device
  because `send_message` re-reads `render_context()` at `mod.rs:476`.
  Keeping a turn internally consistent is the right behavior — but it means:

- **A device switch mid-turn races the in-flight render.** The switch handler
  re-renders the proposal for the new device and sets `preview_image`; the
  in-flight `render_template` then completes and overwrites it at `mod.rs:398`
  with a render for the *old* device. The existing generation counter does
  **not** cover this — a device switch doesn't bump `generation`, so the stale
  write lands. Fix: bump `generation` on device switch too, which makes the
  existing guard at `mod.rs:391` drop the superseded render. (Disabling the
  dropdown while `busy` is the cheaper alternative, but it blocks a
  legitimate action for the length of a multi-tool-call turn.)

- **The initial load must not clobber the proposal.** The `use_resource` at
  `mod.rs:699` sets `preview_image` from the *saved* template. It cannot
  simply gain a dependency on the selected device, or every switch would
  overwrite the proposal preview. The device-switch path stays separate:
  refetch context, then re-render `proposal()` if one exists, else the saved
  template.

### 4b. The system prompt hardcodes 1-bit — it has to vary by device

`build_system_prompt` (`mod.rs:208-218`) currently tells Claude the output is
"converted to a **1-bit black/white BMP**" and to "use only black and white
(**no grayscale** or color; the display is 1-bit)".

Selecting a 2-bit device while leaving that text in place would leave the
feature half-wired: the preview would be grayscale-capable, but Claude would
still be under explicit instructions not to produce grayscale. The prompt
must branch on `ctx.device.supports_2bit_grayscale`, telling Claude it has
four levels available (`#000000 #555555 #aaaaaa #ffffff`) when the selected
device supports them.

This is rebuilt per send (`mod.rs:517`), so a switch between turns takes
effect on the next turn with no extra plumbing.

### 5. Shared `PreviewDeviceSelector`

The manual editor's selector is inline in `template_form.rs` and has a wart:
it offers the virtual device *only* when you own no real devices, so anyone
with hardware can't preview against 800×480 virtual. The AI page needs the
same control. Extract one component used by both, with **Virtual Device
always present as an explicit option**.

Each option is annotated with its mode so the dropdown itself explains why
the preview looks different — e.g. `kitchen-display (800×480, 2-bit)`.

## Tests

- **`src/api/devices.rs`** (inline): device with the flag set → preview bytes
  decode as a PNG whose raw `bit_depth == Two` (via `png::Decoder`, per the
  grayscale project's finding that `image`'s decoder expands sub-8-bit
  depths); flag clear → 8-bit PNG. Both need a dedicated template assigned,
  per the [shared-default-template trap](../../testing.md#database-tests).
- **`src/api/templates.rs`** (inline): same two assertions driven through
  `/preview` with a hand-built `RenderContext`.
- **Browser E2E** (`tests/browser_e2e.rs`): toggle grayscale on a device,
  reload the device page, assert the preview `<img>` `src` starts with
  `data:image/png`. Guards the client half, which unit tests can't reach.

## Open questions / tradeoffs

1. **Always-PNG previews (Decision 2)** — the alternative is returning
   `{data, mime}` and keeping BMP for 1-bit. That's more faithful to what the
   device actually receives (a BMP-encoder bug would show up in preview), but
   it's a wider response-shape change for a path whose only job is "let a
   human eyeball the layout". BMP encoding already has its own unit tests.
   **Recommend always-PNG**; say so if you'd rather keep BMP fidelity.
2. **Deleting `/preview/png` (Decision 3)** — strictly a simplification given
   Decision 2, but it does remove a public-ish endpoint. If you'd rather keep
   it as a stable alias, it can just delegate to the same handler.
3. **Virtual Device always in the dropdown (Decision 5)** — this changes
   manual-editor behavior for anyone who owns devices (they gain an option
   they didn't have). I think that's a fix, not a regression, but it is a
   visible change to a page this feature didn't otherwise need to touch.
4. **Mid-turn device switch (§4)** — recommend bumping `generation` so the
   existing supersede guard drops the stale render. The alternative (disable
   the dropdown while `busy`) is simpler but blocks a legitimate action for
   the length of a multi-tool-call turn. Either is defensible; say if you'd
   prefer the simpler one.
5. **Telling Claude about grayscale (§4b)** — this makes the AI page actually
   *use* the extra levels rather than just previewing them accurately. It
   does mean Claude produces different output for a 2-bit device than a 1-bit
   one for the same prompt, which is the point, but is worth being explicit
   about since it changes generated-template content, not just rendering.
6. **Not in scope:** dithering (previews will band exactly as the device
   will, which is arguably correct), and capability auto-detection — both are
   listed as follow-ups in the grayscale project's Limitations.
