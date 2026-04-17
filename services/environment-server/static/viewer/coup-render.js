/**
 * Coup 3D Rendering Module
 * Three.js scene setup with sci-fi arena aesthetic
 * Enhanced with proper post-processing, custom shaders, and lighting
 */

import * as THREE from 'three';
import { createPostProcessing } from './shared/shared-postprocessing.js';
import { buildArena } from './shared/shared-arena.js';
import { buildChair } from './shared/shared-chair.js';

// Character system — GLB models with role aura
import {
  PLAYER_COLORS,
  getPlayerColor,
  createCharacter,
  setCoupCharacterPose,
  setPlayerActiveVisuals,
  setPlayerEliminatedVisuals,
  setRoleAura,
  clearRoleAura,
} from './coup-characters.js';

// Scene color constants — aligned with shared arena palette
const COLORS = {
  background: 0x0A1520,
  tableTop: 0x1a2835,
  tableGlow: 0x00E5CC,
  ambient: 0x4a6688,
  skyColor: 0x6a8aaa,
  groundColor: 0x2a3840,
  rimLight: 0x00E5CC,
  spotlight: 0xffffff,
  cardFront: 0x1a2a35,
  cardBack: 0x0d1820,
  coin: 0xd4af37,
};

export class CoupRenderer {
  constructor(canvas) {
    this.canvas = canvas;
    this.scene = null;
    this.camera = null;
    this.renderer = null;
    this.composer = null;
    this.bloomPass = null;

    // Scene objects
    this.table = null;
    this.tableGlowMesh = null;
    this.players = new Map();
    this.cards = new Map();
    this.coins = new Map();
    this.deck = null;
    this.roleSigils = new Map();

    // Ambient particles
    this.ambientParticles = null;

    // Holographic materials (for time uniform updates)
    this.holographicMaterials = [];

    // Animation state
    this.clock = new THREE.Clock();
    this.orbitAngle = 0;

    // Active player tracking (for head look targeting)
    this.activePlayerId = 0;

    // Bound event handlers for proper cleanup
    this.boundOnResize = this.onResize.bind(this);

    this.init();
  }

  init() {
    // Scene
    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(COLORS.background);
    this.scene.fog = new THREE.FogExp2(COLORS.background, 0.008);

    // Camera
    this.camera = new THREE.PerspectiveCamera(
      55,
      window.innerWidth / window.innerHeight,
      0.1,
      100
    );
    this.camera.position.set(0, 6.5, 12);
    this.camera.lookAt(0, 0.5, 0);

    // Renderer with proper settings
    this.renderer = new THREE.WebGLRenderer({
      canvas: this.canvas,
      antialias: true,
      alpha: false,
      powerPreference: 'high-performance',
    });
    this.renderer.setSize(window.innerWidth, window.innerHeight);
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.8;
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;

    // Post-processing
    this.setupPostProcessing();

    // Lighting
    this.setupLighting();

    // Arena elements
    this.createArenaBackdrop();
    this.createTable();
    this.createDeck();
    this.createAmbientParticles();

    // Handle resize
    window.addEventListener('resize', this.boundOnResize);
  }

  setupPostProcessing() {
    const { composer, bloomPass } = createPostProcessing(
      this.renderer, this.scene, this.camera, {
        bloom: { strength: 0.5, radius: 0.5, threshold: 0.35 },
        chromaticAberration: { amount: 0.001 },
        vignette: { darkness: 0.15 },
      }
    );
    this.composer = composer;
    this.bloomPass = bloomPass;
  }

