/**
 * Tic-Tac-Toe 3D Renderer
 *
 * Three.js scene with shared arena, holographic GLB characters seated on
 * either side of a board, neon X/O game pieces, and a winning-line glow.
 *
 * Translated from the React (R3F) reference implementation; uses the same
 * shared/* primitives as the existing red-button and coup viewers.
 */

import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import * as SkeletonUtils from 'three/addons/utils/SkeletonUtils.js';
import { buildArena } from './shared/shared-arena.js';
import { buildChair } from './shared/shared-chair.js';
import { createPostProcessing } from './shared/shared-postprocessing.js';
import { createHolographicMaterial, createWireframeMaterial } from './shared/shared-materials.js';
import { PLAYER_COLORS } from './shared/shared-colors.js';

// ─── Layout constants ────────────────────────────────────────────────────────

const BOARD_Y = 1.0;
const PIECE_Y = BOARD_Y + 0.08;
const PLAYER_DISTANCE = 3.5;
const GRID_COLOR = 0x1a99bb;
const PIECE_BAR_LENGTH = 0.65;
const PIECE_BAR_THICK = 0.08;
const PIECE_BAR_HEIGHT = 0.08;
const CELL_SPACING = 1.1;

const CHARACTER_MODELS = [
  'assets/characters/character_01.glb',
  'assets/characters/character_02.glb',
];

// ─── Geometry helpers ────────────────────────────────────────────────────────

function cellToPosition(row, col) {
  return [(col - 1) * CELL_SPACING, PIECE_Y, (row - 1) * CELL_SPACING];
}

const WIN_LINES = [
  [[0, 0], [0, 1], [0, 2]],
  [[1, 0], [1, 1], [1, 2]],
  [[2, 0], [2, 1], [2, 2]],
  [[0, 0], [1, 0], [2, 0]],
  [[0, 1], [1, 1], [2, 1]],
  [[0, 2], [1, 2], [2, 2]],
  [[0, 0], [1, 1], [2, 2]],
  [[0, 2], [1, 1], [2, 0]],
];

function findWinningLine(board) {
  for (const line of WIN_LINES) {
    const [a, b, c] = line;
    const va = board[a[0]]?.[a[1]];
    const vb = board[b[0]]?.[b[1]];
    const vc = board[c[0]]?.[c[1]];
    if (va && va !== 'empty' && va === vb && vb === vc) return line;
  }
  return null;
}

// ─── Renderer ────────────────────────────────────────────────────────────────

export class TicTacToeRenderer {
  constructor(canvas, { xColor, oColor } = {}) {
    this.xColor = xColor ?? PLAYER_COLORS[0];
    this.oColor = oColor ?? PLAYER_COLORS[1];
    this.canvas = canvas;
    this.clock = new THREE.Clock();

    // Three.js core
    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x0a1520);
    this.scene.fog = new THREE.FogExp2(0x0a1520, 0.008);

    this.camera = new THREE.PerspectiveCamera(
      55,
      window.innerWidth / window.innerHeight,
      0.1,
      200,
    );
    // Initial framing chosen to match the R3F AutoCamera (top-down-ish).
    this.camera.position.set(3, 9.1, 3);
    this.camera.lookAt(0, 1, 0);

    this.renderer = new THREE.WebGLRenderer({
      canvas,
      antialias: true,
      alpha: false,
    });
    this.renderer.setSize(window.innerWidth, window.innerHeight);
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.shadowMap.enabled = true;

    // Arena (shared primitive — curved walls + radial floor grid)
    const arena = buildArena(this.scene);
    this.arenaMaterials = arena.materials;

    // Post-processing (Bloom + Vignette via shared helper)
    const pp = createPostProcessing(this.renderer, this.scene, this.camera, {
      bloom: { strength: 1.5, threshold: 0.3, smoothing: 0.4 },
    });
    this.composer = pp.composer;
    this.bloomPass = pp.bloomPass;

    // Lighting
    this._setupLights();

    // Game board (holographic 3x3 grid)
    this._buildBoard();

