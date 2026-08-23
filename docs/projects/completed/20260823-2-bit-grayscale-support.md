# 2-bit Grayscale Support

Devices that report `supports_2bit_grayscale = true` now get a true 2-bit
(four-level) grayscale PNG instead of the 1-bit BMP, without touching the
existing BMP path for any other device.

## What shipped

- `devices.supports_2bit_grayscale` column (default `false`), toggled via
  `POST /dashboard/devices/{id}/grayscale` — mirrors the existing
  `maximum_compatibility`/`firmware_updates_enabled` toggles.
- `GET /api/display` branches on the flag: grayscale-enabled devices get an
  `image_url` pointing at `GET /render/screen_2bit.png` (same HMAC
  signed-URL gate as `/render/screen.bmp`); the response also gained a
  `bitdepth` field (`1` or `2`) for debuggability.
- `src/device/grayscale.rs`: `convert_to_2bit` quantizes full 0-255
  grayscale down to 4 display levels (0/85/170/255); `encode_2bit_png` packs
  those into a **genuine 2-bit-depth PNG** via the `png` crate directly.
- `renderer::render_screen_2bit_png` mirrors `render_screen`/
  `render_screen_png` but keeps full grayscale through rasterization instead
  of thresholding to black/white before quantizing.
- Docs: `docs/architecture.md` (module map + `/api/display` response schema),
  `docs/templates.md` (grayscale palette note).
- Device detail page: a "2-bit Grayscale" toggle mirroring the existing
  Maximum Compatibility / Firmware Updates toggles end to end (fetch helper,
  server-fn stub, `AppStore` method, component), added in a follow-up pass
  once it became clear the API-only endpoint wasn't reachable from the UI.
  Covered by a browser E2E test
  (`enabling_2bit_grayscale_on_a_device_persists`) mirroring
  `enabling_firmware_updates_on_a_device_persists`.

The step-by-step plan (including the two corrections described below) lives
in git history at `docs/projects/plans/20260720-2-bit-grayscale-support-plan.md`,
removed from the working tree per the usual end-of-project cleanup.

## Retrospective

**What worked:** The plan had already been corrected once (in an earlier
session) against the actual codebase layout — file paths, no `RequestContext`,
signing already path-agnostic — which meant implementation could start
straight from Phase 2 without re-discovering those facts.

**What caused friction:** The plan's own code sample for `convert_to_2bit`
was inconsistent with its own test step. The sample encoded via
`image::codecs::png::PngEncoder`, which cannot write below 8-bit-per-channel
for a `Luma<u8>` buffer (it hard-errors on `BitDepth::Two`) — so following
the sample literally would have produced an 8-bit PNG that merely *uses* four
gray values. But the plan's integration-test step asserted the fetched PNG's
"bit-depth is reported as 2" via `image::io::Reader` — an assertion that can
never pass against that encoder's output, and one `image`'s decoder can't
even evaluate, since it transparently expands sub-8-bit depths
(`Transformations::EXPAND`) before handing back pixel data. Neither half of
the plan was internally wrong on its own; they just didn't add up to the same
artifact. This was resolved by asking the user which was intended (true
2-bit depth), then spiking both the encode and decode sides with a throwaway
cargo project before writing any real code: confirmed a hand-packed 2bpp PNG
via the `png` crate directly (a) round-trips through
`image::load_from_memory` to the expected `[0, 85, 170, 255]`, and (b)
reports `bit_depth == Two` when read via the lower-level `png::Decoder`
instead of `image`'s.

A second, smaller friction point: adding a non-optional field to `Device`
broke an unrelated test (`api::templates::preview_png_returns_a_decodable_png`)
that hand-crafts a `RenderContext` JSON fixture — serde deserialization
failed with 422 once the fixture no longer matched the struct shape. Caught
immediately by running the *full* `cargo test --features server` suite
rather than just the newly-added tests; running only the new tests would
have shown all-green and shipped a real regression.

**What to change:** When a plan's code sample and its own test-assertion
step describe the artifact differently, treat that mismatch as a decision
point requiring a spike, not a detail to smooth over silently. Also: after
adding/renaming a struct field, grep for hand-authored JSON fixtures of that
struct (`grep -rl '"some_other_field"'`) before trusting a scoped test run —
this repo has at least one (`templates.rs`), and there's no compiler check
that would have caught it short of running that specific test.

A third, more interesting one: the end-to-end integration test
(`grayscale_device_end_to_end_poll_and_fetch_yields_a_real_2bit_png`) flaked
intermittently under the full parallel `cargo test` run — `UsvgError(ParsingFailed(NoRootNode))`,
i.e. the SVG being rendered was empty or malformed. Root cause:
`create_device()` points every new device at whatever `get_default_template()`
returns, which is "the template with the lowest `id`" in the entire
shared-cache in-memory test DB (see [testing.md](../../testing.md#database-tests))
— one row every test in the binary shares, not something scoped per test.
Nothing had rendered through that row for real before (existing device tests
only ever asserted on JSON shape, never decoded actual pixel output), so nothing
had surfaced this. The fix was to stop depending on it: the test now creates
its own dedicated template and assigns it to the device via
`update_device_template` before rendering, the same way `create_device`
fixtures already scope their access-token/mac/friendly-id with a unique
suffix. **Any new test that renders real pixel output from a `create_device()`
fixture must do the same** — assign a dedicated template rather than trust
the shared default row to still hold valid content by the time the test runs.

**What to change (process):** The first pass shipped the backend toggle
endpoint but not the device-page UI for it, reasoning that the plan didn't
call for a dashboard control and scope should stay minimal. That was the
wrong cut — a toggle nothing can reach is not a smaller feature, it's an
unfinished one, and it cost a full extra round trip (the user had to notice
the gap and ask) that a first-pass "is this reachable end-to-end?" check
would have caught for free. When a plan is silent on how a new flag gets
set, treat "reachable from the UI it lives next to" as the default scope,
not an opt-in extra — the mechanical-mirror cost of adding the toggle here
(rsx! block, five thin plumbing lines, one browser E2E test copied
near-verbatim from the sibling toggle) was tiny next to the cost of leaving
it out.
