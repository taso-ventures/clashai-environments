/**
 * Vibe Check Renderer
 *
 * Scene: Dark sci-fi arena (#0A1520), oval game table on pedestal,
 * spectrum gradient bar with dynamic labels, fire/ice GPU particles,
 * scoring zone rings, guess/steal markers, 6 player positions,
 * 3D HUD sprites, post-processing (bloom + vignette).
 */
import * as THREE from 'three';
import { createPostProcessing } from './shared/shared-postprocessing.js';
import { createTextSprite as sharedCreateTextSprite, updateSpriteText as sharedUpdateSpriteText } from './shared/shared-text-sprites.js';
import { buildArena as sharedBuildArena } from './shared/shared-arena.js';
import { buildChair } from './shared/shared-chair.js';

// Character system
import {
  getPlayerColor,
  createCharacter,
  setCharacterPose,
  setCharacterRole,
  updateCharacter,
  getPlayerPositions,
  POSITION_MODEL_MAP,
} from './vibe-characters.js';

// ─── Constants ───

const TEAM_A_COLOR = new THREE.Color(0x00E5CC);   // teal
const TEAM_B_COLOR = new THREE.Color(0xFF8844);    // orange
const BG_COLOR = new THREE.Color(0x0A1520);
const ACCENT_COLOR = new THREE.Color(0x00E5CC);

const SPECTRUM_LENGTH = 8.0;   // world units
const SPECTRUM_WIDTH = 1.2;
const TABLE_RADIUS_X = 5.5;
const TABLE_RADIUS_Z = 3.5;
const TABLE_HEIGHT = 0.15;
const PEDESTAL_HEIGHT = 1.2;

// Scoring zone colors
const ZONE_BULLSEYE_COLOR = new THREE.Color(0xff3333);  // red
const ZONE_NEAR_COLOR = new THREE.Color(0xffcc00);       // yellow
const ZONE_FAR_COLOR = new THREE.Color(0x44ff88);         // green

// ─── Custom Shaders ───

// Spectrum gradient shader (left = fire/warm, right = ice/cool)
const SpectrumBarShader = {
  uniforms: {
    time: { value: 0.0 },
  },
  vertexShader: `
    varying vec2 vUv;
    void main() {
      vUv = uv;
      gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
    }
  `,
  fragmentShader: `
    uniform float time;
    varying vec2 vUv;
    void main() {
      float t = vUv.x;
      // Fire (left) -> Ice (right) gradient
      vec3 fireColor = vec3(1.0, 0.3, 0.05);
      vec3 warmColor = vec3(1.0, 0.65, 0.1);
      vec3 midColor = vec3(0.8, 0.8, 0.3);
      vec3 coolColor = vec3(0.2, 0.6, 0.9);
      vec3 iceColor = vec3(0.3, 0.7, 1.0);

      vec3 color;
      if (t < 0.25) {
        color = mix(fireColor, warmColor, t / 0.25);
      } else if (t < 0.5) {
        color = mix(warmColor, midColor, (t - 0.25) / 0.25);
      } else if (t < 0.75) {
        color = mix(midColor, coolColor, (t - 0.5) / 0.25);
      } else {
        color = mix(coolColor, iceColor, (t - 0.75) / 0.25);
      }

      // Subtle shimmer
      float shimmer = sin(t * 20.0 + time * 2.0) * 0.05 + 0.95;
      color *= shimmer;

      // Edge glow (brighter at top/bottom edges)
      float edgeDist = abs(vUv.y - 0.5) * 2.0;
      float edgeGlow = smoothstep(0.6, 1.0, edgeDist) * 0.3;
      color += edgeGlow;

      gl_FragColor = vec4(color, 0.9);
    }
  `,
};


// ─── Shared GLSL: 2D Simplex Noise (Ashima) ───

const SNOISE_GLSL = `
  vec3 mod289(vec3 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
  vec2 mod289(vec2 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
  vec3 permute(vec3 x) { return mod289(((x*34.0)+1.0)*x); }
  float snoise(vec2 v) {
    const vec4 C = vec4(0.211324865405187, 0.366025403784439,
                       -0.577350269189626, 0.024390243902439);
    vec2 i = floor(v + dot(v, C.yy));
    vec2 x0 = v - i + dot(i, C.xx);
    vec2 i1 = (x0.x > x0.y) ? vec2(1.0, 0.0) : vec2(0.0, 1.0);
    vec4 x12 = x0.xyxy + C.xxzz;
    x12.xy -= i1;
    i = mod289(i);
    vec3 p = permute(permute(i.y + vec3(0.0, i1.y, 1.0)) + i.x + vec3(0.0, i1.x, 1.0));
    vec3 m = max(0.5 - vec3(dot(x0,x0), dot(x12.xy,x12.xy), dot(x12.zw,x12.zw)), 0.0);
    m = m*m; m = m*m;
    vec3 x = 2.0 * fract(p * C.www) - 1.0;
    vec3 h = abs(x) - 0.5;
    vec3 ox = floor(x + 0.5);
    vec3 a0 = x - ox;
    m *= 1.79284291400159 - 0.85373472095314 * (a0*a0 + h*h);
    vec3 g;
    g.x = a0.x * x0.x + h.x * x0.y;
    g.yz = a0.yz * x12.xz + h.yz * x12.yw;
    return 130.0 * dot(m, g);
  }
`;

// ─── Renderer Class ───

