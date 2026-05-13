# Porting Spectator Viewers from React Three Fiber

Several environments (currently `tic_tac_toe`, plus `connect_four` / `poker` / `wordle` to come) have polished spectator viewers in the internal `agent-clash-client` Next.js app, written as React Three Fiber components. This repo's spectator viewers are standalone HTML + vanilla Three.js — no build step, no React, no Tailwind. Porting between the two is **not** copy-paste because of a handful of concrete API and runtime differences.

This guide captures what we learned porting `tic_tac_toe` (commits `53d4534`, `371ab01`, `a9a65ad`, `f581dcc`, `42f66c5`, `50260e2`) so future ports skip the experimentation.

## File layout per viewer

For each environment `<game>` you're porting, create five files under `services/environment-server/static/viewer/`:

```
<game>.html         # page shell — importmap, theme CSS, canvas, header overlay
<game>-styles.css   # game-specific overlay styles (header, game-over, etc.)
<game>-state.js     # state manager — REST bootstrap + WS subscribe
<game>-render.js    # the 3D scene
<game>-viewer.js    # orchestrator — wires renderer + state + DOM
```

Mirror the structure of an existing viewer (`tic-tac-toe.*` is the leanest reference; `coup-*` is the most elaborate). All five must follow the same naming convention so the route mapping in `services/environment-server/src/routes/mod.rs` (`match env_type.as_str()`) finds the right HTML file.

## Five concrete translation rules

These are the differences between `@react-three/fiber` + `@react-three/drei` + `@react-three/postprocessing` (what the internal repo uses) and vanilla three.js + `EffectComposer` + `UnrealBloomPass` (what this repo uses). They look small but each one will silently break the visual if missed.

### 1. Bloom strength: divide by ~3

The internal repo uses `<Bloom intensity={1.5} ... />` from `@react-three/postprocessing`. That library wraps a different bloom implementation than three.js's `UnrealBloomPass`. Their `intensity` is **not** the same parameter as `UnrealBloomPass`'s `strength`.

**Rule:** the OSS-shared default `bloom: { strength: 0.5, radius: 0.5, threshold: 0.35 }` matches the internal `intensity={1.5}` look. Don't override unless you have a specific reason.

```js
const pp = createPostProcessing(this.renderer, this.scene, this.camera, {
  bloom: { strength: 0.5, radius: 0.5, threshold: 0.35 },
});
```

If you copy the React `intensity={1.5}` value literally into `strength`, characters blow out into bright featureless blobs (no anatomy detail visible).

### 2. Point-light intensity: divide by ~8

The internal repo combines HDR rendering with `ACESFilmicToneMapping` and `toneMappingExposure: 1.2`, which compresses absurdly high light intensities into a sensible visible range. So you'll see code like:

```jsx
<pointLight intensity={12} color={...} distance={8} />
```

That `intensity={12}` is fine in HDR + ACES. Translated literally to vanilla Three.js with the same tone mapping, it still over-drives `UnrealBloomPass`.

**Rule:** scale per-player accent point lights down to **~1.5** in the OSS port (other working OSS viewers use 1.0–1.5). Use ACES tone mapping if the React version does — keep the visual style — but bring the intensities into the range the OSS bloom expects:

```js
this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
this.renderer.toneMappingExposure = 1.2;

const accent = new THREE.PointLight(playerColor, 1.5, 8);
accent.position.set(-2, 3, -3);
this.scene.add(accent);
```

### 3. Arena dimensions: pass explicit options

Some React viewers inline their own copy of `Arena()` rather than importing from a shared module. Those inlined Arenas often use different cylinder/floor dimensions than the OSS `shared/buildArena` defaults.

**Rule:** before calling `buildArena(scene)`, find the React Arena's geometry and pass matching options:

```js
// React TicTacToeGameBoard.tsx:281 — CylinderGeometry(12, 14, 10), wall.position.y=3
// React TicTacToeGameBoard.tsx:339 — CircleGeometry(14, 64)
const arena = buildArena(this.scene, {
  wallRadiusTop: 12,
  wallRadiusBottom: 14,
  wallHeight: 10,
  wallY: 3,
  floorRadius: 14,
});
```

OSS shared defaults are radius 18/20, wallHeight 14, floorRadius 20 — too big for compact games where the board should dominate the frame. Always check the React inline values first.

### 4. Clock read order: `getDelta()` BEFORE `getElapsedTime()`

`THREE.Clock.getElapsedTime()` internally calls `getDelta()` and advances `oldTime`. So this:

```js
const t  = this.clock.getElapsedTime();   // advances oldTime
const dt = this.clock.getDelta();         // returns ~0 (microseconds since previous line)
```

…leaves `dt ≈ 0`. Anything scaled by `dt` (auto-rotate camera, particle drift, mixer animations, scale-in interpolations) is effectively frozen — the visual looks "almost right but not animating."

**Rule:** always call `getDelta()` first:

```js
const dt = this.clock.getDelta();        // captures real frame delta
const t  = this.clock.getElapsedTime();  // accumulated; safe to read now
```

`coup-render.js:912-913` is the canonical OSS example.

### 5. Camera + auto-rotate: replicate by hand

The internal repo uses `<OrbitControls autoRotate autoRotateSpeed={0.35} target={[0, 0.8, 0]} />`. Vanilla doesn't have OrbitControls' update loop — you replicate manually.

**Rule:** capture the initial sphere coordinates relative to the target on construct, then advance the azimuth in `update()`. Use `~0.037 rad/s` to match `autoRotateSpeed=0.35`:

