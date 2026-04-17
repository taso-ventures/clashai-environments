/**
 * Coup Character System
 *
 * Loads Blender-modeled GLB characters via GLTFLoader with holographic +
 * wireframe shaders and skeletal animations. Shares model assets with
 * Vibe Check but maintains Coup-specific visual features (role aura,
 * active/eliminated state).
 */

import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import * as SkeletonUtils from 'three/addons/utils/SkeletonUtils.js';
import { PLAYER_COLORS, getPlayerColor } from './shared/shared-colors.js';
import {
  createHolographicMaterial as _createSharedHolographicMaterial,
  createWireframeMaterial as _createSharedWireframeMaterial,
} from './shared/shared-materials.js';

// Re-export shared colors for downstream consumers
export { PLAYER_COLORS, getPlayerColor };

// ============================================================================
// Animation name mapping: pose name -> glTF animation clip name
// ============================================================================

const POSE_TO_ANIM = {
  idle:     'idle_seated',
  speaking: 'speaking_gesture',
  reactive: 'reactive_worried',
};

// ============================================================================
// Asset Paths
// ============================================================================

const ASSET_BASE = './assets/characters/';

const CHARACTER_FILES = [
  'character_01.glb',
  'character_02.glb',
  'character_03.glb',
  'character_04.glb',
  'character_05.glb',
  'character_06.glb',
];

// ============================================================================
// Materials — delegates to shared with Coup-specific options
// ============================================================================

/**
 * Creates holographic body material with skinning + roleBlend for Coup's
 * role aura system (roleColor/roleBlend uniforms).
 *
 * @param {number} color - Hex color for the hologram
 * @returns {THREE.ShaderMaterial}
 */
export function createHolographicMaterial(color) {
  return _createSharedHolographicMaterial(color, { skinning: true, roleBlend: true });
}

/**
 * Creates wireframe overlay material with skinning for SkinnedMesh from glTF.
 *
 * @param {number} color - Hex color
 * @returns {THREE.ShaderMaterial}
 */
export function createWireframeMaterial(color) {
  return _createSharedWireframeMaterial(color, { skinning: true });
}

// ============================================================================
// GLTFLoader singleton + cache
// ============================================================================

const gltfLoader = new GLTFLoader();
const gltfCache = new Map();

function loadGLTF(url) {
  if (gltfCache.has(url)) {
    return Promise.resolve(gltfCache.get(url));
  }
  return new Promise((resolve, reject) => {
    gltfLoader.load(
      url,
      (gltf) => {
        gltfCache.set(url, gltf);
        resolve(gltf);
      },
      undefined,
      reject,
    );
  });
}

// ============================================================================
// Character Factory (async — loads glTF)
// ============================================================================

/**
 * Creates a complete character by loading a glTF model, applying holographic
 * + wireframe shaders, and setting up an AnimationMixer.
 *
 * @param {number} modelIndex — character variant 0-5
 * @param {number} playerColor — hex color for this player
 * @returns {Promise<{ group, bodyMaterial, wireframeMaterial, mixer }>}
 */
