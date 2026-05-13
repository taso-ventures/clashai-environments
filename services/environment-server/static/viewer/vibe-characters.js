/**
 * Vibe Check Character System
 *
 * 6 Blender-modeled holographic wireframe humanoids loaded from glTF (.glb),
 * each with a distinct silhouette, seated pose, and 3 skeletal animations.
 *
 * Characters are loaded via GLTFLoader and receive holographic + wireframe
 * shaders at runtime. Animations play via THREE.AnimationMixer with crossfade.
 *
 * Character variants:
 *   0: Tall/Lean    — angular features, hands steepled
 *   1: Broad/Stocky — relaxed posture, arms resting wide
 *   2: Medium/Expressive — one hand raised, animated speaker
 *   3: Compact/Alert — leaning forward, hands on knees
 *   4: Lithe/Graceful — elegant posture, one arm on chair back
 *   5: Heavy/Imposing — arms crossed, commanding presence
 */

import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import * as SkeletonUtils from 'three/addons/utils/SkeletonUtils.js';
import { getPlayerColor as sharedGetPlayerColor } from './shared/shared-colors.js';
import {
  createHolographicMaterial as sharedCreateHolographicMaterial,
  createWireframeMaterial as sharedCreateWireframeMaterial,
} from './shared/shared-materials.js';

// ============================================================================
// Player Colors — 6 unique, grouped into cool (Team A) and warm (Team B)
// ============================================================================

export const PLAYER_COLORS = [
  // Team A (cool family) — near side positions 0, 1, 2
  0x00E5CC,  // Player 0: Teal (primary team color)
  0x22AAFF,  // Player 1: Cyan-Blue
  0x44FFAA,  // Player 2: Mint-Green
  // Team B (warm family) — far side positions 3, 4, 5
  0xFF8844,  // Player 3: Orange (primary team color)
  0xFF5566,  // Player 4: Coral-Red
  0xFFCC22,  // Player 5: Amber-Gold
];

export const TEAM_A_COLOR = 0x00E5CC;
export const TEAM_B_COLOR = 0xFF8844;

export function getPlayerColor(playerId) {
  return sharedGetPlayerColor(playerId, PLAYER_COLORS);
}

export function getTeamColor(playerId) {
  return playerId < 3 ? TEAM_A_COLOR : TEAM_B_COLOR;
}

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
// Animation name mapping: pose name -> glTF animation name
// ============================================================================

const POSE_TO_ANIM = {
  idle:     'idle_seated',
  speaking: 'speaking_gesture',
  reactive: 'reactive_worried',
};

// ============================================================================
// Holographic + Wireframe Materials (delegates to shared module with skinning)
// ============================================================================

/**
 * Creates holographic body material with skinning for SkinnedMesh from glTF.
 * Delegates to shared material factory with skinning enabled.
 */
export function createHolographicMaterial(color) {
  return sharedCreateHolographicMaterial(color, { skinning: true });
}

/**
 * Creates wireframe overlay material with skinning for SkinnedMesh from glTF.
 * Delegates to shared material factory with skinning enabled.
 */
export function createWireframeMaterial(color) {
  return sharedCreateWireframeMaterial(color, { skinning: true });
}

// ============================================================================
// GLTFLoader singleton
// ============================================================================

const gltfLoader = new GLTFLoader();

/** Cache loaded GLTF data so multiple players using the same model don't re-fetch. */
const gltfCache = new Map();

