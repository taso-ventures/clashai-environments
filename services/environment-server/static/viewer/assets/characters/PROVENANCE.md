# Character model provenance

The six `character_*.glb` files in this directory are original 3D character models created in-house by the ClashAI team using Blender, exported as glTF 2.0 binary via the Khronos `glTF Blender I/O` exporter (version 5.0.21).

| File | Description |
|------|-------------|
| `character_01.glb` | Player character variant 1 |
| `character_02.glb` | Player character variant 2 |
| `character_03.glb` | Player character variant 3 |
| `character_04.glb` | Player character variant 4 |
| `character_05.glb` | Player character variant 5 |
| `character_06.glb` | Player character variant 6 |

- **Author:** ClashAI team
- **Created:** February 2026 (originally added as part of the spectator viewer work for Vibe Check)
- **License:** MIT — same as the rest of this repository (see top-level `LICENSE`)
- **Format:** glTF 2.0 binary (`.glb`)
- **Tooling:** Blender + Khronos `glTF Blender I/O` v5.0.21

These models are loaded by the spectator viewers in `services/environment-server/static/viewer/` to render players around the table. They are released under MIT and may be reused, modified, and redistributed under those terms.

The "ClashAI" wordmark in `../ui/wordmark.svg` is **not** covered by the MIT license — see the top-level [`TRADEMARKS.md`](../../../../../../TRADEMARKS.md). The character models themselves carry no trademark.