export async function createCharacter(modelIndex, playerColor) {
  const fileIndex = modelIndex % CHARACTER_FILES.length;
  const url = ASSET_BASE + CHARACTER_FILES[fileIndex];

  const gltfData = await loadGLTF(url);
  const scene = SkeletonUtils.clone(gltfData.scene);

  const group = new THREE.Group();

  // Create materials
  const bodyMaterial = createHolographicMaterial(playerColor);
  const wireframeMat = createWireframeMaterial(playerColor);

  // Traverse the loaded scene, replace materials, add wireframe clones
  const wireframeClones = [];

  scene.traverse((child) => {
    if (child.isSkinnedMesh) {
      child.material = bodyMaterial;
      child.frustumCulled = false;

      const wireClone = child.clone();
      wireClone.material = wireframeMat;
      wireClone.frustumCulled = false;
      wireClone.bind(child.skeleton, child.bindMatrix);
      wireframeClones.push({ clone: wireClone, parent: child.parent });
    } else if (child.isMesh) {
      // Hide static geometry from GLB (chair/bench props) — only skeletal meshes are wanted
      child.visible = false;
    }
  });

  for (const { clone, parent } of wireframeClones) {
    if (parent) {
      parent.add(clone);
    } else {
      scene.add(clone);
    }
  }

  group.add(scene);

  // Find head bone for procedural look-at
  let headBone = null;
  scene.traverse((child) => {
    if (child.isBone && child.name === 'head') headBone = child;
  });
  group.userData.headBone = headBone;

  // Set up AnimationMixer with all clip actions
  const mixer = new THREE.AnimationMixer(scene);
  const actions = {};

  for (const clip of gltfData.animations) {
    const clonedClip = clip.clone();
    const action = mixer.clipAction(clonedClip);
    action.setLoop(THREE.LoopRepeat);
    actions[clip.name] = action;
  }

  // Start with idle animation
  const idleAction = actions[POSE_TO_ANIM.idle];
  if (idleAction) {
    idleAction.play();
  }

  // Store animation state on group.userData for pose crossfade
  group.userData.actions = actions;
  group.userData.currentAction = idleAction || null;
  group.userData.currentPose = 'idle';

  return {
    group,
    bodyMaterial,
    wireframeMaterial: wireframeMat,
    mixer,
    actions,
  };
}

// ============================================================================
// Pose Crossfade System
// ============================================================================

/**
 * Crossfade a character to a new pose using AnimationMixer.
 * Reuses the same pattern as vibe-characters.js setCharacterPose.
 *
 * @param {THREE.Group} characterGroup — the character's root group (from createCharacter)
 * @param {string} pose — 'idle' | 'speaking' | 'reactive'
 */
export function setCoupCharacterPose(characterGroup, pose) {
  const ud = characterGroup.userData;
  if (ud.currentPose === pose) return;

  const animName = POSE_TO_ANIM[pose] || POSE_TO_ANIM.idle;
  const nextAction = ud.actions?.[animName];
  const currentAction = ud.currentAction;

  if (nextAction && nextAction !== currentAction) {
    const fadeTime = 0.4;
    nextAction.reset().setEffectiveTimeScale(1).setEffectiveWeight(1).fadeIn(fadeTime).play();
    if (currentAction) {
      currentAction.fadeOut(fadeTime);
    }
    ud.currentAction = nextAction;
  }

  ud.currentPose = pose;
}

// ============================================================================
// Role Aura System
// ============================================================================

/**
 * Applies a role aura tint to a humanoid when a role is claimed.
 * Blends the character's base color toward the role color and adds a pulsing
 * glow ring at the character's feet.
 *
 * @param {THREE.Group} humanoid - The humanoid group
 * @param {number} roleColorHex - The role color hex value
 * @param {THREE.Scene} scene - Scene to add the aura ring to
 */
export function setRoleAura(humanoid, roleColorHex, scene) {
  const roleColor = new THREE.Color(roleColorHex);

  humanoid.traverse((child) => {
    if (child.isMesh && child.material && child.material.uniforms) {
      if (child.material.uniforms.roleColor) {
        child.material.uniforms.roleColor.value.copy(roleColor);
      }
      if (child.material.uniforms.roleBlend) {
        child.material.uniforms.roleBlend.value = 0.3;
      }
    }
  });

  // Add pulsing glow ring at feet
  const ringGeometry = new THREE.TorusGeometry(0.5, 0.04, 8, 32);
  const ringMaterial = new THREE.MeshBasicMaterial({
    color: roleColorHex,
    transparent: true,
    opacity: 0.7,
  });
  const ring = new THREE.Mesh(ringGeometry, ringMaterial);
  ring.rotation.x = -Math.PI / 2;
  ring.position.y = 0.05;
  ring.userData.isRoleAuraRing = true;
  ring.userData.createdAt = performance.now();
  ring.userData.cancelled = false;
  ring.userData.rafId = null;
  humanoid.add(ring);

  const fadeRing = () => {
    if (ring.userData.cancelled) return;
    const age = (performance.now() - ring.userData.createdAt) / 1000;
    if (age < 2.5) {
      ring.material.opacity = 0.7 * (1 - age / 2.5);
      const pulse = 1 + Math.sin(age * 6) * 0.15;
      ring.scale.set(pulse, pulse, 1);
      ring.userData.rafId = requestAnimationFrame(fadeRing);
    } else {
      humanoid.remove(ring);
      ring.geometry.dispose();
      ring.material.dispose();
    }
  };
  ring.userData.rafId = requestAnimationFrame(fadeRing);
}

