# 2-bit Grayscale Support

Devices flagged `supports_2bit_grayscale` receive a true 2-bit (four-level)
grayscale PNG instead of the 1-bit BMP, leaving the BMP path untouched for
every other device. The flag is **set by an admin**, not reported by the
device — see [Limitations](#limitations). Originated from
[ideas/2-bit-grayscale-support](../ideas/) (idea file removed on completion);
the step-by-step plan lives in git history at
`docs/projects/plans/20260720-2-bit-grayscale-support-plan.md` (removed per
the usual end-of-project cleanup — the two mid-implementation corrections it
accumulated are summarized in the Retrospective).

## What shipped

- **Data model**: `devices` gets one new column,
  `supports_2bit_grayscale` (default `0`), via
  [migrations/20260823000000_add_supports_2bit_grayscale_to_devices.sql](../../../migrations/20260823000000_add_supports_2bit_grayscale_to_devices.sql).
  `update_device_supports_2bit_grayscale` in
  [src/db.rs](../../../src/db.rs) is a mechanical mirror of
  `update_device_maximum_compatibility`, landed with a characterization
  round-trip test per the [near-verbatim CRUD mirror
  allowance](../../development-process.md#rules).
- **Quantize + encode** ([src/device/grayscale.rs](../../../src/device/grayscale.rs)):
  `convert_to_2bit` maps luminance into four display levels
  (0/85/170/255); `encode_2bit_png` packs those into a **genuine
  2-bit-depth PNG** — 4 samples per byte, MSB-first, `ceil(width/4)` bytes
  per scanline (PNG rows are byte-aligned; the 4-byte row padding is a BMP
  quirk, not a PNG one). This required a **new direct dependency on the
  `png` crate** (0.18, already present transitively via `image`, so no new
  version entered the lockfile): `image`'s own PNG encoder hard-errors on
  `BitDepth::Two` and cannot write below 8 bits per channel.
- **Render pipeline** ([src/device/renderer.rs](../../../src/device/renderer.rs)):
  `svg_to_grayscale` mirrors the existing `svg_to_bilevel` but skips the
  black/white threshold, and `render_screen_2bit_png` composes it with the
  quantize+encode pair. The 1-bit `render_screen`/`render_screen_png`
  functions are unchanged.
- **Device-facing** ([src/device/api.rs](../../../src/device/api.rs)): a pure
  `render_route_for_device()` returns the route + extension pair, so the
  branch is unit-testable without DB or HTTP setup. `GET /api/display`
  points flagged devices at `GET /render/screen_2bit.png`; the response
  also gained a `bitdepth` field (`1` or `2`) for debuggability. The new
  render route reuses the existing `check_signed_request` gate — the HMAC
  is scoped to device+timestamp only, independent of path, so no signing
  change was needed.
- **Admin API + UI**: `POST /dashboard/devices/{id}/grayscale`
  ([src/api/devices.rs](../../../src/api/devices.rs)) and a
  `GrayscaleToggle` on the device detail page
  ([src/frontend/pages/devices.rs](../../../src/frontend/pages/devices.rs)),
  both mirroring the `FirmwareUpdatesToggle` pattern through all four
  layers (fetch helper, native-tier stub, `AppStore` method, component).
  Added in a follow-up pass — see the last retrospective item.
- **Grayscale-aware previews**: every dashboard preview renders in the mode
  its device is configured for, so what an admin sees matches what the device
  will display. One shared `render_preview_png` helper
  ([src/api/mod.rs](../../../src/api/mod.rs)) branches on
  `ctx.device.supports_2bit_grayscale`; the device detail page follows that
  device's flag, and both template-editing pages follow the selected
  **Preview Device**. Previews are now **always PNG** (see Decisions below),
  which also let `/dashboard/preview/png` be deleted — the AI generator had
  been rasterizing every render twice, once for the `<img>` and once for
  Claude's vision input. A shared `PreviewDeviceSelector`
  ([src/frontend/components/preview_device_selector.rs](../../../src/frontend/components/preview_device_selector.rs))
  serves both editors, labels each option with its mode
  (`kitchen (800×480, 2-bit)`), and always offers the virtual device.
  `build_system_prompt` also branches: on a 2-bit device Claude is told it has
  four levels rather than instructed to avoid grayscale.
- **Docs**: [architecture.md](../../architecture.md) (module map +
  `/api/display` response schema), [templates.md](../../templates.md)
  (grayscale palette note), [testing.md](../../testing.md#database-tests)
  (the shared-default-template trap, below), [state.md](../state.md).

## Decisions

- **Previews are always PNG, never BMP.** The response is a bare base64
  string, so once it could be either format the client had no way to know
  what it got short of re-deriving the server's branch. `render_screen_png`
  is pixel-identical to the BMP, so 1-bit previews are unchanged. Devices
  still receive a real BMP from `/render/screen.bmp` — only previews moved.
- **`/dashboard/preview/png` deleted rather than aliased.** It existed only
  because Claude's vision can't read BMP; once `/preview` always returns PNG
  the two were byte-identical. Folding them means one render serves both the
  on-screen preview and Claude's vision input, so the two cannot drift.
- **The virtual device is always in the selector.** Previously the manual
  editor offered it only if you owned no real devices, which meant anyone
  with hardware couldn't preview at a neutral 800×480.

## Verification

`cargo test --features server` — 147 unit tests + 12 browser E2E tests, all
passing. `cargo check --no-default-features --features web --target
wasm32-unknown-unknown` clean. Two tests pin the ends of the feature:

- `grayscale_device_end_to_end_poll_and_fetch_yields_a_real_2bit_png` polls
  `/api/display`, follows the returned signed URL, and asserts
  `bit_depth == Two` on the actual response bytes.
- `device_page_preview_renders_in_the_devices_configured_mode` drives real
  headless Chromium: it reads the preview's `src`, toggles grayscale on,
  reloads, and asserts the preview is still a PNG data URL *and* that its
  bytes changed — catching both a server that reverted to BMP and a preview
  that ignored the flag.

## Limitations

- **No capability detection.** Despite the plan's "Device Capability
  Detection" heading, nothing is detected: the device never reports
  grayscale support (no header is parsed for it), and the server never
  infers it from `model` or `fw_version`. An admin must know their firmware
  handles 4-level grayscale and flip the toggle. Flip it for a device that
  can't, and it silently receives a PNG it cannot decode — the same class
  of failure as the `It is not a BMP file` symptom that motivated the
  X-Forwarded-Proto fix. Auto-detection by model or firmware version is the
  obvious follow-up.
- **No dithering.** `convert_to_2bit` hard-buckets each pixel
  independently. Fine for the flat fills and text that templates mostly
  contain; photographic or gradient content will band visibly. Error
  diffusion (Floyd–Steinberg) would be a drop-in change to that one
  function.

## Retrospective

**What worked**

- Extracting `render_route_for_device` as a pure function, following the
  `decide_firmware_update` precedent from the OTA project: two
  branch-covering unit tests with zero setup, plus integration tests
  through the real handler for the wiring.
- Spiking the risky assumption in a throwaway cargo project *before*
  writing any real code (per the [spike-the-riskiest-assumption
  rule](../../development-process.md#rules)) — it settled both halves of
  the encode/decode question in one cheap round trip.
- Running the *full* suite rather than just the new tests. Two real
  problems surfaced only that way; neither would have appeared in a scoped
  run.

**What caused friction, surprise, or rework**

- **The plan contradicted itself, and neither half was wrong alone.** Its
  code sample encoded via `image::codecs::png::PngEncoder` — which cannot
  write below 8 bits per channel for a `Luma<u8>` buffer — so following it
  literally yields an 8-bit PNG that merely *uses* four gray values. But
  its test step asserted the fetched PNG's "bit-depth is reported as 2" via
  `image::io::Reader`, an assertion that can never pass against that
  encoder's output *and* that `image`'s decoder cannot even evaluate, since
  it transparently expands sub-8-bit depths (`Transformations::EXPAND`)
  before returning pixels. Resolved by asking which artifact was intended
  (true 2-bit depth), then spiking: a hand-packed 2bpp PNG via the `png`
  crate round-trips through `image::load_from_memory` to the expected
  `[0, 85, 170, 255]`, and reports `bit_depth == Two` when read via
  `png::Decoder` directly.
- **A new non-optional struct field broke a hand-authored JSON fixture.**
  Adding `supports_2bit_grayscale` to `Device` made
  `api::templates::preview_png_returns_a_decodable_png` fail with a 422 —
  its inline `RenderContext` JSON no longer matched the struct. Nothing
  about this is caught at compile time.
- **A flaky test exposed a latent shared-fixture trap.** The end-to-end
  test failed intermittently under the parallel suite with
  `UsvgError(ParsingFailed(NoRootNode))`. Root cause: `create_device()`
  points every device at `get_default_template()`, which is "the template
  with the lowest `id`" in the entire shared-cache in-memory test DB — one
  row every test in the binary shares, not something scoped per test. No
  prior device test had rendered real pixels through it (they asserted on
  JSON shape only), so nothing had surfaced it. Fixed by giving the test
  its own template via `update_device_template`, and documented in
  [testing.md](../../testing.md#database-tests) so the next such test
  doesn't rediscover it.
- **Shipped a setting with no way to set it.** The first pass added the
  toggle endpoint but no device-page control, reasoning that the plan
  didn't call for a dashboard control and scope should stay minimal. Wrong
  cut: a toggle nothing can reach isn't a smaller feature, it's an
  unfinished one, and the user had to notice the gap and ask. The fix cost
  an rsx! block, ~15 lines of plumbing, and one browser E2E test copied
  near-verbatim from the sibling toggle — far less than the round trip it
  caused.

- **Then shipped the setting with no way to *see* it.** The same mistake
  one layer up, and it took a written Limitations section to catch. Previews
  stayed 1-bit, so an admin could enable grayscale and get no feedback that
  anything had changed. Writing the limitation down is what turned it from
  an unknown gap into a scheduled fix — but it should have been caught by
  asking "what does the user see after they flip this?" during the first
  pass, which is the same question that would have caught the missing
  toggle.

- **A plan asserted behavior from assumption and was wrong.** The preview
  plan's AI-page section claimed the render device was "read at render time"
  and that switching devices mid-conversation would be picked up. Checking
  found the device is snapshotted once per `send_message` (`mod.rs:518`) and
  moved into the tool-call closure — so an in-flight turn keeps rendering for
  the previously selected device. Worse, the claim was hiding a race: the
  switch handler sets `preview_image`, then the in-flight render overwrites
  it, because a device switch didn't bump the `generation` counter the
  supersede guard reads. Both are fixed (the switch bumps `generation`), but
  the plan had been presented for approval with that section stated as fact.
  Caught only because the user asked "did you confirm this?"

- **The fix for that race then shipped an infinite loop.** Bumping
  `generation` inside the switch `use_effect` was written as
  `let my = generation() + 1; generation.set(my);` — a *tracked* read
  subscribing the effect to the signal it immediately writes. Every write
  retriggered the effect, which spawned another three-request fetch trio.
  The reported symptom was an unresponsive page, a request flood, and
  climbing memory; the regression test reproduces it as a hard browser-tab
  crash (171s to crash, versus 5s to pass once fixed). `peek()` is the fix.
  Two things stand out. First, every test tier the project already had was
  blind to it: the loop is *correct code executed unboundedly*, so nothing
  that inspects a single request or a single render can see it — catching it
  needs a request count over time
  (`performance.getEntriesByType('resource')` in the browser tier). Second,
  it was introduced by a fix for a race that a *reviewer question* had
  surfaced — tightening one concurrency bug opened a worse one, in code that
  the existing E2E test for the same page walked straight past because it
  never touched the dropdown.

**What to change (proposed, not yet applied)**

Three candidate rules for
[development-process.md](../../development-process.md#rules), pending
confirmation:

- *A plan is not self-consistent until its examples and its assertions
  describe the same artifact.* When a code sample and its test step
  disagree, that's a design decision surfacing late, not a detail to smooth
  over — stop and settle it before implementing either.
- *A new setting isn't done until it's reachable **and observable**.* When a
  plan is silent on how a flag gets set, treat "settable from the UI it lives
  next to" as default scope — and ask what the user sees after flipping it.
  This project shipped the same omission twice: no toggle, then no visible
  effect from the toggle.
- *Don't state behavior of existing code in a plan without reading it.* A
  plan exists to be checkable before implementation; assertions in one carry
  the weight of verified fact. Read the code, or mark the claim as an
  assumption to confirm.
- *Exercise the control you just added, in the browser, before calling it
  done.* The device-page preview got an E2E test; the dropdown that was the
  actual new interaction did not, and that is exactly where the loop lived.
  A new interactive control needs a test that operates it — rendering it
  is not exercising it. (Now documented as a hazard in
  [frontend.md](../../frontend.md#never-write-a-signal-a-use_effect-also-reads).)
- *After adding a struct field, grep for hand-authored JSON fixtures of
  that struct* (`grep -rln '"sibling_field_name"'`) — this repo has at
  least one, and no compiler check covers it.