  setupLighting() {
    // Hemisphere light for ambient sky/ground gradient
    const hemiLight = new THREE.HemisphereLight(
      COLORS.skyColor,
      COLORS.groundColor,
      1.2
    );
    hemiLight.position.set(0, 20, 0);
    this.scene.add(hemiLight);

    // Ambient fill
    const ambient = new THREE.AmbientLight(COLORS.ambient, 0.65);
    this.scene.add(ambient);

    // Key light - top spotlight for card visibility
    const keyLight = new THREE.SpotLight(COLORS.spotlight, 2.0);
    keyLight.position.set(0, 15, 5);
    keyLight.angle = Math.PI / 3.5;
    keyLight.penumbra = 0.7;
    keyLight.decay = 1.5;
    keyLight.distance = 50;
    this.scene.add(keyLight);

    // Fill light from opposite side
    const fillLight = new THREE.SpotLight(0x4488ff, 1.0);
    fillLight.position.set(-8, 8, -5);
    fillLight.angle = Math.PI / 3;
    fillLight.penumbra = 0.8;
    this.scene.add(fillLight);

    // Back fill light for depth
    const backFill = new THREE.SpotLight(0x336688, 0.8);
    backFill.position.set(5, 10, -8);
    backFill.angle = Math.PI / 4;
    backFill.penumbra = 0.8;
    this.scene.add(backFill);

    // Rim lights around the table for neon edge glow
    const rimPositions = [
      [5, 1.5, 0],
      [-5, 1.5, 0],
      [0, 1.5, 5],
      [0, 1.5, -5],
      [3.5, 1.5, 3.5],
      [-3.5, 1.5, 3.5],
    ];
    for (const pos of rimPositions) {
      const rim = new THREE.PointLight(COLORS.rimLight, 0.8, 10, 2);
      rim.position.set(...pos);
      this.scene.add(rim);
    }

    // Table center light for warm glow on cards/deck
    const tableLight = new THREE.PointLight(COLORS.tableGlow, 0.6, 6, 2);
    tableLight.position.set(0, 0.5, 0);
    this.scene.add(tableLight);
  }

  createArenaBackdrop() {
    const { materials } = buildArena(this.scene);
    this.holographicMaterials.push(...materials);
  }

  createTable() {
    // Hexagonal table shape
    const tableShape = new THREE.Shape();
    const sides = 6;
    const radius = 4;
    for (let i = 0; i < sides; i++) {
      const angle = (i / sides) * Math.PI * 2 - Math.PI / 2;
      const x = Math.cos(angle) * radius;
      const y = Math.sin(angle) * radius;
      if (i === 0) {
        tableShape.moveTo(x, y);
      } else {
        tableShape.lineTo(x, y);
      }
    }
    tableShape.closePath();

    // Extrude for table top
    const extrudeSettings = { depth: 0.2, bevelEnabled: true, bevelSize: 0.05, bevelThickness: 0.05 };
    const tableGeometry = new THREE.ExtrudeGeometry(tableShape, extrudeSettings);

    const tableMaterial = new THREE.MeshStandardMaterial({
      color: COLORS.tableTop,
      metalness: 0.8,
      roughness: 0.2,
    });

    this.table = new THREE.Mesh(tableGeometry, tableMaterial);
    this.table.rotation.x = -Math.PI / 2;
    this.table.position.y = 0;
    this.scene.add(this.table);

    // Glowing edge using emissive material for bloom pickup
    const edgeGeometry = new THREE.TorusGeometry(radius, 0.03, 8, sides);
    const edgeMaterial = new THREE.MeshBasicMaterial({
      color: COLORS.tableGlow,
    });
    const edge = new THREE.Mesh(edgeGeometry, edgeMaterial);
    edge.rotation.x = Math.PI / 2;
    edge.position.y = 0.21;
    this.scene.add(edge);

    // Inner glow ring
    const innerEdgeGeometry = new THREE.TorusGeometry(radius - 0.4, 0.02, 8, sides);
    const innerEdgeMaterial = new THREE.MeshBasicMaterial({
      color: COLORS.tableGlow,
      transparent: true,
      opacity: 0.4,
    });
    const innerEdge = new THREE.Mesh(innerEdgeGeometry, innerEdgeMaterial);
    innerEdge.rotation.x = Math.PI / 2;
    innerEdge.position.y = 0.21;
    this.scene.add(innerEdge);

    // Table surface glow plane
    const glowPlaneGeometry = new THREE.CircleGeometry(radius - 0.2, 6);
    const glowPlaneMaterial = new THREE.MeshBasicMaterial({
      color: COLORS.tableGlow,
      transparent: true,
      opacity: 0.06,
    });
    const glowPlane = new THREE.Mesh(glowPlaneGeometry, glowPlaneMaterial);
    glowPlane.rotation.x = -Math.PI / 2;
    glowPlane.position.y = 0.22;
    this.tableGlowMesh = glowPlane;
    this.scene.add(glowPlane);
  }

