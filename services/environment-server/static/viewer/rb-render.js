/**
 * Red Button 3D Renderer
 *
 * Three.js scene with shared arena, post-processing, procedural red button
 * on pedestal, and two holographic GLB characters (Persuader + Resistor).
 */

import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import * as SkeletonUtils from 'three/addons/utils/SkeletonUtils.js';
import { buildArena } from './shared/shared-arena.js';
import { createPostProcessing } from './shared/shared-postprocessing.js';
import { createHolographicMaterial, createWireframeMaterial } from './shared/shared-materials.js';
import { buildChair } from './shared/shared-chair.js';

const DEFAULT_PERSUADER_COLOR = 0x3b82f6;
const DEFAULT_RESISTOR_COLOR = 0x22c55e;
const BUTTON_RED = 0xdc2626;

export class RedButtonRenderer {
  constructor(canvas, { persuaderColor, resistorColor } = {}) {
    this.persuaderColor = persuaderColor ?? DEFAULT_PERSUADER_COLOR;
    this.resistorColor = resistorColor ?? DEFAULT_RESISTOR_COLOR;
    this.canvas = canvas;
    this.clock = new THREE.Clock();

    // Three.js core
    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x0A1520);
    this.scene.fog = new THREE.FogExp2(0x0A1520, 0.008);

    this.camera = new THREE.PerspectiveCamera(
      55,
      window.innerWidth / window.innerHeight,
      0.1,
      200,
    );
    this.camera.position.set(0, 6, 12);
    this.camera.lookAt(0, 2, 0);

    this.renderer = new THREE.WebGLRenderer({
      canvas,
      antialias: true,
      alpha: false,
    });
    this.renderer.setSize(window.innerWidth, window.innerHeight);
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.shadowMap.enabled = true;

    // Arena
    const arena = buildArena(this.scene);
    this.arenaMaterials = arena.materials;

    // Post-processing
    const pp = createPostProcessing(this.renderer, this.scene, this.camera, {
      bloom: { strength: 0.6 },
    });
    this.composer = pp.composer;
    this.bloomPass = pp.bloomPass;

    // Lighting
    this._setupLights();

    // Procedural button
    this._buildButton();

    // Characters
    this.characters = { persuader: null, resistor: null };
    this.characterMaterials = [];
    this.mixers = [];
    this.charactersLoaded = this._loadCharacters();

    // Button animation state
    this.buttonTime = 0;
    this.buttonPressed = false;
    this.buttonPressT = 0;

    // Active player
    this.activeRole = null;

