/**
 * Poker 3D Renderer
 *
 * Translated from the React Three Fiber PokerHolographicScene. Same TTT-style
 * scene scaffolding (arena, characters, top-down auto-rotating camera) plus
 * a green-felt poker table centerpiece, holographic playing cards, chip stacks,
 * and a dealer-button indicator.
 *
 * Cards in the OSS poker protocol are { rank, suit } JSON objects (vs the
 * React app's pre-stringified "Ah" form); we read .suit directly to pick
 * the holographic card tint.
 */

import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import * as SkeletonUtils from 'three/addons/utils/SkeletonUtils.js';
import { buildArena } from './shared/shared-arena.js';
import { buildChair } from './shared/shared-chair.js';
import { createPostProcessing } from './shared/shared-postprocessing.js';
import { createHolographicMaterial, createWireframeMaterial } from './shared/shared-materials.js';
import { PLAYER_COLORS } from './shared/shared-colors.js';

const TABLE_Y = 1.0;
const PLAYER_DISTANCE = 3.2;
const CHARACTER_SCALE = 2.5;
const SEAT_TOP_Y = 0.73;
const TABLE_COLOR = 0x0d4a30;
const ACCENT_HEX = 0x00e5cc;
const CHIP_GOLD = 0xd4a843;
const BUTTON_GRAY = 0xcccccc;

const CHARACTER_MODELS = [
  'assets/characters/character_01.glb',
  'assets/characters/character_02.glb',
];

// Hearts/Diamonds → red; Clubs/Spades → light blue/gray. Matches React.
const SUIT_COLORS = {
  hearts: 0xff4444,
  diamonds: 0xff4444,
  clubs: 0xaaccdd,
  spades: 0xaaccdd,
};

function suitColor(suit) {
  return SUIT_COLORS[String(suit).toLowerCase()] ?? 0xaaccdd;
}

export class PokerRenderer {
  constructor(canvas, { p1Color, p2Color } = {}) {
    this.p1Color = p1Color ?? PLAYER_COLORS[0];
    this.p2Color = p2Color ?? PLAYER_COLORS[1];
    this.canvas = canvas;
    this.clock = new THREE.Clock();

    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x0a1520);
    this.scene.fog = new THREE.FogExp2(0x0a1520, 0.012);

