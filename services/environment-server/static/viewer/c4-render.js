/**
 * Connect Four 3D Renderer
 *
 * Vertical 6x7 holographic board frame with neon discs that pop into the
 * column they're played in. Two GLB characters seated on either side of
 * the board. Translated from the React Three Fiber reference following
 * docs/PORTING_VIEWERS_FROM_R3F.md — bloom toned down 3x, accent lights
 * scaled from intensity 12 -> 1.5, clock ordering correct, manual orbit
 * around target=(0, 1, 0) at autoRotateSpeed=0.25.
 */

import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import * as SkeletonUtils from 'three/addons/utils/SkeletonUtils.js';
import { buildArena } from './shared/shared-arena.js';
import { buildChair } from './shared/shared-chair.js';
import { createPostProcessing } from './shared/shared-postprocessing.js';
import { createHolographicMaterial, createWireframeMaterial } from './shared/shared-materials.js';
import { PLAYER_COLORS } from './shared/shared-colors.js';

import { C4_ROWS, C4_COLS } from './c4-state.js';

// ─── Layout constants (mirror React) ─────────────────────────────────────────

const CELL_SPACING = 0.52;
const BOARD_WIDTH = (C4_COLS - 1) * CELL_SPACING;
const BOARD_HEIGHT = (C4_ROWS - 1) * CELL_SPACING;
const BOARD_CENTER_Y = 1.6;
const PLAYER_DISTANCE = 3.8;
const CHARACTER_SCALE = 2.5;
const SEAT_TOP_Y = 0.73;

const GRID_COLOR = 0x1155cc;
const GRID_COLOR_BRIGHT = 0x2266dd;

const CHARACTER_MODELS = [
  'assets/characters/character_01.glb',
  'assets/characters/character_02.glb',
];

function cellToPosition(row, col) {
  const x = (col - (C4_COLS - 1) / 2) * CELL_SPACING;
  const y = BOARD_CENTER_Y + ((C4_ROWS - 1) / 2 - row) * CELL_SPACING;
  return [x, y, 0.02];
}

const DIRECTIONS = [
  [0, 1],
  [1, 0],
  [1, 1],
  [1, -1],
];

function findWinningCells(board) {
  for (let row = 0; row < C4_ROWS; row += 1) {
    for (let col = 0; col < C4_COLS; col += 1) {
      const cell = board[row]?.[col];
      if (!cell || cell === 'empty') continue;
      for (const [dr, dc] of DIRECTIONS) {
        const cells = [];
        let valid = true;
        for (let k = 0; k < 4; k += 1) {
          const r = row + k * dr;
          const c = col + k * dc;
          if (
            r < 0 || r >= C4_ROWS ||
            c < 0 || c >= C4_COLS ||
            board[r]?.[c] !== cell
          ) {
            valid = false;
            break;
          }
          cells.push([r, c]);
        }
        if (valid && cells.length === 4) {
          return cells.map(([r, c]) => cellToPosition(r, c));
        }
      }
    }
  }
  return null;
}

// ─── Renderer ────────────────────────────────────────────────────────────────

export class ConnectFourRenderer {
  constructor(canvas, { blueColor, orangeColor } = {}) {
    this.blueColor = blueColor ?? PLAYER_COLORS[0];
    this.orangeColor = orangeColor ?? PLAYER_COLORS[1];
    this.canvas = canvas;
    this.clock = new THREE.Clock();

    // Three.js core — match React Canvas: BG 0x0a1520, fov 40, ACES
    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x0a1520);
    this.scene.fog = new THREE.FogExp2(0x0a1520, 0.012);

    this.camera = new THREE.PerspectiveCamera(
      40,
      window.innerWidth / window.innerHeight,
      0.1,
      50,
    );
    // React AutoCamera: position (0, 3.0, 11.0) lookAt (0, 1.0, 0).
    this.camera.position.set(0, 3.0, 11.0);
    this.camera.lookAt(0, 1.0, 0);

    this.renderer = new THREE.WebGLRenderer({
      canvas,
      antialias: true,
      alpha: false,
    });
    this.renderer.setSize(window.innerWidth, window.innerHeight);
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.shadowMap.enabled = true;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.2;

    // Arena — same dimensions as TTT (CylinderGeometry(12, 14, 10), CircleGeometry(14)).
    const arena = buildArena(this.scene, {
      wallRadiusTop: 12,
      wallRadiusBottom: 14,
      wallHeight: 10,
      wallY: 3,
      floorRadius: 14,
    });
    this.arenaMaterials = arena.materials;