/**
 * Clears role aura, restoring original color
 *
 * @param {THREE.Group} humanoid - The humanoid group
 * @param {number} originalColorHex - Original player color
 * @param {THREE.Scene} scene - Scene reference
 */
export function clearRoleAura(humanoid, originalColorHex, scene) {
  humanoid.traverse((child) => {
    if (child.isMesh && child.material && child.material.uniforms) {
      if (child.material.uniforms.roleBlend) {
        child.material.uniforms.roleBlend.value = 0.0;
      }
    }
  });

  // Remove any lingering aura rings
  const toRemove = [];
  humanoid.traverse((child) => {
    if (child.userData && child.userData.isRoleAuraRing) {
      toRemove.push(child);
    }
  });
  for (const ring of toRemove) {
    ring.userData.cancelled = true;
    if (ring.userData.rafId != null) {
      cancelAnimationFrame(ring.userData.rafId);
    }
    ring.parent.remove(ring);
    ring.geometry.dispose();
    ring.material.dispose();
  }
}

// ============================================================================
// Player State Management
// ============================================================================

/**
 * Updates a player's active state visual.
 * Active players glow brighter.
 *
 * @param {THREE.Group} humanoid - The humanoid group
 * @param {THREE.PointLight} light - The player's point light
 * @param {THREE.Mesh} groundGlow - The ground glow mesh
 * @param {boolean} active - Whether player is active
 */
export function setPlayerActiveVisuals(humanoid, light, groundGlow, active) {
  const intensity = active ? 1.0 : 0.7;
  light.intensity = intensity;

  humanoid.traverse((child) => {
    if (child.isMesh && child.material && child.material.uniforms) {
      if (child.material.uniforms.glowIntensity) {
        child.material.uniforms.glowIntensity.value = active ? 0.50 : 0.35;
      }
    }
  });

  if (groundGlow) {
    groundGlow.material.opacity = active ? 0.30 : 0.20;
  }
}

/**
 * Updates a player to eliminated state.
 * Greys out the hologram and dims all glows.
 *
 * @param {THREE.Group} humanoid - The humanoid group
 * @param {THREE.PointLight} light - The player's point light
 * @param {THREE.Mesh} groundGlow - The ground glow mesh
 */
export function setPlayerEliminatedVisuals(humanoid, light, groundGlow) {
  light.intensity = 0.1;

  humanoid.traverse((child) => {
    if (child.isMesh && child.material) {
      if (child.material.uniforms) {
        if (child.material.uniforms.baseColor) {
          child.material.uniforms.baseColor.value.setHex(0x333333);
        }
        if (child.material.uniforms.glowIntensity) {
          child.material.uniforms.glowIntensity.value = 0.1;
        }
        if (child.material.uniforms.roleBlend) {
          child.material.uniforms.roleBlend.value = 0.0;
        }
      } else {
        child.material.color.setHex(0x333333);
        child.material.emissive?.setHex(0x111111);
        child.material.emissiveIntensity = 0.05;
        child.material.opacity = 0.3;
      }
    }
  });

  if (groundGlow) {
    groundGlow.material.opacity = 0.05;
    groundGlow.material.color.setHex(0x333333);
  }
}