  createAmbientParticles() {
    // Floating dust/energy particles for atmosphere
    const particleCount = 100;
    const geometry = new THREE.BufferGeometry();
    const positions = new Float32Array(particleCount * 3);
    const sizes = new Float32Array(particleCount);
    const phases = new Float32Array(particleCount);

    for (let i = 0; i < particleCount; i++) {
      // Distribute in a cylinder around the table
      const angle = Math.random() * Math.PI * 2;
      const radius = 3 + Math.random() * 12;
      const height = Math.random() * 8;

      positions[i * 3] = Math.cos(angle) * radius;
      positions[i * 3 + 1] = height;
      positions[i * 3 + 2] = Math.sin(angle) * radius;

      sizes[i] = 0.02 + Math.random() * 0.04;
      phases[i] = Math.random() * Math.PI * 2;
    }

    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    geometry.setAttribute('size', new THREE.BufferAttribute(sizes, 1));
    geometry.setAttribute('phase', new THREE.BufferAttribute(phases, 1));

    // Custom shader for particles that drift and twinkle
    const material = new THREE.ShaderMaterial({
      uniforms: {
        time: { value: 0 },
        color: { value: new THREE.Color(COLORS.tableGlow) },
      },
      vertexShader: `
        attribute float size;
        attribute float phase;
        uniform float time;
        varying float vAlpha;

        void main() {
          vec3 pos = position;

          // Gentle floating motion
          pos.y += sin(time * 0.5 + phase) * 0.3;
          pos.x += sin(time * 0.3 + phase * 2.0) * 0.2;
          pos.z += cos(time * 0.4 + phase) * 0.2;

          // Twinkle effect
          vAlpha = 0.3 + 0.4 * sin(time * 2.0 + phase * 3.0);

          vec4 mvPosition = modelViewMatrix * vec4(pos, 1.0);
          gl_PointSize = size * 300.0 / -mvPosition.z;
          gl_Position = projectionMatrix * mvPosition;
        }
      `,
      fragmentShader: `
        uniform vec3 color;
        varying float vAlpha;

        void main() {
          // Soft circle shape
          float dist = length(gl_PointCoord - 0.5) * 2.0;
          if (dist > 1.0) discard;

          float alpha = vAlpha * (1.0 - dist * dist);
          gl_FragColor = vec4(color, alpha * 0.6);
        }
      `,
      transparent: true,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
    });

    this.holographicMaterials.push(material);

    this.ambientParticles = new THREE.Points(geometry, material);
    this.scene.add(this.ambientParticles);
  }

  /**
   * Creates a sci-fi chair for a player seat position.
   */
  createPlayerPlatform(playerColor) {
    return buildChair(playerColor);
  }

  createDeck() {
    // Stack of cards in the center with holographic effect
    const deckGroup = new THREE.Group();

    for (let i = 0; i < 10; i++) {
      const cardGeometry = new THREE.BoxGeometry(0.5, 0.015, 0.7);
      const cardMaterial = new THREE.MeshStandardMaterial({
        color: COLORS.cardBack,
        metalness: 0.7,
        roughness: 0.3,
        emissive: COLORS.tableGlow,
        emissiveIntensity: 0.02,
      });
      const card = new THREE.Mesh(cardGeometry, cardMaterial);
      card.position.y = i * 0.018;
      // Slight random rotation for realism
      card.rotation.y = (Math.random() - 0.5) * 0.05;
      deckGroup.add(card);
    }

    deckGroup.position.set(0, 0.25, 0);
    this.deck = deckGroup;
    this.scene.add(deckGroup);

    // Deck glow ring
    const glowGeometry = new THREE.RingGeometry(0.5, 0.7, 32);
    const glowMaterial = new THREE.MeshBasicMaterial({
      color: COLORS.tableGlow,
      transparent: true,
      opacity: 0.15,
      side: THREE.DoubleSide,
    });
    const glow = new THREE.Mesh(glowGeometry, glowMaterial);
    glow.rotation.x = -Math.PI / 2;
    glow.position.set(0, 0.22, 0);
    this.scene.add(glow);
  }