    // Post-processing — OSS-default bloom (React Bloom intensity={1.5} via
    // postprocessing.js scales to ~strength=0.5 under UnrealBloomPass).
    const pp = createPostProcessing(this.renderer, this.scene, this.camera, {
      bloom: { strength: 0.5, radius: 0.5, threshold: 0.35 },
    });
    this.composer = pp.composer;
    this.bloomPass = pp.bloomPass;

    this._setupLights();
    this._buildBoardFrame();
    this._buildParticles();

    // Characters
    this.characters = { blue: null, orange: null };
    this.characterMaterials = [];
    this.mixers = [];
    this.charactersLoaded = this._loadCharacters();

    // Discs: keyed by `${row},${col}` so we can reconcile against board state
    this.discs = new Map();

    // Win highlight rings
    this.winHighlightGroup = null;
    this._winHighlightMats = [];

    // Auto-rotating orbit camera (target [0, 1.0, 0], autoRotateSpeed 0.25).
    this.cameraTarget = new THREE.Vector3(0, 1.0, 0);
    const dx = this.camera.position.x - this.cameraTarget.x;
    const dz = this.camera.position.z - this.cameraTarget.z;
    this.cameraTheta = Math.atan2(dz, dx);
    this.cameraRadius = Math.sqrt(dx * dx + dz * dz);
    this.cameraHeight = this.camera.position.y;
    // 0.25 is React OrbitControls autoRotateSpeed; ~0.026 rad/s.
    this.cameraOrbitSpeed = (0.25 / 0.35) * 0.037;

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

    const fill = new THREE.PointLight(0x113344, 0.4, 30);
    fill.position.set(0, -1, 0);
    this.scene.add(fill);

    // Per-player accent lights — React used intensity=12 with HDR + ACES;
    // scaled to 1.5 for OSS bloom. Positions mirror React (left/right of board).
    const blueAccent = new THREE.PointLight(this._dim(this.blueColor), 1.5, 8);
    blueAccent.position.set(-4, 2.5, 2);
    this.scene.add(blueAccent);