    // Per-character idle animation state (staggered phases)
    this.idlePhases = {
      persuader: { breathPhase: 0, swayPhase: Math.PI * 0.3, microPhase: 0 },
      resistor:  { breathPhase: Math.PI, swayPhase: Math.PI * 1.1, microPhase: Math.PI * 0.7 },
    };

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
    const fillLight = new THREE.PointLight(0x113344, 0.4, 30);
    fillLight.position.set(0, -1, 0);
    this.scene.add(fillLight);
  }

  _buildButton() {
    this.buttonGroup = new THREE.Group();

    // Pedestal
    const pedestalGeo = new THREE.CylinderGeometry(1.2, 1.5, 0.8, 32);
    const pedestalMat = new THREE.MeshStandardMaterial({
      color: 0x1a1a2e,
      metalness: 0.8,
      roughness: 0.3,
    });
    const pedestal = new THREE.Mesh(pedestalGeo, pedestalMat);
    pedestal.position.y = 0.4;
    pedestal.castShadow = true;
    pedestal.receiveShadow = true;
    this.buttonGroup.add(pedestal);

    // Button cap (half-sphere)
    const capGeo = new THREE.SphereGeometry(0.9, 32, 16, 0, Math.PI * 2, 0, Math.PI / 2);
    this.buttonMat = new THREE.MeshStandardMaterial({
      color: BUTTON_RED,
      emissive: 0xff2222,
      emissiveIntensity: 0.4,
      metalness: 0.3,
      roughness: 0.4,
    });
    this.buttonCap = new THREE.Mesh(capGeo, this.buttonMat);
    this.buttonCap.position.y = 0.8;
    this.buttonCap.castShadow = true;
    this.buttonGroup.add(this.buttonCap);

    // Red point light for floor glow
    this.buttonLight = new THREE.PointLight(0xff3333, 2, 8);
    this.buttonLight.position.set(0, 0.3, 0);
    this.buttonGroup.add(this.buttonLight);

    // Floor ring around pedestal
    const ringGeo = new THREE.RingGeometry(1.6, 2.2, 64);
    const ringMat = new THREE.ShaderMaterial({
      uniforms: {
        time: { value: 0 },
        ringColor: { value: new THREE.Color(BUTTON_RED) },
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
        uniform vec3 ringColor;
        varying vec2 vUv;
        void main() {
          float pulse = sin(time * 2.0) * 0.3 + 0.7;
          float alpha = pulse * 0.25 * smoothstep(0.0, 0.3, vUv.x) * smoothstep(1.0, 0.7, vUv.x);
          gl_FragColor = vec4(ringColor * 1.5, alpha);
        }
      `,
      transparent: true,
      depthWrite: false,
    });
    this.ringMat = ringMat;
    const ring = new THREE.Mesh(ringGeo, ringMat);
    ring.rotation.x = -Math.PI / 2;
    ring.position.y = 0.01;
    this.buttonGroup.add(ring);

    this.scene.add(this.buttonGroup);
  }

  /** Build a sci-fi chair for a character seat position. */
  _buildSeatPlatform(color) {
    const chair = buildChair(color, { scale: 1.3 });

    // Point light (higher for 4x scale characters)
    const light = new THREE.PointLight(color, 1.0, 8);
    light.position.y = 3.0;
    chair.add(light);

    return chair;
  }

  async _loadCharacters() {
    const loader = new GLTFLoader();

    const configs = [
      {
        role: 'persuader',
        path: 'assets/characters/character_01.glb',
        color: this.persuaderColor,
        position: new THREE.Vector3(-4, 0, 1.5),
      },
      {
        role: 'resistor',
        path: 'assets/characters/character_02.glb',
        color: this.resistorColor,
        position: new THREE.Vector3(4, 0, 1.5),
      },
    ];

    const promises = configs.map(
      (cfg) =>
        new Promise((resolve) => {
          loader.load(
            cfg.path,
            (gltf) => {
              // Build seat platform at the character's XZ position
              const seatGroup = this._buildSeatPlatform(cfg.color);
              seatGroup.position.copy(cfg.position);
              // Face directly toward the button along X axis (same Y/Z to avoid tilt)
              seatGroup.lookAt(0, cfg.position.y, cfg.position.z);
              this.scene.add(seatGroup);

              // Clone scene with proper skeleton rebinding
              const model = SkeletonUtils.clone(gltf.scene);
              model.scale.set(4.0, 4.0, 4.0);
              // Sit on chair seat surface (seatTopY ~0.95 at scale 1.3)
              model.position.y = 0.95;
              // GLB faces +Z but lookAt already aligned seatGroup's +Z toward button;
              // flip character 180° so its front faces the same direction
              model.rotation.y = Math.PI;

              // Apply holographic + wireframe materials to skinned meshes only
              const holoMat = createHolographicMaterial(cfg.color, {
                skinning: true,
                roleBlend: false,
              });
              const wireMat = createWireframeMaterial(cfg.color, { skinning: true });
              this.characterMaterials.push(holoMat, wireMat);

              const wireframeClones = [];
              model.traverse((child) => {
                if (child.isSkinnedMesh) {
                  child.material = holoMat;
                  child.frustumCulled = false;
                  const wireClone = child.clone();
                  wireClone.material = wireMat;
                  wireClone.frustumCulled = false;
                  wireClone.bind(child.skeleton, child.bindMatrix);
                  wireframeClones.push({ clone: wireClone, parent: child.parent });
                } else if (child.isMesh) {
                  // Hide static geometry (chair/bench props baked in GLB)
                  child.visible = false;
                }
              });
              for (const { clone, parent } of wireframeClones) {
                (parent || model).add(clone);
              }

              // Animations — play idle_seated if available, else first clip
              let mixer = null;
              if (gltf.animations && gltf.animations.length > 0) {
                mixer = new THREE.AnimationMixer(model);
                const idleClip = gltf.animations.find(c => c.name === 'idle_seated')
                  || gltf.animations[0];
                const action = mixer.clipAction(idleClip.clone());
                action.play();
                this.mixers.push(mixer);
              }

              seatGroup.add(model);
              this.characters[cfg.role] = {
                model: seatGroup,
                holoMat,
                wireMat,
                mixer,
                baseY: 0,
                color: cfg.color,
              };

              resolve();
            },
            undefined,
            (err) => {
              console.warn(`Failed to load ${cfg.role} character:`, err);
              this._createPlaceholder(cfg);
              resolve();
            },
          );
        }),
    );

    await Promise.all(promises);
  }

  _createPlaceholder(cfg) {
    // Build platform even for placeholders
    const seatGroup = this._buildSeatPlatform(cfg.color);
    seatGroup.position.copy(cfg.position);
    seatGroup.lookAt(0, 0, 0);

    const geo = new THREE.CapsuleGeometry(0.4, 1.2, 8, 16);
    const holoMat = createHolographicMaterial(cfg.color, { skinning: false });
    const wireMat = createWireframeMaterial(cfg.color, { skinning: false });
    this.characterMaterials.push(holoMat, wireMat);

    const mesh = new THREE.Mesh(geo, holoMat);
    const wireMesh = new THREE.Mesh(geo.clone(), wireMat);

    const charGroup = new THREE.Group();
    charGroup.add(mesh);
    charGroup.add(wireMesh);
    charGroup.position.y = 1.5; // above chair seat
    charGroup.scale.set(4.0, 4.0, 4.0);
    seatGroup.add(charGroup);
    this.scene.add(seatGroup);

    this.characters[cfg.role] = {
      model: seatGroup,
      holoMat,
      wireMat,
      mixer: null,
      baseY: 0,
      color: cfg.color,
    };
  }

  /** Get world position of a character by role ('persuader' or 'resistor'). */
  getCharacterPosition(role) {
    const char = this.characters[role];
    if (!char) return new THREE.Vector3(role === 'persuader' ? -3.5 : 3.5, 5, 1.5);
    const pos = new THREE.Vector3();
    char.model.getWorldPosition(pos);
    // seatGroup at y=0, character at y=1.6 scaled 4x → head ~8 above group origin
    pos.y += 8.0;
    return pos;
  }

  /** Brighten active character, dim other. */
  setActivePlayer(role) {
    this.activeRole = role;
    for (const [r, char] of Object.entries(this.characters)) {
      if (!char) continue;
      const intensity = r === role ? 0.5 : 0.2;
      if (char.holoMat?.uniforms?.glowIntensity) {
        char.holoMat.uniforms.glowIntensity.value = intensity;
      }
    }
  }

  /** Trigger button press animation. */
  animateButtonPress() {
    this.buttonPressed = true;
    this.buttonPressT = 0;
  }

  /** Trigger victory effect: winner brightens, loser fades. */
  animateVictory(winnerRole) {
    for (const [role, char] of Object.entries(this.characters)) {
      if (!char) continue;
      if (role === winnerRole) {
        if (char.holoMat?.uniforms?.glowIntensity) {
          char.holoMat.uniforms.glowIntensity.value = 0.8;
        }
      } else {
        // Fade loser to 20% opacity
        if (char.holoMat) char.holoMat.transparent = true;
        char.model.traverse((child) => {
          if (child.isMesh && child.material) {
            child.material.opacity = 0.2;
          }
        });
      }
    }
  }

  /** Main update tick — call each frame. */
  update(delta) {
    this.buttonTime += delta;

    // Update arena shader materials (time uniform)
    for (const mat of this.arenaMaterials) {
      if (mat.uniforms?.time) mat.uniforms.time.value = this.buttonTime;
    }

    // Update character holographic/wireframe materials
    for (const mat of this.characterMaterials) {
      if (mat.uniforms?.time) mat.uniforms.time.value = this.buttonTime;
    }

    // Update ring shader
    if (this.ringMat?.uniforms?.time) {
      this.ringMat.uniforms.time.value = this.buttonTime;
    }

    // Animation mixers
    for (const mixer of this.mixers) {
      mixer.update(delta);
    }

    // Idle character animations — breathing, subtle sway, micro-movements
    for (const [role, char] of Object.entries(this.characters)) {
      if (!char) continue;
      const phase = this.idlePhases[role];
      if (!phase) continue;

      // Find the inner character model (the child Group placed on the seat)
      const innerModel = char.model.children.find(
        (c) => c.isGroup || (c.isObject3D && c.children.length > 0 && c !== char.model),
      );
      const target = innerModel || char.model;

      // Breathing: subtle Y-scale oscillation (±0.4%)
      const breathSpeed = 1.2;
      const breathAmp = 0.004;
      const breath = Math.sin(this.buttonTime * breathSpeed + phase.breathPhase) * breathAmp;
      target.scale.y = (target.userData.baseScaleY || target.scale.y) * (1.0 + breath);
      if (!target.userData.baseScaleY) target.userData.baseScaleY = target.scale.y / (1.0 + breath);

      // Gentle sway: subtle Y-axis rotation oscillation (±1.5°)
      const swaySpeed = 0.4;
      const swayAmp = 0.026; // ~1.5 degrees
      const sway = Math.sin(this.buttonTime * swaySpeed + phase.swayPhase) * swayAmp;
      const baseRotY = target.userData.baseRotY ?? target.rotation.y;
      if (target.userData.baseRotY === undefined) target.userData.baseRotY = target.rotation.y;
      target.rotation.y = baseRotY + sway;

      // Micro-bob: gentle vertical float (only for characters WITHOUT skeletal animation)
      if (!char.mixer) {
        const bobSpeed = 1.5;
        const bobAmp = 0.05;
        const bob = Math.sin(this.buttonTime * bobSpeed + phase.microPhase) * bobAmp;
        target.position.y = (target.userData.basePosY ?? target.position.y) + bob;
        if (target.userData.basePosY === undefined) target.userData.basePosY = target.position.y - bob;
      }

      // Active player glow pulse (subtle intensity oscillation for the active role)
      if (role === this.activeRole && char.holoMat?.uniforms?.glowIntensity) {
        const pulse = Math.sin(this.buttonTime * 3.0) * 0.08 + 0.5;
        char.holoMat.uniforms.glowIntensity.value = pulse;
      }
    }

    // Button pulse (scale oscillation + emissive intensity)
    if (!this.buttonPressed) {
      const pulse = Math.sin(this.buttonTime * 2.0) * 0.025 + 1.0;
      this.buttonCap.scale.setScalar(pulse);
      this.buttonMat.emissiveIntensity = 0.3 + Math.sin(this.buttonTime * 2.0) * 0.15;
      this.buttonLight.intensity = 1.5 + Math.sin(this.buttonTime * 2.0) * 0.5;
    } else {
      // Button press animation
      this.buttonPressT += delta;
      const t = Math.min(this.buttonPressT / 0.2, 1.0);
      const scale = 1.0 - 0.3 * t;
      this.buttonCap.scale.setScalar(scale);
      this.buttonMat.emissiveIntensity = 0.4 + t * 1.5;
      this.buttonLight.intensity = 2.0 + t * 6.0;

      // Hold pressed state
      if (this.buttonPressT > 0.2) {
        this.buttonCap.scale.setScalar(0.7);
      }
    }
  }

  /** Render frame via post-processing composer. */
  render() {
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
    this.renderer.dispose();
  }
}