  createPlayer(playerId, totalPlayers) {
    const angle = ((playerId / totalPlayers) * Math.PI * 2) - Math.PI / 2;
    const distance = 5.5;
    const x = Math.cos(angle) * distance;
    const z = Math.sin(angle) * distance;

    const playerGroup = new THREE.Group();
    playerGroup.position.set(x, 0, z);
    playerGroup.lookAt(0, 0, 0);

    // Get player color from character system
    const playerColor = getPlayerColor(playerId);

    // Create floating seat platform (matches Vibe Check style)
    const platform = this.createPlayerPlatform(playerColor);
    playerGroup.add(platform);

    // Player point light — positioned above the scaled character
    const light = new THREE.PointLight(playerColor, 1.8, 8, 2);
    light.position.y = 2.8;
    playerGroup.add(light);

    this.scene.add(playerGroup);
    this.players.set(playerId, {
      group: playerGroup,
      humanoid: null,      // set async after GLB loads
      humanoidMaterial: null,
      mixer: null,
      light,
      groundGlow: null,
      color: playerColor,
      eliminated: false,
      dissolving: false,
    });

    // Create card positions for this player
    this.createPlayerCards(playerId, playerGroup);

    // Create coin stack
    this.createPlayerCoins(playerId, playerGroup);

    // Launch async character load (promise collected by initializePlayers)
    return this.loadCharacterAsync(playerId, playerGroup, playerColor);
  }

  async loadCharacterAsync(playerId, playerGroup, playerColor) {
    const modelIndex = playerId % 6;
    try {
      const character = await createCharacter(modelIndex, playerColor);
      character.group.scale.set(3.0, 3.0, 3.0);
      character.group.rotation.y = Math.PI; // GLB faces +Z, rotate to face table center
      character.group.position.y = 0.73; // Sit on chair seat surface
      playerGroup.add(character.group);

      this.holographicMaterials.push(character.bodyMaterial);
      this.holographicMaterials.push(character.wireframeMaterial);

      const player = this.players.get(playerId);
      if (player) {
        player.humanoid = character.group;
        player.humanoidMaterial = character.bodyMaterial;
        player.mixer = character.mixer;

        // Idle life animation state (staggered per player)
        const elapsed = this.clock.getElapsedTime();
        player.idleState = {
          nextPoseShift: elapsed + 5 + Math.random() * 15,
          poseRevertTime: 0,
          headTargetY: 0,
          headCurrentY: 0,
          nextHeadShift: elapsed + 2 + Math.random() * 5,
          breathPhase: playerId * 1.3,
          reactionEndTime: 0,
        };
      }
    } catch (err) {
      console.warn(`Failed to load character for player ${playerId}:`, err);
    }
  }

  createPlayerCards(playerId, playerGroup) {
    const cardGroup = new THREE.Group();

    for (let i = 0; i < 2; i++) {
      const cardGeometry = new THREE.BoxGeometry(0.5, 0.02, 0.7);
      const cardMaterial = new THREE.MeshStandardMaterial({
        color: COLORS.cardBack,
        metalness: 0.7,
        roughness: 0.3,
        emissive: COLORS.tableGlow,
        emissiveIntensity: 0.03,
      });
      const card = new THREE.Mesh(cardGeometry, cardMaterial);
      // Cards positioned on the seat surface (seat top ~0.78) in front of player
      card.position.set((i - 0.5) * 0.6, 0.80, 0.9);
      card.userData = { playerId, cardIndex: i, revealed: false };
      cardGroup.add(card);
    }

    playerGroup.add(cardGroup);
    this.cards.set(playerId, cardGroup);
  }

  createPlayerCoins(playerId, playerGroup) {
    const coinGroup = new THREE.Group();
    // Coins positioned on the seat surface beside cards
    coinGroup.position.set(0, 0.80, 1.5);
    coinGroup.userData = { count: 2 };

    this.updateCoinStack(coinGroup, 2);

    playerGroup.add(coinGroup);
    this.coins.set(playerId, coinGroup);
  }

  updateCoinStack(coinGroup, count) {
    // Clear existing coins
    while (coinGroup.children.length > 0) {
      const child = coinGroup.children[0];
      coinGroup.remove(child);
      if (child.geometry) child.geometry.dispose();
      if (child.material) child.material.dispose();
    }

    // Create new stack with proper metallic coins
    const coinGeometry = new THREE.CylinderGeometry(0.12, 0.12, 0.04, 12);
    const coinMaterial = new THREE.MeshStandardMaterial({
      color: COLORS.coin,
      metalness: 0.9,
      roughness: 0.1,
      emissive: COLORS.coin,
      emissiveIntensity: 0.05,
    });

    const maxVisible = Math.min(count, 10);
    for (let i = 0; i < maxVisible; i++) {
      const coin = new THREE.Mesh(coinGeometry, coinMaterial);
      coin.position.y = i * 0.05;
      // Slight random offset for natural look
      coin.position.x = (Math.random() - 0.5) * 0.02;
      coin.position.z = (Math.random() - 0.5) * 0.02;
      coinGroup.add(coin);
    }

    coinGroup.userData.count = count;
  }