    // Particle field
    this._buildParticles();

    // Characters: load asynchronously; the viewer awaits charactersLoaded
    // before rendering so the seats appear at first paint.
    this.characters = { x: null, o: null };
    this.characterMaterials = [];
    this.mixers = [];
    this.charactersLoaded = this._loadCharacters();

    // Pieces: keyed by `${row},${col}` so we can reconcile against board state
    this.pieces = new Map();
    // Win line mesh, replaced when the winning configuration changes
    this.winLineGroup = null;

    // Auto-rotating orbit camera
    this.cameraTheta = Math.atan2(this.camera.position.z, this.camera.position.x);
    this.cameraTarget = new THREE.Vector3(0, 0.8, 0);
    this.cameraRadius = Math.sqrt(
      this.camera.position.x ** 2 + this.camera.position.z ** 2,
    );
    this.cameraHeight = this.camera.position.y;

    // Resize handler
    this._onResize = this._handleResize.bind(this);
    window.addEventListener('resize', this._onResize);
  }

  _setupLights() {
    const ambient = new THREE.AmbientLight(0x334455, 0.6);
    this.scene.add(ambient);

    const directional = new THREE.DirectionalLight(0xaaccff, 0.8);
    directional.position.set(5, 12, 8);
    directional.castShadow = true;
    this.scene.add(directional);

    // Subtle fill light from below
    const fill = new THREE.PointLight(0x113344, 0.4, 30);
    fill.position.set(0, -1, 0);
    this.scene.add(fill);

    // Per-player accent lights
    const xAccent = new THREE.PointLight(this._dim(this.xColor), 12, 8);
    xAccent.position.set(-2, 3, -3);
    this.scene.add(xAccent);

    const oAccent = new THREE.PointLight(this._dim(this.oColor), 12, 8);
    oAccent.position.set(2, 3, 3);
    this.scene.add(oAccent);
  }

  _dim(hex) {
    const c = new THREE.Color(hex);
    c.multiplyScalar(0.4);
    return c;
  }

  _buildBoard() {
    const group = new THREE.Group();
    group.position.y = BOARD_Y;

    const lineThickness = 0.025;
    const gridSpan = 1.75;
    const gridColor = new THREE.Color(GRID_COLOR);

    // Semi-transparent base plane
    const base = new THREE.Mesh(
      new THREE.PlaneGeometry(3.6, 3.6),
      new THREE.MeshBasicMaterial({
        color: 0x061520,
        transparent: true,
        opacity: 0.12,
        side: THREE.DoubleSide,
      }),
    );
    base.rotation.x = -Math.PI / 2;
    group.add(base);

    const borderMatProps = {
      color: gridColor,
      toneMapped: false,
      transparent: true,
      opacity: 0.35,
    };
    const dividerMatProps = {
      color: gridColor,
      toneMapped: false,
      transparent: true,
      opacity: 0.65,
    };

    const borders = [
      { p: [0, 0, -gridSpan], s: [gridSpan * 2, 0.01, lineThickness] },
      { p: [0, 0, gridSpan], s: [gridSpan * 2, 0.01, lineThickness] },
      { p: [-gridSpan, 0, 0], s: [lineThickness, 0.01, gridSpan * 2] },
      { p: [gridSpan, 0, 0], s: [lineThickness, 0.01, gridSpan * 2] },
    ];
    this._boardPulseMats = [];
    for (const b of borders) {
      const mesh = new THREE.Mesh(
        new THREE.BoxGeometry(...b.s),
        new THREE.MeshBasicMaterial({ ...borderMatProps }),
      );
      mesh.position.set(...b.p);
      group.add(mesh);
      this._boardPulseMats.push({ mat: mesh.material, base: 0.35 });
    }

    const dividers = [-0.55, 0.55];
    for (const x of dividers) {
      const mesh = new THREE.Mesh(
        new THREE.BoxGeometry(lineThickness, 0.015, gridSpan * 2),
        new THREE.MeshBasicMaterial({ ...dividerMatProps }),
      );
      mesh.position.set(x, 0, 0);
      group.add(mesh);
      this._boardPulseMats.push({ mat: mesh.material, base: 0.65 });
    }
    for (const z of dividers) {
      const mesh = new THREE.Mesh(
        new THREE.BoxGeometry(gridSpan * 2, 0.015, lineThickness),
        new THREE.MeshBasicMaterial({ ...dividerMatProps }),
      );
      mesh.position.set(0, 0, z);
      group.add(mesh);
      this._boardPulseMats.push({ mat: mesh.material, base: 0.65 });
    }

    this.boardGroup = group;
    this.scene.add(group);
  }

  _buildParticles() {
    const count = 250;
    const geo = new THREE.BufferGeometry();
    const positions = new Float32Array(count * 3);
    const phases = new Float32Array(count);
    for (let i = 0; i < count; i += 1) {
      positions[i * 3] = (Math.random() - 0.5) * 20;
      positions[i * 3 + 1] = Math.random() * 10;
      positions[i * 3 + 2] = (Math.random() - 0.5) * 20;
      phases[i] = Math.random() * Math.PI * 2;
    }
    geo.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    geo.setAttribute('phase', new THREE.BufferAttribute(phases, 1));

    const mat = new THREE.PointsMaterial({
      size: 0.04,
      color: 0x66ddff,
      transparent: true,
      opacity: 0.5,
      sizeAttenuation: true,
      depthWrite: false,
    });
    this.particleSystem = new THREE.Points(geo, mat);
    this.scene.add(this.particleSystem);
  }

  async _loadCharacters() {
    const loader = new GLTFLoader();
    const configs = [
      {
        slot: 'x',
        path: CHARACTER_MODELS[0],
        color: this.xColor,
        position: new THREE.Vector3(0, 0, -PLAYER_DISTANCE),
        facingY: 0, // chair looks toward +Z (board center)
      },
      {
        slot: 'o',
        path: CHARACTER_MODELS[1],
        color: this.oColor,
        position: new THREE.Vector3(0, 0, PLAYER_DISTANCE),
        facingY: Math.PI, // chair looks toward -Z (board center)
      },
    ];

    const promises = configs.map(
      (cfg) =>
        new Promise((resolve) => {
          loader.load(
            cfg.path,
            (gltf) => {
              // Chair seat
              const seat = buildChair(cfg.color, { scale: 1.3 });
              seat.position.copy(cfg.position);
              seat.rotation.y = cfg.facingY;
              const accent = new THREE.PointLight(cfg.color, 1.0, 8);
              accent.position.y = 3.0;
              seat.add(accent);
              this.scene.add(seat);

              // Character — clone for skeleton rebinding, then sit on chair
              const model = SkeletonUtils.clone(gltf.scene);
              model.scale.set(4.0, 4.0, 4.0);
              model.position.y = 0.95;
              model.rotation.y = Math.PI;

              const holoMat = createHolographicMaterial(cfg.color, {
                skinning: true,
                roleBlend: false,
              });
              const wireMat = createWireframeMaterial(cfg.color, { skinning: true });
              this.characterMaterials.push(holoMat, wireMat);

              const wireClones = [];
              model.traverse((child) => {
                if (child.isSkinnedMesh) {
                  child.material = holoMat;
                  child.frustumCulled = false;
                  const clone = child.clone();
                  clone.material = wireMat;
                  clone.frustumCulled = false;
                  clone.bind(child.skeleton, child.bindMatrix);
                  wireClones.push({ clone, parent: child.parent });
                } else if (child.isMesh) {
                  // Hide static geometry baked into the GLB (chairs/etc).
                  child.visible = false;
                }
              });
              for (const { clone, parent } of wireClones) {
                (parent || model).add(clone);
              }

              let mixer = null;
              if (gltf.animations && gltf.animations.length > 0) {
                mixer = new THREE.AnimationMixer(model);
                const idleClip =
                  gltf.animations.find((c) => c.name === 'idle_seated') ||
                  gltf.animations[0];
                const action = mixer.clipAction(idleClip.clone());
                action.play();
                this.mixers.push(mixer);
              }

              seat.add(model);
              this.characters[cfg.slot] = {
                seat,
                model,
                holoMat,
                wireMat,
                mixer,
                color: cfg.color,
              };
              resolve();
            },
            undefined,
            (err) => {
              console.warn(`[TTTRenderer] failed to load ${cfg.path}`, err);
              resolve();
            },
          );
        }),
    );

    await Promise.all(promises);
  }

  // ─── Game state → scene reconciliation ───

  /**
   * Reconcile the rendered pieces against the engine board. Adds new pieces
   * with a pop-in animation; leaves existing pieces in place. Called by the
   * viewer after every state refresh.
   */
  syncBoard(board) {
    for (let row = 0; row < 3; row += 1) {
      for (let col = 0; col < 3; col += 1) {
        const cell = board[row][col];
        const key = `${row},${col}`;
        if (cell === 'empty') {
          if (this.pieces.has(key)) {
            const old = this.pieces.get(key);
            this.scene.remove(old.group);
            this.pieces.delete(key);
          }
          continue;
        }
        if (this.pieces.has(key)) continue;
        const pos = cellToPosition(row, col);
        const piece = cell === 'x'
          ? this._buildXPiece(pos, this.xColor)
          : this._buildOPiece(pos, this.oColor);
        this.scene.add(piece.group);
        this.pieces.set(key, piece);
      }
    }
  }

  _buildXPiece(position, hexColor) {
    const group = new THREE.Group();
    group.position.set(...position);
    group.scale.set(0, 0, 0);

    const color = new THREE.Color(hexColor);
    const barGeo = new THREE.BoxGeometry(PIECE_BAR_LENGTH, PIECE_BAR_HEIGHT, PIECE_BAR_THICK);
    const barMat = new THREE.MeshStandardMaterial({
      color,
      emissive: color,
      emissiveIntensity: 1.2,
      toneMapped: false,
    });
    const bar1 = new THREE.Mesh(barGeo, barMat);
    bar1.rotation.y = Math.PI / 4;
    group.add(bar1);
    const bar2 = new THREE.Mesh(barGeo.clone(), barMat.clone());
    bar2.rotation.y = -Math.PI / 4;
    group.add(bar2);

    const halo = new THREE.Mesh(
      new THREE.CircleGeometry(0.35, 16),
      new THREE.MeshBasicMaterial({
        color,
        transparent: true,
        opacity: 0.06,
        toneMapped: false,
        side: THREE.DoubleSide,
      }),
    );
    halo.position.y = -0.04;
    halo.rotation.x = -Math.PI / 2;
    group.add(halo);

    return { group, mark: 'x' };
  }

  _buildOPiece(position, hexColor) {
    const group = new THREE.Group();
    group.position.set(...position);
    group.scale.set(0, 0, 0);

    const color = new THREE.Color(hexColor);
    const torus = new THREE.Mesh(
      new THREE.TorusGeometry(0.28, 0.055, 16, 32),
      new THREE.MeshStandardMaterial({
        color,
        emissive: color,
        emissiveIntensity: 1.2,
        toneMapped: false,
      }),
    );
    torus.rotation.x = -Math.PI / 2;
    group.add(torus);

    const halo = new THREE.Mesh(
      new THREE.CircleGeometry(0.35, 16),
      new THREE.MeshBasicMaterial({
        color,
        transparent: true,
        opacity: 0.06,
        toneMapped: false,
        side: THREE.DoubleSide,
      }),
    );
    halo.position.y = -0.04;
    halo.rotation.x = -Math.PI / 2;
    group.add(halo);

    return { group, mark: 'o' };
  }

  /**
   * Add or replace the winning-line glow. Pass null/undefined to clear it.
   */
  setWinningLine(board) {
    if (this.winLineGroup) {
      this.scene.remove(this.winLineGroup);
      this.winLineGroup = null;
    }
    if (!board) return;
    const line = findWinningLine(board);
    if (!line) return;

    const start = cellToPosition(line[0][0], line[0][1]);
    const end = cellToPosition(line[2][0], line[2][1]);
    const midX = (start[0] + end[0]) / 2;
    const midZ = (start[2] + end[2]) / 2;
    const length =
      Math.sqrt((end[0] - start[0]) ** 2 + (end[2] - start[2]) ** 2) + 0.5;
    const angle = Math.atan2(end[2] - start[2], end[0] - start[0]);

    const group = new THREE.Group();
    group.position.set(midX, PIECE_Y + 0.02, midZ);
    group.rotation.y = -angle;

    // Outer glow
    const outer = new THREE.Mesh(
      new THREE.CapsuleGeometry(0.08, length - 0.16, 8, 16),
      new THREE.MeshBasicMaterial({
        color: 0xffe040,
        toneMapped: false,
        transparent: true,
        opacity: 0.6,
      }),
    );
    outer.rotation.z = Math.PI / 2;
    group.add(outer);

    // Bright core (pulsed in update())
    const core = new THREE.Mesh(
      new THREE.CapsuleGeometry(0.025, length - 0.05, 8, 16),
      new THREE.MeshBasicMaterial({
        color: 0xffee66,
        toneMapped: false,
        transparent: true,
      }),
    );
    core.rotation.z = Math.PI / 2;
    group.add(core);
    this._winLineCoreMat = core.material;

    this.scene.add(group);
    this.winLineGroup = group;
  }

  // ─── Per-frame update ───

  update() {
    const t = this.clock.getElapsedTime();
    const dt = this.clock.getDelta();

    // Arena shader uniforms (scrolling grid, etc.)
    if (this.arenaMaterials) {
      for (const m of this.arenaMaterials) {
        if (m.uniforms?.time) m.uniforms.time.value = t;
      }
    }

    // Holographic + wireframe shader uniforms on characters
    for (const m of this.characterMaterials) {
      if (m.uniforms?.time) m.uniforms.time.value = t;
    }

    // Skeleton animation
    for (const mixer of this.mixers) mixer.update(dt);

    // Board pulse — opacity sin wave matching the React HolographicGrid frame
    if (this._boardPulseMats) {
      for (const entry of this._boardPulseMats) {
        // Each pulse band is centered on its base opacity; ±0.15 amplitude.
        entry.mat.opacity = entry.base + Math.sin(t * 1.5) * 0.15;
      }
    }

    // Piece pop-in (scale 0 → 1 over a few frames)
    for (const piece of this.pieces.values()) {
      const s = piece.group.scale.x;
      if (s < 1) {
        const next = Math.min(s + 0.08, 1);
        piece.group.scale.set(next, next, next);
      }
    }

    // Win-line core pulse
    if (this._winLineCoreMat) {
      this._winLineCoreMat.opacity = 0.85 + Math.sin(t * 3) * 0.15;
    }

    // Particle drift
    if (this.particleSystem) {
      const positions = this.particleSystem.geometry.attributes.position.array;
      const phases = this.particleSystem.geometry.attributes.phase.array;
      for (let i = 0; i < phases.length; i += 1) {
        positions[i * 3 + 1] += Math.sin(t * 0.5 + phases[i]) * 0.0015;
        if (positions[i * 3 + 1] > 10) positions[i * 3 + 1] = 0;
      }
      this.particleSystem.geometry.attributes.position.needsUpdate = true;
    }

    // Auto-rotating orbit camera (matches React OrbitControls autoRotateSpeed=0.35)
    this.cameraTheta += dt * 0.35 * 0.1;
    this.camera.position.x = Math.cos(this.cameraTheta) * this.cameraRadius;
    this.camera.position.z = Math.sin(this.cameraTheta) * this.cameraRadius;
    this.camera.position.y = this.cameraHeight;
    this.camera.lookAt(this.cameraTarget);

    this.composer.render();
  }

  _handleResize() {
    const w = window.innerWidth;
    const h = window.innerHeight;
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(w, h);
    this.composer.setSize(w, h);
  }

  dispose() {
    window.removeEventListener('resize', this._onResize);
  }
}