export class VibeCheckRenderer {
  constructor(canvas) {
    this.canvas = canvas;
    this.scene = new THREE.Scene();
    this.scene.background = BG_COLOR;
    this.scene.fog = new THREE.FogExp2(0x0A1520, 0.025);
    this.clock = new THREE.Clock();
    this.elapsed = 0;

    // Tracked objects for animation/updates
    this.holographicMaterials = [];
    this.playerGroups = new Map();     // playerId -> THREE.Group
    this.playerBases = new Map();      // playerId -> THREE.Mesh (base platform)
    this.characters = new Map();       // playerId -> { group, bodyMaterial, wireframeMaterial }

    // Spectrum components
    this.spectrumBar = null;
    this.spectrumBarMaterial = null;
    this.leftLabel = null;
    this.rightLabel = null;

    // Markers
    this.guessMarker = null;
    this.stealMarker = null;

    // Scoring zones
    this.scoringZoneGroup = null;

    // Fire & Ice effects
    this.fireEffect = null;
    this.iceEffect = null;

    // HUD sprites
    this.hudSprites = {};

    // Setup
    this.initRenderer();
    this.initCamera();
    this.initLighting();
    this.initPostProcessing();
    this.buildArena();
    this.buildTable();
    this.buildSpectrum();
    this.buildParticles();
    this.buildMarkers();
    this.buildScoringZones();
    this.buildHUD();
  }

  // ─── Renderer Setup ───

  initRenderer() {
    this.renderer = new THREE.WebGLRenderer({
      canvas: this.canvas,
      antialias: true,
      alpha: false,
    });
    this.renderer.setSize(window.innerWidth, window.innerHeight);
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.1;
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;

    this.boundOnResize = () => this.onResize();
    window.addEventListener('resize', this.boundOnResize);
  }

  initCamera() {
    const aspect = window.innerWidth / window.innerHeight;
    this.camera = new THREE.PerspectiveCamera(45, aspect, 0.1, 100);
    // Lower 3/4 view, zoomed out so all characters visible
    this.camera.position.set(12, 10, 16);
    this.camera.lookAt(0, 1, 0);
    this.orbitAngle = 0;
  }

  initLighting() {
    // Hemisphere
    const hemiLight = new THREE.HemisphereLight(0x6a8aaa, 0x2a3840, 1.0);
    this.scene.add(hemiLight);

    // Ambient
    const ambientLight = new THREE.AmbientLight(0x4a6688, 0.5);
    this.scene.add(ambientLight);

    // Key spot
    const keyLight = new THREE.SpotLight(0xffffff, 2.0);
    keyLight.position.set(0, 15, 5);
    keyLight.angle = Math.PI / 3.5;
    keyLight.penumbra = 0.7;
    keyLight.castShadow = true;
    keyLight.shadow.mapSize.set(1024, 1024);
    this.scene.add(keyLight);

    // Fill
    const fillLight = new THREE.SpotLight(0x4488ff, 0.8);
    fillLight.position.set(-8, 8, -5);
    fillLight.penumbra = 0.9;
    this.scene.add(fillLight);

    // Back fill
    const backFill = new THREE.SpotLight(0x336688, 0.6);
    backFill.position.set(5, 10, -8);
    backFill.penumbra = 0.9;
    this.scene.add(backFill);

    // Rim lights around table perimeter (teal accent)
    for (let i = 0; i < 6; i++) {
      const angle = (i / 6) * Math.PI * 2;
      const rimLight = new THREE.PointLight(0x00E5CC, 0.6, 10);
      rimLight.position.set(
        Math.cos(angle) * TABLE_RADIUS_X * 0.8,
        0.5,
        Math.sin(angle) * TABLE_RADIUS_Z * 0.8
      );
      this.scene.add(rimLight);
    }

    // Table center glow
    const centerLight = new THREE.PointLight(0x00E5CC, 0.4, 6);
    centerLight.position.set(0, 0.5, 0);
    this.scene.add(centerLight);
  }

  initPostProcessing() {
    const { composer } = createPostProcessing(
      this.renderer, this.scene, this.camera, {
        bloom: { strength: 0.5, radius: 0.5, threshold: 0.35 },
        vignette: { darkness: 0.85 },
      }
    );
    this.composer = composer;
  }

  onResize() {
    const w = window.innerWidth;
    const h = window.innerHeight;
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(w, h);
    this.composer.setSize(w, h);
  }

  // ─── Arena ───

  buildArena() {
    const { materials } = sharedBuildArena(this.scene);
    this.holographicMaterials.push(...materials);
  }

  // ─── Table ───