  setPlayerActive(playerId, active) {
    const player = this.players.get(playerId);
    if (!player) return;

    if (active) this.activePlayerId = playerId;

    if (player.humanoid) {
      setPlayerActiveVisuals(player.humanoid, player.light, player.groundGlow, active);
    }
  }

  setPlayerEliminated(playerId) {
    const player = this.players.get(playerId);
    if (!player) return;

    player.eliminated = true;
    if (player.humanoid) {
      setPlayerEliminatedVisuals(player.humanoid, player.light, player.groundGlow);
    }
  }

  // Role Aura (color blending) methods
  setPlayerRoleAura(playerId, role) {
    const player = this.players.get(playerId);
    if (!player || !player.humanoid) return;
    const roleColor = this.getRoleColor(role);
    setRoleAura(player.humanoid, roleColor, this.scene);
  }

  clearPlayerRoleAura(playerId) {
    const player = this.players.get(playerId);
    if (!player || !player.humanoid) return;
    clearRoleAura(player.humanoid, player.color, this.scene);
  }

  // Body morph — no-op with GLB models (role aura provides visual role feedback)
  morphPlayerToRole(playerId, role) {
    const player = this.players.get(playerId);
    if (player) player.currentRole = role;
    return null;
  }

  revertPlayerShape(playerId) {
    const player = this.players.get(playerId);
    if (player) player.currentRole = null;
    return null;
  }

  updatePlayerCoins(playerId, count) {
    const coinGroup = this.coins.get(playerId);
    if (coinGroup) {
      this.updateCoinStack(coinGroup, count);
    }
  }

  revealCard(playerId, cardIndex, role) {
    const cardGroup = this.cards.get(playerId);
    if (!cardGroup || !cardGroup.children[cardIndex]) return;

    const card = cardGroup.children[cardIndex];
    card.userData.revealed = true;
    card.userData.role = role;

    // Change material to show revealed/dead state
    card.material.color.setHex(0x1a0808);
    card.material.emissive.setHex(0xff2222);
    card.material.emissiveIntensity = 0.15;
  }

  createRoleSigil(playerId, role) {
    const player = this.players.get(playerId);
    if (!player) return null;

    // Remove existing sigil if any
    this.removeRoleSigil(playerId);

    const sigilGroup = new THREE.Group();
    const roleColor = this.getRoleColor(role);

    // Hexagonal frame background (matches sci-fi aesthetic)
    const hexShape = new THREE.Shape();
    const hexRadius = 0.4;
    for (let i = 0; i < 6; i++) {
      const angle = (i / 6) * Math.PI * 2 + Math.PI / 6;
      const x = Math.cos(angle) * hexRadius;
      const y = Math.sin(angle) * hexRadius;
      if (i === 0) hexShape.moveTo(x, y);
      else hexShape.lineTo(x, y);
    }
    hexShape.closePath();

    const backgroundGeometry = new THREE.ShapeGeometry(hexShape);
    const backgroundMaterial = new THREE.MeshBasicMaterial({
      color: 0x0a0f14,
      transparent: true,
      opacity: 0.85,
      side: THREE.DoubleSide,
    });
    const background = new THREE.Mesh(backgroundGeometry, backgroundMaterial);
    background.position.z = -0.01;
    sigilGroup.add(background);

    // Glowing hexagonal edge
    const edgeGeometry = new THREE.RingGeometry(hexRadius - 0.03, hexRadius, 6);
    const edgeMaterial = new THREE.MeshBasicMaterial({
      color: roleColor,
      transparent: true,
      opacity: 0.9,
      side: THREE.DoubleSide,
    });
    const edge = new THREE.Mesh(edgeGeometry, edgeMaterial);
    edge.rotation.z = Math.PI / 6;
    sigilGroup.add(edge);

    // Role-specific icon shape
    const iconShape = this.createRoleIconShape(role);
    if (iconShape) {
      const iconGeometry = new THREE.ShapeGeometry(iconShape);
      const iconMaterial = new THREE.MeshBasicMaterial({
        color: roleColor,
        transparent: true,
        opacity: 0.95,
        side: THREE.DoubleSide,
      });
      const icon = new THREE.Mesh(iconGeometry, iconMaterial);
      icon.position.z = 0.01;
      sigilGroup.add(icon);
    }

    // Inner glow circle
    const innerGlowGeometry = new THREE.CircleGeometry(0.25, 32);
    const innerGlowMaterial = new THREE.MeshBasicMaterial({
      color: roleColor,
      transparent: true,
      opacity: 0.15,
      side: THREE.DoubleSide,
    });
    const innerGlow = new THREE.Mesh(innerGlowGeometry, innerGlowMaterial);
    innerGlow.position.z = -0.005;
    sigilGroup.add(innerGlow);

    // Position above player (higher to account for platform)
    sigilGroup.position.copy(player.group.position);
    sigilGroup.position.y = 2.5;

    // Mark as billboard for update loop
    sigilGroup.userData.isBillboard = true;
    sigilGroup.userData.baseY = 2.5;
    sigilGroup.userData.roleColor = roleColor;

    this.scene.add(sigilGroup);
    this.roleSigils.set(playerId, sigilGroup);

    return sigilGroup;
  }