    const orangeAccent = new THREE.PointLight(this._dim(this.orangeColor), 1.5, 8);
    orangeAccent.position.set(4, 2.5, 2);
    this.scene.add(orangeAccent);
  }

  _dim(hex) {
    const c = new THREE.Color(hex);
    c.multiplyScalar(0.4);
    return c;
  }

  _buildBoardFrame() {
    const group = new THREE.Group();
    group.position.y = BOARD_CENTER_Y;

    const lineThick = 0.025;
    const padX = CELL_SPACING * 0.6;
    const padY = CELL_SPACING * 0.6;
    const halfW = BOARD_WIDTH / 2 + padX;
    const halfH = BOARD_HEIGHT / 2 + padY;

    this._boardPulseMats = [];

    // Back panel
    const back = new THREE.Mesh(
      new THREE.PlaneGeometry(halfW * 2 + 0.1, halfH * 2 + 0.1),
      new THREE.MeshBasicMaterial({
        color: 0x040e1a,
        transparent: true,
        opacity: 0.3,
        side: THREE.DoubleSide,
        depthWrite: false,
      }),
    );
    back.position.z = -0.04;
    group.add(back);

    // Border frame (top, bottom, left, right). All pulse at 0.45 + sin*0.1.
    const border = (s, p) => {
      const m = new THREE.Mesh(
        new THREE.BoxGeometry(...s),
        new THREE.MeshBasicMaterial({
          color: GRID_COLOR_BRIGHT,
          toneMapped: false,
          transparent: true,
          opacity: 0.5,
          depthWrite: false,
        }),
      );
      m.position.set(...p);
      group.add(m);
      this._boardPulseMats.push(m.material);
    };
    border([halfW * 2, lineThick, lineThick], [0, halfH, 0]);
    border([halfW * 2, lineThick, lineThick], [0, -halfH, 0]);
    border([lineThick, halfH * 2, lineThick], [-halfW, 0, 0]);
    border([lineThick, halfH * 2, lineThick], [halfW, 0, 0]);

    // Column separators (vertical lines between columns)
    for (let i = 0; i < C4_COLS - 1; i += 1) {
      const x =
        (i + 1 - (C4_COLS - 1) / 2) * CELL_SPACING - CELL_SPACING / 2;
      const m = new THREE.Mesh(
        new THREE.BoxGeometry(lineThick * 0.6, halfH * 2, lineThick * 0.6),
        new THREE.MeshBasicMaterial({
          color: GRID_COLOR,
          toneMapped: false,
          transparent: true,
          opacity: 0.25,
          depthWrite: false,
        }),
      );
      m.position.set(x, 0, 0);
      group.add(m);
      this._boardPulseMats.push(m.material);
    }

    // Row separators (horizontal lines between rows)
    for (let i = 0; i < C4_ROWS - 1; i += 1) {
      const y =
        (i + 1 - (C4_ROWS - 1) / 2) * CELL_SPACING - CELL_SPACING / 2;
      const m = new THREE.Mesh(
        new THREE.BoxGeometry(halfW * 2, lineThick * 0.6, lineThick * 0.6),
        new THREE.MeshBasicMaterial({
          color: GRID_COLOR,
          toneMapped: false,
          transparent: true,
          opacity: 0.25,
          depthWrite: false,
        }),
      );
      m.position.set(0, y, 0);
      group.add(m);
      this._boardPulseMats.push(m.material);
    }

    // Empty cell slot rings
    for (let row = 0; row < C4_ROWS; row += 1) {
      for (let col = 0; col < C4_COLS; col += 1) {
        const x = (col - (C4_COLS - 1) / 2) * CELL_SPACING;
        const y = ((C4_ROWS - 1) / 2 - row) * CELL_SPACING;
        const m = new THREE.Mesh(
          new THREE.TorusGeometry(0.17, 0.008, 8, 24),
          new THREE.MeshBasicMaterial({
            color: GRID_COLOR,
            toneMapped: false,
            transparent: true,
            opacity: 0.2,
            depthWrite: false,
            side: THREE.DoubleSide,
          }),
        );
        m.position.set(x, y, 0.01);
        group.add(m);
      }
    }

    // Column-number indicator dots above the board
    for (let col = 0; col < C4_COLS; col += 1) {
      const x = (col - (C4_COLS - 1) / 2) * CELL_SPACING;
      const m = new THREE.Mesh(
        new THREE.SphereGeometry(0.025, 6, 4),
        new THREE.MeshBasicMaterial({
          color: GRID_COLOR_BRIGHT,
          toneMapped: false,
          transparent: true,
          opacity: 0.5,
          depthWrite: false,
        }),
      );
      m.position.set(x, halfH + 0.15, 0);
      group.add(m);
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
    this.particleSystem = new THREE.Points(
      geo,
      new THREE.PointsMaterial({
        size: 0.04,
        color: 0x66ddff,
        transparent: true,
        opacity: 0.5,
        sizeAttenuation: true,
        depthWrite: false,
      }),
    );
    this.scene.add(this.particleSystem);
  }

  async _loadCharacters() {
    const loader = new GLTFLoader();
    // React seats: blue (-PLAYER_DISTANCE, 0, 0) rotation Math.PI/2,
    //              orange (PLAYER_DISTANCE, 0, 0) rotation -Math.PI/2.
    const configs = [
      {
        slot: 'blue',
        path: CHARACTER_MODELS[0],
        color: this.blueColor,
        position: new THREE.Vector3(-PLAYER_DISTANCE, 0, 0),
        facingY: Math.PI / 2,
      },
      {
        slot: 'orange',
        path: CHARACTER_MODELS[1],
        color: this.orangeColor,
        position: new THREE.Vector3(PLAYER_DISTANCE, 0, 0),
        facingY: -Math.PI / 2,
      },
    ];

    const promises = configs.map(
      (cfg) =>
        new Promise((resolve) => {
          loader.load(
            cfg.path,
            (gltf) => {
              const seat = buildChair(cfg.color);
              seat.position.copy(cfg.position);
              seat.rotation.y = cfg.facingY;
              const accent = new THREE.PointLight(cfg.color, 1.0, 8);
              accent.position.y = 2.0;
              seat.add(accent);
              this.scene.add(seat);

              const model = SkeletonUtils.clone(gltf.scene);
              model.scale.setScalar(CHARACTER_SCALE);
              model.position.y = SEAT_TOP_Y;
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
              console.warn(`[C4Renderer] failed to load ${cfg.path}`, err);
              resolve();
            },
          );
        }),
    );

    await Promise.all(promises);
  }

  // ─── Game state → scene reconciliation ───

  syncBoard(board) {
    for (let row = 0; row < C4_ROWS; row += 1) {
      for (let col = 0; col < C4_COLS; col += 1) {
        const cell = board[row][col];
        const key = `${row},${col}`;
        if (cell === 'empty') {
          if (this.discs.has(key)) {
            const old = this.discs.get(key);
            this.scene.remove(old.group);
            this.discs.delete(key);
          }
          continue;
        }
        if (this.discs.has(key)) continue;
        const pos = cellToPosition(row, col);
        const color = cell === 'blue' ? this.blueColor : this.orangeColor;
        const piece = this._buildDisc(pos, color);
        this.scene.add(piece.group);
        this.discs.set(key, piece);
      }
    }
  }

  _buildDisc(position, hexColor) {
    const group = new THREE.Group();
    group.position.set(...position);
    // Discs face the camera (the board is in the XY plane at z≈0).
    group.rotation.x = Math.PI / 2;
    group.scale.set(0, 0, 0);

    const color = new THREE.Color(hexColor);
    const main = new THREE.Mesh(
      new THREE.CylinderGeometry(0.18, 0.18, 0.06, 24),
      new THREE.MeshStandardMaterial({
        color,
        emissive: color,
        emissiveIntensity: 1.2,
        toneMapped: false,
      }),
    );
    group.add(main);

    const halo = new THREE.Mesh(
      new THREE.CircleGeometry(0.22, 24),
      new THREE.MeshBasicMaterial({
        color,
        transparent: true,
        opacity: 0.06,
        toneMapped: false,
        side: THREE.DoubleSide,
      }),
    );
    halo.rotation.x = -Math.PI / 2;
    group.add(halo);

    return { group };
  }

  /** Replace the win-highlight rings. Pass null to clear. */
  setWinHighlight(board) {
    if (this.winHighlightGroup) {
      this.scene.remove(this.winHighlightGroup);
      this.winHighlightGroup = null;
      this._winHighlightMats = [];
    }
    if (!board) return;
    const positions = findWinningCells(board);
    if (!positions) return;

    const group = new THREE.Group();
    for (const pos of positions) {
      const ring = new THREE.Mesh(
        new THREE.RingGeometry(0.2, 0.26, 24),
        new THREE.MeshBasicMaterial({
          color: 0xffe040,
          toneMapped: false,
          transparent: true,
          opacity: 0.7,
          depthWrite: false,
          side: THREE.DoubleSide,
        }),
      );
      ring.position.set(...pos);
      group.add(ring);
      this._winHighlightMats.push(ring.material);
    }
    this.scene.add(group);
    this.winHighlightGroup = group;
  }

  // ─── Per-frame update ───

  update() {
    // getDelta() before getElapsedTime() — see PORTING_VIEWERS_FROM_R3F.md rule 4.
    const dt = this.clock.getDelta();
    const t = this.clock.getElapsedTime();

    if (this.arenaMaterials) {
      for (const m of this.arenaMaterials) {
        if (m.uniforms?.time) m.uniforms.time.value = t;
      }
    }

    for (const m of this.characterMaterials) {
      if (m.uniforms?.time) m.uniforms.time.value = t;
    }

    for (const mixer of this.mixers) mixer.update(dt);

    // Board frame pulse (matches React HolographicBoardFrame frame opacity)
    if (this._boardPulseMats) {
      const opacity = 0.45 + Math.sin(t * 1.2) * 0.1;
      for (const mat of this._boardPulseMats) mat.opacity = opacity;
    }

    // Disc pop-in animation
    for (const piece of this.discs.values()) {
      const s = piece.group.scale.x;
      if (s < 1) {
        const next = Math.min(s + 0.1, 1);
        piece.group.scale.set(next, next, next);
      }
    }

    // Win-highlight pulse
    if (this._winHighlightMats.length) {
      const opacity = 0.25 + Math.sin(t * 3) * 0.15;
      for (const mat of this._winHighlightMats) mat.opacity = opacity;
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

    // Auto-rotate camera around target
    this.cameraTheta += dt * this.cameraOrbitSpeed;
    this.camera.position.x = this.cameraTarget.x + Math.cos(this.cameraTheta) * this.cameraRadius;
    this.camera.position.z = this.cameraTarget.z + Math.sin(this.cameraTheta) * this.cameraRadius;
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