/**
 * Load a GLTF file, returning a promise. Uses cache to avoid re-fetching.
 */
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
  // Clone the scene so each player gets independent instances.
  // SkeletonUtils.clone() properly rebinds SkinnedMesh skeletons to cloned bones,
  // unlike Object3D.clone(true) which leaves skeleton refs pointing at the original.
  const scene = SkeletonUtils.clone(gltfData.scene);

  const group = new THREE.Group();
  group.userData.modelIndex = modelIndex;
  group.userData.playerColor = playerColor;
  group.userData.currentPose = 'idle';

  // Create materials
  const bodyMaterial = createHolographicMaterial(playerColor);
  const wireframeMat = createWireframeMaterial(playerColor);

  // Traverse the loaded scene, replace materials, add wireframe clones
  const wireframeClones = [];

  scene.traverse((child) => {
    if (child.isSkinnedMesh) {
      // Replace material with holographic shader
      child.material = bodyMaterial;
      child.frustumCulled = false;

      // Create wireframe overlay clone
      const wireClone = child.clone();
      wireClone.material = wireframeMat;
      wireClone.frustumCulled = false;
      // Share the same skeleton so wireframe deforms with body
      wireClone.bind(child.skeleton, child.bindMatrix);
      wireframeClones.push({ clone: wireClone, parent: child.parent });
    } else if (child.isMesh) {
      // Hide static geometry from GLB (chair/bench props) — only skeletal meshes are wanted
      child.visible = false;
    }
  });

  // Add wireframe clones
  for (const { clone, parent } of wireframeClones) {
    if (parent) {
      parent.add(clone);
    } else {
      scene.add(clone);
    }
  }

  group.add(scene);

  // Set up AnimationMixer
  const mixer = new THREE.AnimationMixer(scene);
  const actions = {};

  // Clone animations from original GLTF data (not from cloned scene)
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

  // Store references
  group.userData.bodyMaterial = bodyMaterial;
  group.userData.wireframeMaterial = wireframeMat;
  group.userData.mixer = mixer;
  group.userData.actions = actions;
  group.userData.currentAction = idleAction || null;

  return {
    group,
    bodyMaterial,
    wireframeMaterial: wireframeMat,
    mixer,
  };
}

// ============================================================================
// Pose System (crossfade between skeletal animations)
// ============================================================================

/**
 * Set the pose for a character group.
 * Crossfades between skeletal animations using AnimationMixer.
 *
 * @param {THREE.Group} characterGroup — the character's root group
 * @param {string} pose — 'idle' | 'speaking' | 'reactive'
 */
export function setCharacterPose(characterGroup, pose) {
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

  // Update glow intensity based on pose
  const glowMap = { idle: 0.4, speaking: 0.55, reactive: 0.3 };
  ud.targetGlow = glowMap[pose] ?? 0.4;
  ud.currentPose = pose;
}

// ============================================================================
// Role Badge System
// ============================================================================

const ROLE_BADGE_COLORS = {
  CLUEGIVER: 0x00E5CC,
  GUESSER: 0x22AAFF,
  'STEAL TEAM': 0xFF8844,
};

function createRoleBadgeSprite(text, color) {
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d');
  canvas.width = 256;
  canvas.height = 64;

  // Background pill
  ctx.fillStyle = 'rgba(10, 21, 32, 0.85)';
  const r = 12;
  ctx.beginPath();
  ctx.roundRect(4, 4, canvas.width - 8, canvas.height - 8, r);
  ctx.fill();

  // Border
  const hex = '#' + (color & 0xFFFFFF).toString(16).padStart(6, '0');
  ctx.strokeStyle = hex;
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.roundRect(4, 4, canvas.width - 8, canvas.height - 8, r);
  ctx.stroke();

  // Text
  ctx.font = 'bold 26px "Geist Sans", sans-serif';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillStyle = hex;
  ctx.shadowColor = hex;
  ctx.shadowBlur = 8;
  ctx.fillText(text, canvas.width / 2, canvas.height / 2);
  ctx.shadowBlur = 0;

  const texture = new THREE.CanvasTexture(canvas);
  texture.minFilter = THREE.LinearFilter;

  const material = new THREE.SpriteMaterial({
    map: texture,
    transparent: true,
    depthTest: false,
  });

  const sprite = new THREE.Sprite(material);
  sprite.scale.set(1.6, 0.4, 1);
  sprite.userData.isRoleBadge = true;
  sprite.userData.canvas = canvas;
  sprite.userData.ctx = ctx;
  sprite.userData.texture = texture;

  return sprite;
}

/**
 * Set the role label above a character.
 * Pass null/undefined to remove.
 *
 * @param {THREE.Group} characterGroup
 * @param {string|null} role — 'CLUEGIVER' | 'GUESSER' | 'STEAL TEAM' | null
 */