  /**
   * Creates a role-specific icon shape for sigils
   * Each role has a distinctive geometric symbol
   */
  createRoleIconShape(role) {
    const shape = new THREE.Shape();
    const s = 0.18; // Scale factor

    switch (role) {
      case 'duke':
        // Crown shape (3 points)
        shape.moveTo(-s, -s * 0.5);
        shape.lineTo(-s, s * 0.3);
        shape.lineTo(-s * 0.5, 0);
        shape.lineTo(0, s * 0.6);
        shape.lineTo(s * 0.5, 0);
        shape.lineTo(s, s * 0.3);
        shape.lineTo(s, -s * 0.5);
        shape.closePath();
        break;

      case 'assassin':
        // Dagger/blade shape
        shape.moveTo(0, s * 0.8);
        shape.lineTo(s * 0.15, s * 0.2);
        shape.lineTo(s * 0.4, s * 0.1);
        shape.lineTo(s * 0.15, 0);
        shape.lineTo(s * 0.15, -s * 0.7);
        shape.lineTo(0, -s * 0.5);
        shape.lineTo(-s * 0.15, -s * 0.7);
        shape.lineTo(-s * 0.15, 0);
        shape.lineTo(-s * 0.4, s * 0.1);
        shape.lineTo(-s * 0.15, s * 0.2);
        shape.closePath();
        break;

      case 'captain':
        // Shield shape
        shape.moveTo(0, -s * 0.8);
        shape.quadraticCurveTo(-s * 0.7, -s * 0.3, -s * 0.7, s * 0.2);
        shape.lineTo(-s * 0.7, s * 0.5);
        shape.lineTo(0, s * 0.7);
        shape.lineTo(s * 0.7, s * 0.5);
        shape.lineTo(s * 0.7, s * 0.2);
        shape.quadraticCurveTo(s * 0.7, -s * 0.3, 0, -s * 0.8);
        break;

      case 'ambassador':
        // Star/compass shape (diplomacy)
        for (let i = 0; i < 8; i++) {
          const angle = (i / 8) * Math.PI * 2 - Math.PI / 2;
          const r = i % 2 === 0 ? s * 0.8 : s * 0.35;
          const x = Math.cos(angle) * r;
          const y = Math.sin(angle) * r;
          if (i === 0) shape.moveTo(x, y);
          else shape.lineTo(x, y);
        }
        shape.closePath();
        break;

      case 'contessa':
        // Heart/lady symbol
        shape.moveTo(0, -s * 0.6);
        shape.bezierCurveTo(-s * 0.1, -s * 0.8, -s * 0.6, -s * 0.6, -s * 0.6, -s * 0.2);
        shape.bezierCurveTo(-s * 0.6, s * 0.2, 0, s * 0.5, 0, s * 0.7);
        shape.bezierCurveTo(0, s * 0.5, s * 0.6, s * 0.2, s * 0.6, -s * 0.2);
        shape.bezierCurveTo(s * 0.6, -s * 0.6, s * 0.1, -s * 0.8, 0, -s * 0.6);
        break;

      default:
        // Generic diamond for unknown roles
        shape.moveTo(0, s * 0.6);
        shape.lineTo(s * 0.4, 0);
        shape.lineTo(0, -s * 0.6);
        shape.lineTo(-s * 0.4, 0);
        shape.closePath();
    }

    return shape;
  }