```js
// constructor
this.cameraTarget = new THREE.Vector3(0, 0.8, 0);
const dx = this.camera.position.x - this.cameraTarget.x;
const dz = this.camera.position.z - this.cameraTarget.z;
this.cameraTheta  = Math.atan2(dz, dx);
this.cameraRadius = Math.sqrt(dx * dx + dz * dz);
this.cameraHeight = this.camera.position.y;

// update()
this.cameraTheta += dt * 0.037;
this.camera.position.x = this.cameraTarget.x + Math.cos(this.cameraTheta) * this.cameraRadius;
this.camera.position.z = this.cameraTarget.z + Math.sin(this.cameraTheta) * this.cameraRadius;
this.camera.position.y = this.cameraHeight;
this.camera.lookAt(this.cameraTarget);
```

**Camera position note:** if the React file has an `AutoCamera` component setting `camera.position.set(...)` in a `useEffect`, that value IS what's actually rendered (despite OrbitControls being mounted) — start with those exact coordinates. Don't second-guess based on what the screenshot looks like; once bloom and scaling are right, the spec'd camera position will produce the spec'd framing.

## State management: diff-driven, not event-driven

The four games being ported (`tic_tac_toe`, `connect_four`, `poker`, `wordle`) all return `events: []` from their environment adapter's `apply_action` — the action stream's "what changed" payload is intentionally empty. Their viewers can't read game-specific deltas off the WebSocket the way `coup`/`red_button`/`vibe_check` viewers do.

**Pattern:** REST-bootstrap from `GET /matches/:id/state`, subscribe to the WS, and on every `event_type === "action"` or `"terminal"` frame, **re-fetch `/state`** and diff against the previous snapshot to derive new moves/changes. See `ttt-state.js::_refetchAndDispatch()` for the canonical pattern.

```js
this.ws.onmessage = async (ev) => {
  const frame = JSON.parse(ev.data);
  if (frame.catchup_start) { this.isCatchingUp = true; return; }
  if (frame.catchup_end)   { this.isCatchingUp = false; await this._refetchAndDispatch(); return; }
  if (frame.event_type === 'action' || frame.event_type === 'terminal') {
    await this._refetchAndDispatch();
  }
};
```

**Catchup markers:** `{catchup_start: true}` / `{catchup_end: true}` are bare control frames per [PROTOCOL.md](../PROTOCOL.md), **not** `UnifiedEvent` envelopes. Check for the marker keys before parsing as `UnifiedEvent` or you'll throw.

## Server route addition

After the viewer files are in place, extend the env-type → HTML mapping in `services/environment-server/src/routes/mod.rs`:

```rust
let viewer_page = match env_type.as_str() {
    "coup" => Some("coup.html"),
    "vibe_check" => Some("vibe-check.html"),
    "red_button" => Some("red-button.html"),
    "tic_tac_toe" => Some("tic-tac-toe.html"),
    // add new env here:
    "connect_four" => Some("connect-four.html"),
    _ => None,
};
```

The `spectator_url` will then be populated for new matches of that env automatically.

## Workflow checklist

For each new viewer:

1. **Read the React component fully** — note every `intensity={...}`, `<Bloom .../>`, arena cylinder/floor dimensions, light positions, character scale, and any `<AutoCamera/>` position. These are the parameters you'll be translating.

2. **Identify what's React-side-inlined vs. shared.** If the React file has its own `Arena()`, `SciFiChair()`, or `createHoloMaterial()`, capture their parameters (especially geometry sizes and light positions); you'll pass these as options to the OSS `shared/*` calls.

3. **Write the five files,** mirroring `ttt-*.{html,css,state.js,render.js,viewer.js}` structure.

4. **Add the route mapping.**

5. **Build and smoke-test:**

   ```bash
   cargo build --release --bin environment-server
   ./target/release/environment-server &
   curl -s -X POST http://localhost:8080/matches \
     -H 'content-type: application/json' \
     -d '{"environment_type":"<game>","player_count":<N>,"seed":42}'
   ```

   Confirm `spectator_url` returns the right HTML path, then load it in a browser.

6. **Compare side-by-side** with the internal viewer (the React app at `localhost:3001`). Five things to check, in order:
   - Camera framing — board centered, characters visible at expected positions
   - Brightness — characters readable as humanoid silhouettes, not bright blobs
   - Auto-rotation — slow orbit visible
   - Game pieces — pop in correctly when actions are submitted
   - Game-over overlay — fires with the right winner state

7. **Iterate per-issue,** committing each fix separately so the diff-against-React reasoning stays in the git history. The TTT commits above are the template — each fix is one focused commit with a multi-paragraph body explaining the parameter mismatch.

## What you don't need to translate

- **GLB character loading.** OSS already imports the same `character_01.glb` / `character_02.glb` / etc. assets. Use the same loader pattern as `rb-render.js::_loadCharacters()`.
- **Holographic material.** `shared/createHolographicMaterial(color, { skinning: true })` matches the React-inlined `createHoloMaterial(color, true)` byte-for-byte.
- **Wireframe overlay.** Same — `shared/createWireframeMaterial`.
- **Vignette + chromatic aberration.** Already in `shared/createPostProcessing`.

## Things to avoid

- **Don't carry over Tailwind classes** in the HTML — this repo doesn't ship Tailwind. Convert to plain CSS in `<game>-styles.css`.
- **Don't carry over agent brand colors** (Steel/Sage/etc.). Use `shared/PLAYER_COLORS` from `shared/shared-colors.js` — generic palette, no internal naming.
- **Don't carry over external API references** (`/api/observer/...`). The OSS viewer hits the documented protocol endpoints (`/matches/:id/state`, `/matches/:id/spectator/ws`).
- **Don't carry over branding text** ("ClashAI Spectator" titles, alt tags). Strip the same way the original three viewers were sanitized — see commits porting `coup.html` / `red-button.html` / `vibe-check.html` from internal.