  buildTable() {
    const tableGroup = new THREE.Group();

    // Pedestal (cylinder)
    const pedestalGeo = new THREE.CylinderGeometry(1.5, 2.0, PEDESTAL_HEIGHT, 16);
    const pedestalMat = new THREE.MeshStandardMaterial({
      color: 0x1a2a35,
      metalness: 0.95,
      roughness: 0.15,
    });
    const pedestal = new THREE.Mesh(pedestalGeo, pedestalMat);
    pedestal.position.y = PEDESTAL_HEIGHT / 2;
    pedestal.castShadow = true;
    tableGroup.add(pedestal);

    // Pedestal glow ring
    const pedestalRingGeo = new THREE.TorusGeometry(1.8, 0.03, 8, 32);
    const pedestalRingMat = new THREE.MeshBasicMaterial({
      color: ACCENT_COLOR,
      transparent: true,
      opacity: 0.6,
    });
    const pedestalRing = new THREE.Mesh(pedestalRingGeo, pedestalRingMat);
    pedestalRing.rotation.x = Math.PI / 2;
    pedestalRing.position.y = PEDESTAL_HEIGHT;
    tableGroup.add(pedestalRing);

    // Oval table top (ellipse via scaled circle)
    const tableGeo = new THREE.CylinderGeometry(1, 1, TABLE_HEIGHT, 64);
    tableGeo.scale(TABLE_RADIUS_X, 1, TABLE_RADIUS_Z);
    const tableMat = new THREE.MeshStandardMaterial({
      color: 0x0D1A24,
      metalness: 0.85,
      roughness: 0.2,
      envMapIntensity: 0.5,
    });
    const table = new THREE.Mesh(tableGeo, tableMat);
    table.position.y = PEDESTAL_HEIGHT + TABLE_HEIGHT / 2;
    table.receiveShadow = true;
    table.castShadow = true;
    tableGroup.add(table);

    // Teal edge glow
    const edgeGeo = new THREE.TorusGeometry(1, 0.02, 8, 64);
    edgeGeo.scale(TABLE_RADIUS_X, 1, TABLE_RADIUS_Z);
    const edgeMat = new THREE.MeshBasicMaterial({
      color: ACCENT_COLOR,
      transparent: true,
      opacity: 0.8,
    });
    const edge = new THREE.Mesh(edgeGeo, edgeMat);
    edge.rotation.x = Math.PI / 2;
    edge.position.y = PEDESTAL_HEIGHT + TABLE_HEIGHT;
    tableGroup.add(edge);

    // Inner accent ring
    const innerRingGeo = new THREE.TorusGeometry(1, 0.015, 8, 64);
    innerRingGeo.scale(TABLE_RADIUS_X * 0.85, 1, TABLE_RADIUS_Z * 0.85);
    const innerRingMat = new THREE.MeshBasicMaterial({
      color: ACCENT_COLOR,
      transparent: true,
      opacity: 0.3,
    });
    const innerRing = new THREE.Mesh(innerRingGeo, innerRingMat);
    innerRing.rotation.x = Math.PI / 2;
    innerRing.position.y = PEDESTAL_HEIGHT + TABLE_HEIGHT + 0.01;
    tableGroup.add(innerRing);

    // Surface glow
    const surfaceGlowGeo = new THREE.CircleGeometry(1, 64);
    surfaceGlowGeo.scale(TABLE_RADIUS_X * 0.7, TABLE_RADIUS_Z * 0.7, 1);
    const surfaceGlowMat = new THREE.MeshBasicMaterial({
      color: ACCENT_COLOR,
      transparent: true,
      opacity: 0.06,
    });
    const surfaceGlow = new THREE.Mesh(surfaceGlowGeo, surfaceGlowMat);
    surfaceGlow.rotation.x = -Math.PI / 2;
    surfaceGlow.position.y = PEDESTAL_HEIGHT + TABLE_HEIGHT + 0.02;
    tableGroup.add(surfaceGlow);
    this.tableGlowMesh = surfaceGlow;

    this.scene.add(tableGroup);
    this.tableGroup = tableGroup;
    this.tableTopY = PEDESTAL_HEIGHT + TABLE_HEIGHT;
  }

  // ─── Spectrum Bar ───

  buildSpectrum() {
    const spectrumGroup = new THREE.Group();
    spectrumGroup.position.y = this.tableTopY + 0.03;

    // Gradient bar
    const barGeo = new THREE.PlaneGeometry(SPECTRUM_LENGTH, SPECTRUM_WIDTH);
    this.spectrumBarMaterial = new THREE.ShaderMaterial({
      ...SpectrumBarShader,
      uniforms: { time: { value: 0 } },
      transparent: true,
      depthWrite: false,
    });
    this.spectrumBar = new THREE.Mesh(barGeo, this.spectrumBarMaterial);
    this.spectrumBar.rotation.x = -Math.PI / 2;
    spectrumGroup.add(this.spectrumBar);

    // Edge frame around spectrum
    const frameGeo = new THREE.EdgesGeometry(barGeo);
    const frameMat = new THREE.LineBasicMaterial({
      color: 0xffffff,
      transparent: true,
      opacity: 0.6,
    });
    const frame = new THREE.LineSegments(frameGeo, frameMat);
    frame.rotation.x = -Math.PI / 2;
    frame.position.y = 0.01;
    spectrumGroup.add(frame);

    // Tick marks along spectrum (every 0.1 from 0 to 1)
    for (let i = 0; i <= 10; i++) {
      const t = i / 10;
      const x = (t - 0.5) * SPECTRUM_LENGTH;
      const isMajor = (i === 0 || i === 5 || i === 10);
      const tickHeight = isMajor ? 0.2 : 0.1;
      const tickGeo = new THREE.PlaneGeometry(0.02, tickHeight);
      const tickMat = new THREE.MeshBasicMaterial({
        color: 0xffffff,
        transparent: true,
        opacity: isMajor ? 0.7 : 0.3,
        side: THREE.DoubleSide,
      });
      const tick = new THREE.Mesh(tickGeo, tickMat);
      tick.position.set(x, 0.02, SPECTRUM_WIDTH / 2 + tickHeight / 2 + 0.02);
      tick.rotation.x = -Math.PI / 2;
      spectrumGroup.add(tick);
    }

    this.spectrumGroup = spectrumGroup;
    this.scene.add(spectrumGroup);

    // Create label sprites (updated dynamically when RoundStarted arrives)
    this.leftLabel = this.createSpectrumLabel('HOT', 0xffffff);
    this.leftLabel.position.set(-SPECTRUM_LENGTH / 2 - 0.8, 5.0, 0);
    this.scene.add(this.leftLabel);

    this.rightLabel = this.createSpectrumLabel('COLD', 0xffffff);
    this.rightLabel.position.set(SPECTRUM_LENGTH / 2 + 0.8, 5.0, 0);
    this.scene.add(this.rightLabel);
  }

  // ─── Scoring Zones (hidden initially) ───

  buildScoringZones() {
    this.scoringZoneGroup = new THREE.Group();
    this.scoringZoneGroup.position.y = this.tableTopY + 0.02;
    this.scoringZoneGroup.visible = false;
    this.scene.add(this.scoringZoneGroup);
  }