export function setCharacterRole(characterGroup, role) {
  // Remove existing badge
  const existing = characterGroup.children.find(
    c => c.userData?.isRoleBadge
  );
  if (existing) {
    characterGroup.remove(existing);
    existing.material.map?.dispose();
    existing.material.dispose();
  }

  if (!role) {
    characterGroup.userData.role = null;
    return;
  }

  const color = ROLE_BADGE_COLORS[role] || 0xffffff;
  const badge = createRoleBadgeSprite(role, color);

  // Position above character (characters are ~0.9m tall in glTF space)
  const badgeY = 1.2;
  badge.position.set(0, badgeY, 0);
  badge.userData.baseY = badgeY;

  characterGroup.add(badge);
  characterGroup.userData.role = role;
}

// ============================================================================
// Per-frame Character Update
// ============================================================================

/**
 * Update character: AnimationMixer tick, material time, badge animation.
 *
 * @param {THREE.Group} characterGroup
 * @param {number} elapsed — total elapsed seconds
 * @param {number} playerId — for phase-offset in animations
 * @param {number} delta — frame delta seconds (for mixer)
 */
export function updateCharacter(characterGroup, elapsed, playerId, delta) {
  const ud = characterGroup.userData;

  // Advance skeletal animation
  if (ud.mixer) {
    ud.mixer.update(delta || 0.016);
  }

  // Update material time uniforms
  if (ud.bodyMaterial?.uniforms?.time) {
    ud.bodyMaterial.uniforms.time.value = elapsed;
  }
  if (ud.wireframeMaterial?.uniforms?.time) {
    ud.wireframeMaterial.uniforms.time.value = elapsed;
  }

  // Smooth glow intensity transition
  if (ud.bodyMaterial?.uniforms?.glowIntensity && ud.targetGlow !== undefined) {
    const current = ud.bodyMaterial.uniforms.glowIntensity.value;
    ud.bodyMaterial.uniforms.glowIntensity.value += (ud.targetGlow - current) * 0.05;
  }

  // Badge floating animation
  const phaseOffset = playerId * 1.5;
  const badge = characterGroup.children.find(c => c.userData?.isRoleBadge);
  if (badge && badge.userData.baseY !== undefined) {
    badge.position.y = badge.userData.baseY + Math.sin(elapsed * 2.5 + phaseOffset) * 0.05;
  }
}

// ============================================================================
// Position Helpers
// ============================================================================

/**
 * Get the 6 player positions around the oval table.
 * 3 on near side (Team A, teal), 3 on far side (Team B, orange).
 * Teams sit at opposite ends of the table (split along Z axis).
 * Supports variable team sizes (1-3 players per team).
 *
 * @param {number} tableRadiusX
 * @param {number} tableRadiusZ
 * @param {number} teamACount — number of players on Team A (default 3)
 * @param {number} teamBCount — number of players on Team B (default 3)
 * @returns {Array<{x, z, teamSide}>}
 */
export function getPlayerPositions(tableRadiusX, tableRadiusZ, teamACount = 3, teamBCount = 3) {
  const offsetZ = tableRadiusZ + 2.5;
  const midOffsetZ = tableRadiusZ + 3.0;
  const xSpread = 3.5;

  function teamPositions(count, side, zSign) {
    const z = zSign * offsetZ;
    const zMid = zSign * midOffsetZ;
    if (count === 1) return [{ x: 0, z: zMid, teamSide: side }];
    if (count === 2) return [
      { x: -xSpread, z, teamSide: side },
      { x: xSpread,  z, teamSide: side },
    ];
    return [
      { x: -xSpread, z,    teamSide: side },
      { x: 0,        z: zMid, teamSide: side },
      { x: xSpread,  z,    teamSide: side },
    ];
  }

  return [
    ...teamPositions(teamACount, 'A', -1),
    ...teamPositions(teamBCount, 'B', 1),
  ];
}

/**
 * Model index assignment per player position.
 * Each position gets a unique character variant.
 */
export const POSITION_MODEL_MAP = [0, 1, 2, 3, 4, 5];