  removeRoleSigil(playerId) {
    const sigil = this.roleSigils.get(playerId);
    if (sigil) {
      this.scene.remove(sigil);
      // Dispose geometries and materials
      sigil.traverse((child) => {
        if (child.geometry) child.geometry.dispose();
        if (child.material) child.material.dispose();
      });
      this.roleSigils.delete(playerId);
    }
  }

  getRoleColor(role) {
    const colors = {
      duke: 0x9b59b6,
      assassin: 0x34495e,
      captain: 0x3498db,
      ambassador: 0x27ae60,
      contessa: 0xe74c3c,
    };
    return colors[role] || 0x7f8c8d;
  }

  initializePlayers(playerCount) {
    // Clear existing players
    for (const [, player] of this.players) {
      this.scene.remove(player.group);
      player.group.traverse((child) => {
        if (child.geometry) child.geometry.dispose();
        if (child.material) {
          if (Array.isArray(child.material)) {
            child.material.forEach((m) => m.dispose());
          } else {
            child.material.dispose();
          }
        }
      });
    }
    this.players.clear();
    this.cards.clear();
    this.coins.clear();
    // Keep only non-player holographic materials (grid materials don't have baseColor)
    this.holographicMaterials = this.holographicMaterials.filter(
      (m) => m.uniforms && !m.uniforms.baseColor
    );

    // Create new players — collect async character load promises
    const loadPromises = [];
    for (let i = 0; i < playerCount; i++) {
      loadPromises.push(this.createPlayer(i, playerCount));
    }
    this.charactersLoaded = Promise.all(loadPromises);
  }

  /**
   * Trigger a reaction pose on a player (e.g. in response to game events).
   * Auto-reverts to idle after 2.5 seconds.
   */
  triggerReaction(playerId) {
    const player = this.players.get(playerId);
    if (!player || player.eliminated || !player.humanoid || !player.idleState) return;

    setCoupCharacterPose(player.humanoid, 'reactive');
    player.idleState.reactionEndTime = this.clock.getElapsedTime() + 2.5;
    // Postpone next random pose shift so reaction plays fully
    player.idleState.nextPoseShift = player.idleState.reactionEndTime + 3 + Math.random() * 8;
  }

  /**
   * Per-frame idle behavior updates: random pose shifts, head look, breathing.
   */
  updateIdleBehaviors(elapsed, delta) {
    const POSES = ['reactive', 'speaking'];

    for (const [playerId, player] of this.players) {
      if (player.eliminated || player.dissolving || !player.humanoid || !player.idleState) continue;
      const st = player.idleState;

      // --- Event reaction auto-revert ---
      if (st.reactionEndTime > 0 && elapsed >= st.reactionEndTime) {
        setCoupCharacterPose(player.humanoid, 'idle');
        st.reactionEndTime = 0;
      }

      // --- Random pose shifts (skip if reaction is playing or this is the active player) ---
      if (st.reactionEndTime === 0 && playerId !== this.activePlayerId && elapsed >= st.nextPoseShift) {
        if (player.humanoid.userData.currentPose === 'idle') {
          // Shift to a random non-idle pose
          const pose = POSES[Math.floor(Math.random() * POSES.length)];
          setCoupCharacterPose(player.humanoid, pose);
          st.poseRevertTime = elapsed + 3 + Math.random() * 2;
        }
        st.nextPoseShift = elapsed + 8 + Math.random() * 12;
      }

      // Revert random pose shift back to idle
      if (st.poseRevertTime > 0 && elapsed >= st.poseRevertTime && st.reactionEndTime === 0) {
        setCoupCharacterPose(player.humanoid, 'idle');
        st.poseRevertTime = 0;
      }

      // --- Head look toward active player ---
      const headBone = player.humanoid.userData.headBone;
      if (headBone) {
        if (elapsed >= st.nextHeadShift) {
          // 60% chance: look toward active player, 40%: random direction
          if (Math.random() < 0.6 && playerId !== this.activePlayerId) {
            const activePlayer = this.players.get(this.activePlayerId);
            if (activePlayer) {
              const myPos = player.group.position;
              const targetPos = activePlayer.group.position;
              const dx = targetPos.x - myPos.x;
              const dz = targetPos.z - myPos.z;
              // Angle relative to player's forward direction (lookAt table center)
              const worldAngle = Math.atan2(dx, dz);
              const playerFacing = Math.atan2(-myPos.x, -myPos.z);
              let relAngle = worldAngle - playerFacing;
              // Normalize to [-PI, PI]
              while (relAngle > Math.PI) relAngle -= Math.PI * 2;
              while (relAngle < -Math.PI) relAngle += Math.PI * 2;
              // Clamp to +-0.25 rad (~15 deg)
              st.headTargetY = Math.max(-0.25, Math.min(0.25, relAngle));
            }
          } else {
            st.headTargetY = (Math.random() - 0.5) * 0.4;
          }
          st.nextHeadShift = elapsed + 4 + Math.random() * 4;
        }

        // Smooth lerp toward target
        st.headCurrentY += (st.headTargetY - st.headCurrentY) * Math.min(1, delta * 2);
        headBone.rotation.y = st.headCurrentY;
      }

      // --- Breathing (subtle Y-scale oscillation) ---
      player.humanoid.scale.y = 3.0 * (1.0 + Math.sin(elapsed * 1.2 + st.breathPhase) * 0.004);
    }
  }