  /**
   * Show scoring zones at the revealed target position.
   * @param {number} targetPos — normalized [0,1]
   * @param {object} zoneConfig — { bullseye_half_width, near_half_width, far_half_width }
   */
  showScoringZones(targetPos, zoneConfig) {
    // Clear existing zones
    while (this.scoringZoneGroup.children.length > 0) {
      const child = this.scoringZoneGroup.children[0];
      child.geometry?.dispose();
      child.material?.dispose();
      this.scoringZoneGroup.remove(child);
    }

    const targetX = (targetPos - 0.5) * SPECTRUM_LENGTH;
    const bHW = zoneConfig.bullseye_half_width * SPECTRUM_LENGTH;
    const nHW = zoneConfig.near_half_width * SPECTRUM_LENGTH;
    const fHW = zoneConfig.far_half_width * SPECTRUM_LENGTH;

    // Far zone (outermost) — green
    this.addZoneRing(targetX, fHW, ZONE_FAR_COLOR, 0.2);
    // Near zone — yellow
    this.addZoneRing(targetX, nHW, ZONE_NEAR_COLOR, 0.3);
    // Bullseye (innermost) — red
    this.addZoneRing(targetX, bHW, ZONE_BULLSEYE_COLOR, 0.45);

    // Gold target beam (tall vertical pillar at exact target position)
    const targetBeamHeight = 2.0;
    const targetBeamGeo = new THREE.CylinderGeometry(0.04, 0.04, targetBeamHeight, 8);
    const targetBeamMat = new THREE.MeshBasicMaterial({
      color: 0xffd700,
      transparent: true,
      opacity: 0.9,
    });
    const targetBeam = new THREE.Mesh(targetBeamGeo, targetBeamMat);
    targetBeam.position.set(targetX, targetBeamHeight / 2, 0);
    this.scoringZoneGroup.add(targetBeam);

    // Gold beam outer glow
    const targetGlowGeo = new THREE.CylinderGeometry(0.15, 0.15, targetBeamHeight, 8);
    const targetGlowMat = new THREE.MeshBasicMaterial({
      color: 0xffd700,
      transparent: true,
      opacity: 0.15,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    const targetGlow = new THREE.Mesh(targetGlowGeo, targetGlowMat);
    targetGlow.position.set(targetX, targetBeamHeight / 2, 0);
    this.scoringZoneGroup.add(targetGlow);

    // Gold point light at target
    const targetLight = new THREE.PointLight(0xffd700, 1.5, 4);
    targetLight.position.set(targetX, 1.0, 0);
    this.scoringZoneGroup.add(targetLight);

    this.scoringZoneGroup.visible = true;
  }

  addZoneRing(centerX, halfWidth, color, opacity) {
    const width = halfWidth * 2;
    const zoneDepth = SPECTRUM_WIDTH + 0.6;

    // Zone fill
    const geo = new THREE.PlaneGeometry(width, zoneDepth);
    const mat = new THREE.MeshBasicMaterial({
      color: color,
      transparent: true,
      opacity: opacity,
      side: THREE.DoubleSide,
      depthWrite: false,
    });
    const mesh = new THREE.Mesh(geo, mat);
    mesh.rotation.x = -Math.PI / 2;
    mesh.position.set(centerX, 0.01, 0);
    this.scoringZoneGroup.add(mesh);

    // Bright edge lines (thicker)
    const edgeMat = new THREE.MeshBasicMaterial({
      color: color,
      transparent: true,
      opacity: Math.min(opacity + 0.35, 0.9),
      side: THREE.DoubleSide,
    });
    for (const dx of [-halfWidth, halfWidth]) {
      const lineGeo = new THREE.PlaneGeometry(0.04, zoneDepth);
      const line = new THREE.Mesh(lineGeo, edgeMat.clone());
      line.rotation.x = -Math.PI / 2;
      line.position.set(centerX + dx, 0.02, 0);
      this.scoringZoneGroup.add(line);

      // Vertical glow beam at each zone edge
      const edgeBeamGeo = new THREE.CylinderGeometry(0.02, 0.02, 1.0, 6);
      const edgeBeamMat = new THREE.MeshBasicMaterial({
        color: color,
        transparent: true,
        opacity: 0.4,
      });
      const edgeBeam = new THREE.Mesh(edgeBeamGeo, edgeBeamMat);
      edgeBeam.position.set(centerX + dx, 0.5, 0);
      this.scoringZoneGroup.add(edgeBeam);
    }
  }

  hideScoringZones() {
    this.scoringZoneGroup.visible = false;
  }

  // ─── Fire & Ice Effects (shader-based) ───

  buildParticles() {
    this.fireEffect = this.createFireEffect();
    this.fireEffect.position.set(-SPECTRUM_LENGTH / 2 - 0.5, this.tableTopY, 0);
    this.scene.add(this.fireEffect);

    this.iceEffect = this.createIceEffect();
    this.iceEffect.position.set(SPECTRUM_LENGTH / 2 + 0.5, this.tableTopY, 0);
    this.scene.add(this.iceEffect);
  }

  createFireEffect() {
    const group = new THREE.Group();

    const fireFrag = `
      ${SNOISE_GLSL}
      uniform float time;
      varying vec2 vUv;

      void main() {
        vec2 uv = vUv;

        // Flame shape: wide at base, narrow at top
        float flameWidth = mix(0.45, 0.05, pow(uv.y, 0.8));
        float shape = smoothstep(flameWidth, flameWidth - 0.12, abs(uv.x - 0.5));

        // Multi-octave noise for turbulence
        float n1 = snoise(vec2(uv.x * 4.0, uv.y * 3.0 - time * 2.0)) * 0.5 + 0.5;
        float n2 = snoise(vec2(uv.x * 8.0, uv.y * 6.0 - time * 3.5)) * 0.5 + 0.5;
        float n3 = snoise(vec2(uv.x * 12.0 + 3.0, uv.y * 10.0 - time * 4.0)) * 0.5 + 0.5;
        float noise = n1 * 0.5 + n2 * 0.3 + n3 * 0.2;

        // Height fade
        float heightFade = 1.0 - smoothstep(0.2, 0.95, uv.y);

        // Combined flame intensity
        float flame = shape * heightFade * noise;
        flame = smoothstep(0.05, 0.7, flame);

        // Color gradient: dark red -> orange -> yellow -> white-hot core
        vec3 hotCore = vec3(1.0, 0.95, 0.6);
        vec3 midFlame = vec3(1.0, 0.45, 0.05);
        vec3 outerFlame = vec3(0.6, 0.1, 0.0);
        vec3 darkBase = vec3(0.3, 0.02, 0.0);

        vec3 color;
        if (flame > 0.7) {
          color = mix(midFlame, hotCore, (flame - 0.7) / 0.3);
        } else if (flame > 0.4) {
          color = mix(outerFlame, midFlame, (flame - 0.4) / 0.3);
        } else if (flame > 0.15) {
          color = mix(darkBase, outerFlame, (flame - 0.15) / 0.25);
        } else {
          color = darkBase * (flame / 0.15);
        }

        gl_FragColor = vec4(color, flame * 0.65);
      }
    `;

    const fireVert = `
      varying vec2 vUv;
      void main() {
        vUv = uv;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
      }
    `;

    // Two crossed planes for pseudo-volumetric appearance
    for (let i = 0; i < 2; i++) {
      const geo = new THREE.PlaneGeometry(1.1, 1.8);
      const mat = new THREE.ShaderMaterial({
        uniforms: { time: { value: 0 } },
        vertexShader: fireVert,
        fragmentShader: fireFrag,
        transparent: true,
        depthWrite: false,
        side: THREE.DoubleSide,
        blending: THREE.AdditiveBlending,
      });
      const mesh = new THREE.Mesh(geo, mat);
      mesh.rotation.y = (i * Math.PI) / 2;
      mesh.position.y = 0.9;
      group.add(mesh);
      this.holographicMaterials.push(mat);
    }

    return group;
  }

  createIceEffect() {
    const group = new THREE.Group();

    const iceFrag = `
      ${SNOISE_GLSL}
      uniform float time;
      varying vec2 vUv;

      void main() {
        vec2 uv = vUv;

        // Crystal column shape: slightly narrower at top
        float colWidth = mix(0.4, 0.25, uv.y);
        float shape = smoothstep(colWidth, colWidth - 0.1, abs(uv.x - 0.5));

        // Frost noise patterns (slow, crystalline)
        float n1 = snoise(vec2(uv.x * 5.0 + time * 0.3, uv.y * 4.0)) * 0.5 + 0.5;
        float n2 = snoise(vec2(uv.x * 10.0, uv.y * 8.0 - time * 0.2)) * 0.5 + 0.5;
        float frost = n1 * 0.6 + n2 * 0.4;

        // Crystal sparkle (sharp bright points)
        float sparkle = snoise(vec2(uv.x * 20.0 + time * 1.5, uv.y * 20.0 - time * 0.8));
        sparkle = smoothstep(0.7, 0.9, sparkle) * 0.4;

        // Height fade (less aggressive than fire)
        float heightFade = 1.0 - smoothstep(0.5, 1.0, uv.y);

        // Combined ice intensity
        float ice = shape * frost * heightFade;
        ice = smoothstep(0.05, 0.6, ice);

        // Color gradient: deep blue -> cyan -> pale frost (moderate to avoid bloom blowout)
        vec3 deepBlue = vec3(0.03, 0.06, 0.18);
        vec3 midCyan = vec3(0.1, 0.25, 0.45);
        vec3 frostWhite = vec3(0.3, 0.5, 0.65);

        vec3 color;
        if (ice > 0.6) {
          color = mix(midCyan, frostWhite, (ice - 0.6) / 0.4);
        } else if (ice > 0.25) {
          color = mix(deepBlue, midCyan, (ice - 0.25) / 0.35);
        } else {
          color = deepBlue * (ice / 0.25);
        }

        // Add sparkle highlights (subtle)
        color += vec3(0.25, 0.35, 0.5) * sparkle * ice;

        gl_FragColor = vec4(color, ice * 0.45);
      }
    `;

    const iceVert = `
      varying vec2 vUv;
      void main() {
        vUv = uv;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
      }
    `;

    // Two crossed planes for pseudo-volumetric appearance
    for (let i = 0; i < 2; i++) {
      const geo = new THREE.PlaneGeometry(1.0, 1.4);
      const mat = new THREE.ShaderMaterial({
        uniforms: { time: { value: 0 } },
        vertexShader: iceVert,
        fragmentShader: iceFrag,
        transparent: true,
        depthWrite: false,
        side: THREE.DoubleSide,
        blending: THREE.NormalBlending,
      });
      const mesh = new THREE.Mesh(geo, mat);
      mesh.rotation.y = (i * Math.PI) / 2;
      mesh.position.y = 0.7;
      group.add(mesh);
      this.holographicMaterials.push(mat);
    }

    return group;
  }

  // ─── Guess & Steal Markers ───

  buildMarkers() {
    // Guess marker (teal beam pillar)
    this.guessMarker = this.createBeamMarker(TEAM_A_COLOR, 'GUESS');
    this.guessMarker.visible = false;
    this.scene.add(this.guessMarker);

    // Steal marker (orange directional beam)
    this.stealMarker = this.createStealDirectionMarker(TEAM_B_COLOR);
    this.stealMarker.visible = false;
    this.scene.add(this.stealMarker);
  }

  createBeamMarker(color, label) {
    const group = new THREE.Group();
    const beamHeight = 3.0;

    // Vertical light beam (glowing pillar)
    const beamGeo = new THREE.CylinderGeometry(0.06, 0.06, beamHeight, 12);
    const beamMat = new THREE.MeshBasicMaterial({
      color: color,
      transparent: true,
      opacity: 0.8,
    });
    const beam = new THREE.Mesh(beamGeo, beamMat);
    beam.position.y = beamHeight / 2;
    group.add(beam);

    // Outer glow cylinder (wider, more transparent)
    const glowGeo = new THREE.CylinderGeometry(0.2, 0.2, beamHeight, 12);
    const glowMat = new THREE.MeshBasicMaterial({
      color: color,
      transparent: true,
      opacity: 0.15,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    const glow = new THREE.Mesh(glowGeo, glowMat);
    glow.position.y = beamHeight / 2;
    group.add(glow);

    // Downward-pointing cone at bottom
    const coneGeo = new THREE.ConeGeometry(0.25, 0.5, 12);
    const coneMat = new THREE.MeshBasicMaterial({ color: color });
    const cone = new THREE.Mesh(coneGeo, coneMat);
    cone.rotation.x = Math.PI;
    cone.position.y = 0.25;
    group.add(cone);

    // Base ring on spectrum surface
    const ringGeo = new THREE.RingGeometry(0.2, 0.4, 24);
    const ringMat = new THREE.MeshBasicMaterial({
      color: color,
      transparent: true,
      opacity: 0.7,
      side: THREE.DoubleSide,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    const ring = new THREE.Mesh(ringGeo, ringMat);
    ring.rotation.x = -Math.PI / 2;
    ring.position.y = 0.02;
    group.add(ring);

    // Ground glow disc
    const discGeo = new THREE.CircleGeometry(0.6, 24);
    const discMat = new THREE.MeshBasicMaterial({
      color: color,
      transparent: true,
      opacity: 0.25,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    const disc = new THREE.Mesh(discGeo, discMat);
    disc.rotation.x = -Math.PI / 2;
    disc.position.y = 0.01;
    group.add(disc);

    // Point light for scene illumination
    const light = new THREE.PointLight(color.getHex(), 1.5, 4);
    light.position.y = 1.5;
    group.add(light);

    // Label sprite (larger)
    const sprite = this.createTextSprite(label, color.getHex(), 0.5);
    sprite.position.y = beamHeight + 0.5;
    group.add(sprite);

    return group;
  }

  createStealDirectionMarker(color) {
    const group = new THREE.Group();
    const beamHeight = 2.5;

    // Vertical beam (shorter than guess)
    const beamGeo = new THREE.CylinderGeometry(0.05, 0.05, beamHeight, 12);
    const beamMat = new THREE.MeshBasicMaterial({
      color: color,
      transparent: true,
      opacity: 0.7,
    });
    const beam = new THREE.Mesh(beamGeo, beamMat);
    beam.position.y = beamHeight / 2;
    group.add(beam);

    // Outer glow
    const glowGeo = new THREE.CylinderGeometry(0.15, 0.15, beamHeight, 12);
    const glowMat = new THREE.MeshBasicMaterial({
      color: color,
      transparent: true,
      opacity: 0.12,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    const glow = new THREE.Mesh(glowGeo, glowMat);
    glow.position.y = beamHeight / 2;
    group.add(glow);

    // Horizontal direction arrow (cone + shaft) at mid-height
    const coneGeo = new THREE.ConeGeometry(0.25, 0.6, 8);
    const coneMat = new THREE.MeshBasicMaterial({ color: color });
    const cone = new THREE.Mesh(coneGeo, coneMat);
    cone.rotation.z = -Math.PI / 2; // default: point right
    cone.position.set(0.5, 1.2, 0);
    group.add(cone);
    group.userData.cone = cone;

    const shaftGeo = new THREE.CylinderGeometry(0.06, 0.06, 0.7, 8);
    const shaftMat = new THREE.MeshBasicMaterial({ color: color });
    const shaft = new THREE.Mesh(shaftGeo, shaftMat);
    shaft.rotation.z = Math.PI / 2;
    shaft.position.set(0, 1.2, 0);
    group.add(shaft);
    group.userData.shaft = shaft;

    // Base ring
    const ringGeo = new THREE.RingGeometry(0.15, 0.3, 24);
    const ringMat = new THREE.MeshBasicMaterial({
      color: color,
      transparent: true,
      opacity: 0.6,
      side: THREE.DoubleSide,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    const ring = new THREE.Mesh(ringGeo, ringMat);
    ring.rotation.x = -Math.PI / 2;
    ring.position.y = 0.02;
    group.add(ring);

    // Point light
    const light = new THREE.PointLight(color.getHex(), 1.0, 3);
    light.position.y = 1.2;
    group.add(light);

    // Label sprite
    const sprite = this.createTextSprite('STEAL', color.getHex(), 0.5);
    sprite.position.y = beamHeight + 0.4;
    group.add(sprite);

    return group;
  }

  /**
   * Position the guess marker on the spectrum.
   * @param {number} position — normalized [0,1]
   */
  setGuessMarkerPosition(position) {
    const x = (position - 0.5) * SPECTRUM_LENGTH;
    this.guessMarker.position.set(x, this.tableTopY, 0);
    this.guessMarker.visible = true;
  }

  /**
   * Show the steal direction marker.
   * @param {string} direction — "left" or "right"
   * @param {number} guessPosition — the active team's guess position [0,1]
   */
  setStealMarker(direction, guessPosition) {
    const guessX = (guessPosition - 0.5) * SPECTRUM_LENGTH;
    this.stealMarker.position.set(guessX, this.tableTopY, 0);

    // Flip arrow direction
    const cone = this.stealMarker.userData.cone;
    const shaft = this.stealMarker.userData.shaft;
    if (direction === 'left') {
      cone.rotation.z = Math.PI / 2;
      cone.position.x = -0.5;
      shaft.position.x = 0;
    } else {
      cone.rotation.z = -Math.PI / 2;
      cone.position.x = 0.5;
      shaft.position.x = 0;
    }

    this.stealMarker.visible = true;
  }

  hideMarkers() {
    this.guessMarker.visible = false;
    this.stealMarker.visible = false;
  }

  // ─── Player Positions + Characters ───

  /**
   * Set up player positions and characters based on actual game data.
   * Call this from game_started with the real player/team arrays.
   *
   * @param {Array<{player_id: number, team: number}>} players
   * @param {Array<{team_id: number}>} teams
   */
  setupPlayers(players, teams) {
    // Clear any existing player groups (in case of re-init)
    for (const [, group] of this.playerGroups) {
      this.scene.remove(group);
    }
    this.playerGroups.clear();
    this.playerBases.clear();
    this.characters.clear();

    // Count players per team
    const teamACount = players.filter(p => p.team === 0).length;
    const teamBCount = players.filter(p => p.team === 1).length;

    const positions = getPlayerPositions(TABLE_RADIUS_X, TABLE_RADIUS_Z, teamACount, teamBCount);

    // Sort players: team 0 first, then team 1 (preserving order within team)
    const sorted = [
      ...players.filter(p => p.team === 0),
      ...players.filter(p => p.team === 1),
    ];

    const loadPromises = [];

    sorted.forEach((player, index) => {
      const pos = positions[index];
      if (!pos) return;
      const teamColor = pos.teamSide === 'A' ? TEAM_A_COLOR : TEAM_B_COLOR;
      const playerColor = getPlayerColor(index);
      const group = this.createPlayerBase(player.player_id, teamColor, playerColor);
      group.position.set(pos.x, 0, pos.z);
      group.lookAt(0, 0, 0);
      this.scene.add(group);
      this.playerGroups.set(player.player_id, group);

      loadPromises.push(this.loadCharacterAsync(player.player_id, group, playerColor));
    });

    this.charactersLoaded = Promise.all(loadPromises);
    console.log(`[VibeRenderer] Set up ${sorted.length} players (${teamACount}v${teamBCount})`);
  }

  /**
   * Create the non-character elements for a player position (chair, light).
   * Characters are loaded asynchronously via loadCharacterAsync.
   */
  createPlayerBase(playerId, teamColor, playerColor) {
    const chair = buildChair(teamColor);

    // Point light per player (uses unique player color)
    const light = new THREE.PointLight(playerColor, 0.8, 5);
    light.position.y = 2.0;
    chair.add(light);

    return chair;
  }

  /**
   * Load a character model asynchronously and attach it to the player group.
   */
  async loadCharacterAsync(playerId, group, playerColor) {
    const modelIndex = POSITION_MODEL_MAP[playerId] ?? playerId;
    try {
      const character = await createCharacter(modelIndex, playerColor);
      character.group.scale.set(3.0, 3.0, 3.0);
      character.group.rotation.y = Math.PI; // glTF faces +Z, rotate to face table center
      character.group.position.y = 0.73; // sit on chair seat surface
      group.add(character.group);

      // Track character materials for time updates
      this.holographicMaterials.push(character.bodyMaterial);
      this.holographicMaterials.push(character.wireframeMaterial);

      // Store character reference
      this.characters.set(playerId, character);
    } catch (err) {
      console.warn(`Failed to load character ${modelIndex} for player ${playerId}:`, err);
    }
  }

  getPlayerPosition(playerId) {
    const group = this.playerGroups.get(playerId);
    if (!group) return new THREE.Vector3();
    return group.position.clone();
  }

  // ─── Character State Methods ───

  /**
   * Set a player's character pose.
   * @param {number} playerId
   * @param {string} pose — 'idle' | 'speaking' | 'reactive'
   */
  setPlayerPose(playerId, pose) {
    const char = this.characters.get(playerId);
    if (char) setCharacterPose(char.group, pose);
  }

  /**
   * Set a player's role badge.
   * @param {number} playerId
   * @param {string|null} role — 'CLUEGIVER' | 'GUESSER' | 'STEAL TEAM' | null
   */
  setPlayerRole(playerId, role) {
    const char = this.characters.get(playerId);
    if (char) setCharacterRole(char.group, role);
  }

  /**
   * Reset all player poses and roles.
   */
  resetAllPlayerStates() {
    for (const [id, char] of this.characters) {
      setCharacterPose(char.group, 'idle');
      setCharacterRole(char.group, null);
    }
  }

  // ─── 3D HUD Sprites (in-scene only: clue + score popup) ───

  buildHUD() {
    // Clue display (large, above spectrum)
    this.hudSprites.clue = this.createTextSprite('', 0xffffff, 1.2);
    this.hudSprites.clue.position.set(0, this.tableTopY + 2.0, 0);
    this.hudSprites.clue.visible = false;
    this.scene.add(this.hudSprites.clue);

    // Score popup (big, shown during target reveal)
    this.hudSprites.scorePopup = this.createTextSprite('', 0xffd700, 1.4);
    this.hudSprites.scorePopup.position.set(0, this.tableTopY + 3.5, 0);
    this.hudSprites.scorePopup.visible = false;
    this.scene.add(this.hudSprites.scorePopup);
  }

  /**
   * Create a text sprite for 3D HUD elements.
   * Delegates to shared text sprite factory.
   */
  createTextSprite(text, color, scale) {
    return sharedCreateTextSprite(text, color, scale);
  }

  /**
   * Create a large, prominent spectrum endpoint label with drop shadow.
   */
  createSpectrumLabel(text, color) {
    const canvas = document.createElement('canvas');
    const ctx = canvas.getContext('2d');
    canvas.width = 512;
    canvas.height = 128;

    this.drawSpectrumLabelToCanvas(ctx, text, color, canvas.width, canvas.height);

    const texture = new THREE.CanvasTexture(canvas);
    texture.minFilter = THREE.LinearFilter;

    const material = new THREE.SpriteMaterial({
      map: texture,
      transparent: true,
      depthTest: false,
    });

    const scale = 1.4;
    const sprite = new THREE.Sprite(material);
    sprite.scale.set(4 * scale, 1 * scale, 1);
    sprite.userData.canvas = canvas;
    sprite.userData.ctx = ctx;
    sprite.userData.texture = texture;
    sprite.userData.color = color;
    sprite.userData.isSpectrumLabel = true;

    return sprite;
  }

  drawSpectrumLabelToCanvas(ctx, text, colorHex, width, height) {
    ctx.clearRect(0, 0, width, height);

    ctx.font = 'bold 56px "Geist Sans", sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';

    // Heavy black drop shadow (multiple passes for thickness)
    ctx.shadowColor = 'rgba(0, 0, 0, 1)';
    ctx.shadowOffsetX = 0;
    ctx.shadowOffsetY = 0;
    ctx.fillStyle = 'rgba(0,0,0,0)';
    for (const blur of [24, 16, 8]) {
      ctx.shadowBlur = blur;
      ctx.fillText(text, width / 2, height / 2);
    }

    // White text on top
    ctx.shadowColor = 'rgba(0, 0, 0, 0.8)';
    ctx.shadowBlur = 4;
    ctx.shadowOffsetX = 1;
    ctx.shadowOffsetY = 2;
    ctx.fillStyle = '#ffffff';
    ctx.fillText(text, width / 2, height / 2);

    ctx.shadowBlur = 0;
    ctx.shadowOffsetX = 0;
    ctx.shadowOffsetY = 0;
  }

  /**
   * Update text on a sprite. Delegates to shared text sprite updater.
   */
  updateSpriteText(sprite, text, color) {
    sharedUpdateSpriteText(sprite, text, color);
  }

  // ─── HUD Update Methods ───

  updateSpectrumLabels(leftEndpoint, rightEndpoint) {
    this.updateSpectrumLabelText(this.leftLabel, leftEndpoint.toUpperCase(), 0xffffff);
    this.updateSpectrumLabelText(this.rightLabel, rightEndpoint.toUpperCase(), 0xffffff);
  }

  updateSpectrumLabelText(sprite, text, color) {
    if (!sprite?.userData?.ctx) return;
    const { ctx, canvas, texture } = sprite.userData;
    const c = color !== undefined ? color : sprite.userData.color;
    this.drawSpectrumLabelToCanvas(ctx, text, c, canvas.width, canvas.height);
    texture.needsUpdate = true;
    sprite.userData.color = c;
  }

  showClue(clueText) {
    this.updateSpriteText(this.hudSprites.clue, `"${clueText}"`, 0xffffff);
    this.hudSprites.clue.visible = true;
  }

  hideClue() {
    this.hudSprites.clue.visible = false;
  }

  showScorePopup(text, color) {
    this.updateSpriteText(this.hudSprites.scorePopup, text, color || 0xffd700);
    this.hudSprites.scorePopup.visible = true;
  }

  hideScorePopup() {
    this.hudSprites.scorePopup.visible = false;
  }

  // ─── Per-Frame Update & Render ───

  update(delta) {
    this.elapsed += delta;

    // Update all holographic/particle materials
    for (const mat of this.holographicMaterials) {
      if (mat.uniforms?.time) {
        mat.uniforms.time.value = this.elapsed;
      }
    }

    // Spectrum bar shimmer
    if (this.spectrumBarMaterial?.uniforms?.time) {
      this.spectrumBarMaterial.uniforms.time.value = this.elapsed;
    }

    // Camera subtle drift — skipped when camera presets are controlling the camera
    if (!this.cameraPresetController || this.cameraPresetController.isHome) {
      this.orbitAngle += delta * 0.03;
      this.camera.position.x = 12 + Math.sin(this.orbitAngle) * 0.4;
      this.camera.position.z = 16 + Math.cos(this.orbitAngle * 0.7) * 0.3;
      this.camera.lookAt(0, 1, 0);
    }

    // Update camera preset transitions
    if (this.cameraPresetController) {
      this.cameraPresetController.update(delta);
    }

    // Table glow pulse
    if (this.tableGlowMesh) {
      this.tableGlowMesh.material.opacity = 0.06 + Math.sin(this.elapsed * 1.5) * 0.02;
    }

    // Character animations (skeletal animation, material updates, badge)
    for (const [playerId, char] of this.characters) {
      updateCharacter(char.group, this.elapsed, playerId, delta);
    }

    // Guess marker bob
    if (this.guessMarker.visible) {
      this.guessMarker.position.y = this.tableTopY + Math.sin(this.elapsed * 3) * 0.05;
    }

    // Steal marker bob
    if (this.stealMarker.visible) {
      this.stealMarker.position.y = this.tableTopY + Math.sin(this.elapsed * 3 + 1) * 0.05;
    }
  }

  render() {
    const delta = this.clock.getDelta();
    this.update(delta);
    this.composer.render();
  }

  // ─── Cleanup ───

  dispose() {
    window.removeEventListener('resize', this.boundOnResize);
    this.renderer.dispose();
    this.scene.traverse((obj) => {
      if (obj.geometry) obj.geometry.dispose();
      if (obj.material) {
        if (Array.isArray(obj.material)) {
          obj.material.forEach(m => m.dispose());
        } else {
          obj.material.dispose();
        }
      }
    });
  }
}