    this.camera = new THREE.PerspectiveCamera(
      50,
      window.innerWidth / window.innerHeight,
      0.1,
      100,
    );
    // React Canvas + AutoCamera: (3, 9.1, 3) lookAt (0, 1, 0).
    this.camera.position.set(3, 9.1, 3);
    this.camera.lookAt(0, 1, 0);

    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: false });
    this.renderer.setSize(window.innerWidth, window.innerHeight);
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.shadowMap.enabled = true;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.2;

    const arena = buildArena(this.scene, {
      wallRadiusTop: 12,
      wallRadiusBottom: 14,
      wallHeight: 10,
      wallY: 3,
      floorRadius: 14,
    });
    this.arenaMaterials = arena.materials;

    const pp = createPostProcessing(this.renderer, this.scene, this.camera, {
      bloom: { strength: 0.5, radius: 0.5, threshold: 0.35 },
    });
    this.composer = pp.composer;
    this.bloomPass = pp.bloomPass;

    this._setupLights();
    this._buildTable();
    this._buildParticles();

    this.characters = { p1: null, p2: null };
    this.characterMaterials = [];
    this.mixers = [];
    this.charactersLoaded = this._loadCharacters();

    this.communityCardsGroup = new THREE.Group();
    this.communityCardsGroup.position.set(0, TABLE_Y, 0);
    this.scene.add(this.communityCardsGroup);
    this._communityCache = [];
    this._communityCount = 0;

    this.holeCardsGroup = new THREE.Group();
    this.holeCardsGroup.position.set(0, TABLE_Y, 0);
    this.scene.add(this.holeCardsGroup);
    this._holeCache = [[], []];

    this.chipsGroup = new THREE.Group();
    this.chipsGroup.position.set(0, TABLE_Y, 0);
    this.scene.add(this.chipsGroup);
    const chip0 = this._buildChip(CHIP_GOLD);
    chip0.position.set(-0.4, 0.001, -0.6);
    this.chipsGroup.add(chip0);
    const chip1 = this._buildChip(CHIP_GOLD);
    chip1.position.set(0.4, 0.001, 0.6);
    this.chipsGroup.add(chip1);

    this.dealerGroup = new THREE.Group();
    this.dealerGroup.position.set(0, TABLE_Y, 0);
    this.scene.add(this.dealerGroup);
    this._dealerMesh = new THREE.Mesh(
      new THREE.CircleGeometry(0.09, 16),
      new THREE.MeshBasicMaterial({
        color: BUTTON_GRAY,
        toneMapped: false,
        transparent: true,
        opacity: 0.7,
        side: THREE.DoubleSide,
      }),
    );
    this._dealerMesh.rotation.x = -Math.PI / 2;
    this.dealerGroup.add(this._dealerMesh);
    this._dealerButton = null;

    // Auto-rotating camera (target [0, 0.8, 0], autoRotateSpeed=0.35 matches TTT).
    this.cameraTarget = new THREE.Vector3(0, 0.8, 0);
    const dx = this.camera.position.x - this.cameraTarget.x;
    const dz = this.camera.position.z - this.cameraTarget.z;
    this.cameraTheta = Math.atan2(dz, dx);
    this.cameraRadius = Math.sqrt(dx * dx + dz * dz);
    this.cameraHeight = this.camera.position.y;

    this._onResize = this._handleResize.bind(this);
    window.addEventListener('resize', this._onResize);
  }

  _setupLights() {
    this.scene.add(new THREE.AmbientLight(0x334455, 0.6));

    const directional = new THREE.DirectionalLight(0xaaccff, 0.8);
    directional.position.set(5, 12, 8);
    directional.castShadow = true;
    this.scene.add(directional);

    const fill = new THREE.PointLight(0x113344, 0.4, 30);
    fill.position.set(0, -1, 0);
    this.scene.add(fill);

    // Per-player accent point lights. React used intensity=12 with HDR + ACES;
    // scaled to 1.5 for OSS bloom per the porting guide.
    const p1Accent = new THREE.PointLight(this._dim(this.p1Color), 1.5, 8);
    p1Accent.position.set(0, 3, -PLAYER_DISTANCE);
    this.scene.add(p1Accent);

    const p2Accent = new THREE.PointLight(this._dim(this.p2Color), 1.5, 8);
    p2Accent.position.set(0, 3, PLAYER_DISTANCE);
    this.scene.add(p2Accent);
  }

  _dim(hex) {
    const c = new THREE.Color(hex);
    c.multiplyScalar(0.4);
    return c;
  }

  _buildTable() {
    const group = new THREE.Group();
    group.position.y = TABLE_Y - 0.02;

    // Felt
    const felt = new THREE.Mesh(
      new THREE.CircleGeometry(1.2, 48),
      new THREE.MeshStandardMaterial({
        color: TABLE_COLOR,
        emissive: 0x0a3322,
        emissiveIntensity: 0.4,
        metalness: 0.3,
        roughness: 0.7,
        transparent: true,
        opacity: 0.7,
      }),
    );
    felt.rotation.x = -Math.PI / 2;
    group.add(felt);

    // Glow ring
    const ring = new THREE.Mesh(
      new THREE.TorusGeometry(1.2, 0.015, 8, 48),
      new THREE.MeshBasicMaterial({
        color: ACCENT_HEX,
        transparent: true,
        opacity: 0.3,
      }),
    );
    ring.rotation.x = -Math.PI / 2;
    ring.position.y = 0.01;
    group.add(ring);

    this.tableGroup = group;
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
    const configs = [
      {
        slot: 'p1',
        path: CHARACTER_MODELS[0],
        color: this.p1Color,
        position: new THREE.Vector3(0, 0, -PLAYER_DISTANCE),
        facingY: 0, // face +Z (table center)
      },
      {
        slot: 'p2',
        path: CHARACTER_MODELS[1],
        color: this.p2Color,
        position: new THREE.Vector3(0, 0, PLAYER_DISTANCE),
        facingY: Math.PI,
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
                const idle = gltf.animations.find((c) => c.name === 'idle_seated') || gltf.animations[0];
                const action = mixer.clipAction(idle.clone());
                action.play();
                this.mixers.push(mixer);
              }

              seat.add(model);
              this.characters[cfg.slot] = { seat, model, color: cfg.color };
              resolve();
            },
            undefined,
            (err) => {
              console.warn(`[PokerRenderer] failed to load ${cfg.path}`, err);
              resolve();
            },
          );
        }),
    );

    await Promise.all(promises);
  }

  // ─── Card / chip geometry helpers ───

  _buildCard(color, faceDown = false) {
    const group = new THREE.Group();
    const cardColor = faceDown ? new THREE.Color(0x224466) : new THREE.Color(color);
    const baseOpacity = faceDown ? 0.15 : 0.2;
    const wireOpacity = faceDown ? 0.3 : 0.5;

    const baseMesh = new THREE.Mesh(
      new THREE.PlaneGeometry(0.18, 0.26),
      new THREE.MeshBasicMaterial({
        color: cardColor,
        toneMapped: false,
        transparent: true,
        opacity: baseOpacity,
        side: THREE.DoubleSide,
      }),
    );
    baseMesh.rotation.x = -Math.PI / 2;
    group.add(baseMesh);

    const wireMesh = new THREE.Mesh(
      new THREE.PlaneGeometry(0.18, 0.26),
      new THREE.MeshBasicMaterial({
        wireframe: true,
        color: cardColor,
        toneMapped: false,
        transparent: true,
        opacity: wireOpacity,
      }),
    );
    wireMesh.rotation.x = -Math.PI / 2;
    group.add(wireMesh);

    return group;
  }

  _buildChip(colorHex) {
    const chip = new THREE.Mesh(
      new THREE.CircleGeometry(0.07, 16),
      new THREE.MeshBasicMaterial({
        color: colorHex,
        toneMapped: false,
        transparent: true,
        opacity: 0.6,
        side: THREE.DoubleSide,
      }),
    );
    chip.rotation.x = -Math.PI / 2;
    return chip;
  }

  // ─── State → scene reconciliation ───

  /**
   * Update the table to reflect the latest hand snapshot. Idempotent —
   * the viewer calls this on every state refetch and we replace mutable
   * groups wholesale; cards "pop in" via a staggered scale tween when the
   * community count grows.
   */
  syncHand({ currentHand, button }) {
    const community = currentHand?.community ?? [];
    this._reconcileCommunity(community);

    const holeCards = currentHand?.hole_cards ?? [[], []];
    const folded = currentHand?.folded ?? [false, false];
    this._reconcileHole(0, holeCards[0] ?? [], folded[0], -0.55);
    this._reconcileHole(1, holeCards[1] ?? [], folded[1], 0.55);

    if (button !== this._dealerButton) {
      this._dealerButton = button;
      const x = button === 0 ? -0.62 : 0.62;
      const z = button === 0 ? -0.6 : 0.6;
      this._dealerMesh.position.set(x, 0.003, z);
    }
  }

  _reconcileCommunity(community) {
    while (this._communityCache.length > community.length) {
      const entry = this._communityCache.pop();
      this.communityCardsGroup.remove(entry.group);
      this._disposeGroup(entry.group);
    }
    for (let i = 0; i < community.length; i += 1) {
      const card = community[i];
      const sig = `${card.suit}|${card.rank}`;
      const cached = this._communityCache[i];
      // Row is centered around X=0 so every card's X depends on the total
      // count — update existing cards' positions when the row grows.
      const xPos = (i - (community.length - 1) / 2) * 0.24;
      if (cached && cached.sig === sig) {
        cached.group.position.set(xPos, 0.001, 0);
        continue;
      }
      if (cached) {
        this.communityCardsGroup.remove(cached.group);
        this._disposeGroup(cached.group);
      }
      const group = this._buildCard(suitColor(card.suit), false);
      group.position.set(xPos, 0.001, 0);
      if (i >= this._communityCount) {
        group.scale.set(0, 0, 0);
        group.userData.popIn = true;
      }
      this.communityCardsGroup.add(group);
      this._communityCache[i] = { sig, group };
    }
    this._communityCount = community.length;
  }

  _reconcileHole(playerIdx, cards, foldedFlag, baseZ) {
    const cache = this._holeCache[playerIdx];
    while (cache.length > cards.length) {
      const entry = cache.pop();
      this.holeCardsGroup.remove(entry.group);
      this._disposeGroup(entry.group);
    }
    for (let i = 0; i < cards.length; i += 1) {
      const card = cards[i];
      const sig = `${card.suit}|${card.rank}|${foldedFlag ? 'down' : 'up'}`;
      const cached = cache[i];
      if (cached && cached.sig === sig) continue;
      if (cached) {
        this.holeCardsGroup.remove(cached.group);
        this._disposeGroup(cached.group);
      }
      const group = this._buildCard(suitColor(card.suit), foldedFlag);
      group.position.set(-0.12 + i * 0.22, 0.001, baseZ);
      this.holeCardsGroup.add(group);
      cache[i] = { sig, group };
    }
  }

  _disposeGroup(group) {
    group.traverse((node) => {
      if (node.geometry) node.geometry.dispose?.();
      if (node.material) {
        if (Array.isArray(node.material)) {
          node.material.forEach((m) => m.dispose?.());
        } else {
          node.material.dispose?.();
        }
      }
    });
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

    // Community-card pop-in
    for (const child of this.communityCardsGroup.children) {
      if (child.userData.popIn && child.scale.x < 1) {
        const next = Math.min(child.scale.x + 0.08, 1);
        child.scale.set(next, next, next);
        if (next >= 1) child.userData.popIn = false;
      }
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

    // Auto-rotate camera (~0.037 rad/s = autoRotateSpeed=0.35 in OrbitControls)
    this.cameraTheta += dt * 0.037;
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