  update() {
    const delta = this.clock.getDelta();
    const elapsed = this.clock.getElapsedTime();

    // Update holographic material time uniforms
    for (const material of this.holographicMaterials) {
      if (material.uniforms && material.uniforms.time) {
        material.uniforms.time.value = elapsed;
      }
    }

    // Subtle camera orbital drift — skipped when camera presets are controlling the camera
    if (!this.cameraPresetController || this.cameraPresetController.isHome) {
      this.orbitAngle += delta * 0.03;
      const orbitRadius = 0.4;
      this.camera.position.x = Math.sin(this.orbitAngle) * orbitRadius;
      this.camera.position.z = 12 + Math.cos(this.orbitAngle) * orbitRadius * 0.5;
      this.camera.lookAt(0, 0.5, 0);
    }

    // Update camera preset transitions
    if (this.cameraPresetController) {
      this.cameraPresetController.update(delta);
    }

    // Update billboards (role sigils) - face camera and float
    for (const sigil of this.roleSigils.values()) {
      if (sigil.userData.isBillboard) {
        sigil.lookAt(this.camera.position);
        // Floating animation
        const baseY = sigil.userData.baseY || 2.2;
        sigil.position.y = baseY + Math.sin(elapsed * 2.5) * 0.08;
      }
    }

    // Table glow pulse
    if (this.tableGlowMesh) {
      this.tableGlowMesh.material.opacity = 0.06 + Math.sin(elapsed * 1.5) * 0.02;
    }

    // Deck subtle pulse
    if (this.deck) {
      const scale = 1 + Math.sin(elapsed * 1.2) * 0.015;
      this.deck.scale.set(scale, 1, scale);
    }

    // Advance skeletal animations for GLB characters
    for (const [, player] of this.players) {
      if (player.eliminated || !player.mixer) continue;
      player.mixer.update(delta);
    }

    // Idle life behaviors (head turns, breathing, random pose shifts)
    this.updateIdleBehaviors(elapsed, delta);
  }

  render() {
    this.update();
    this.composer.render();
  }

  onResize() {
    const width = window.innerWidth;
    const height = window.innerHeight;
    const pixelRatio = this.renderer.getPixelRatio();

    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();

    this.renderer.setSize(width, height);
    this.composer.setSize(width, height);

    // Update bloom pass resolution
    if (this.bloomPass) {
      this.bloomPass.resolution.set(width, height);
    }
  }

  getPlayerPosition(playerId) {
    const player = this.players.get(playerId);
    if (!player) return new THREE.Vector3();
    return player.group.position.clone();
  }

  getTableCenter() {
    return new THREE.Vector3(0, 0.25, 0);
  }

  dispose() {
    window.removeEventListener('resize', this.boundOnResize);

    // Dispose ambient particles
    if (this.ambientParticles) {
      this.scene.remove(this.ambientParticles);
      this.ambientParticles.geometry.dispose();
      this.ambientParticles.material.dispose();
    }

    this.renderer.dispose();
    this.composer.dispose();

    // Dispose geometries and materials
    this.scene.traverse((object) => {
      if (object.geometry) object.geometry.dispose();
      if (object.material) {
        if (Array.isArray(object.material)) {
          object.material.forEach((m) => m.dispose());
        } else {
          object.material.dispose();
        }
      }
    });
  }
}
