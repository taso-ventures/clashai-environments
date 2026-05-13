/**
 * Shared Sci-Fi Chair Builder
 *
 * Procedural holographic chair geometry for AI arena viewers.
 * Replaces the flat disc seat platforms with a proper sci-fi throne
 * featuring curved backrest, armrests, and energy glow effects.
 *
 * All geometry is built from Three.js primitives — no external GLB needed.
 * The chair color is set per-player; materials use emissive + transparency
 * to match the holographic wireframe aesthetic of the arena.
 */

import * as THREE from 'three';

/**
 * Build a sci-fi chair group.
 *
 * @param {number} color - Hex color (e.g. 0x00E5CC)
 * @param {object} [opts]
 * @param {number} [opts.scale=1] - Uniform scale multiplier
 * @returns {THREE.Group} chair group with .userData.seatTopY (world-local Y of seat surface)
 */
export function buildChair(color, opts = {}) {
  const scale = opts.scale ?? 1;
  const group = new THREE.Group();

  const baseColor = new THREE.Color(color);
  const dimColor = new THREE.Color(color).multiplyScalar(0.4);

  // -- Shared material helpers --
  function solidMat(c, emissiveIntensity = 0.3, opacity = 0.85) {
    return new THREE.MeshStandardMaterial({
      color: c,
      emissive: c,
      emissiveIntensity,
      metalness: 0.9,
      roughness: 0.15,
      transparent: true,
      opacity,
    });
  }

  function glowMat(c, opacity = 0.5) {
    return new THREE.MeshBasicMaterial({
      color: c,
      transparent: true,
      opacity,
    });
  }

  // =====================
  // Base pedestal (tapered cylinder)
  // =====================
  const pedestalGeo = new THREE.CylinderGeometry(0.25, 0.45, 0.15, 16);
  const pedestal = new THREE.Mesh(pedestalGeo, solidMat(dimColor, 0.2, 0.9));
  pedestal.position.y = 0.075;
  group.add(pedestal);

  // =====================
  // Central stem (thin cylinder from base to seat)
  // =====================
  const stemH = 0.5;
  const stemGeo = new THREE.CylinderGeometry(0.06, 0.08, stemH, 8);
  const stem = new THREE.Mesh(stemGeo, solidMat(dimColor, 0.15, 0.7));
  stem.position.y = 0.15 + stemH / 2;
  group.add(stem);

  // Energy line along stem
  const energyGeo = new THREE.CylinderGeometry(0.03, 0.03, stemH * 0.8, 6);
  const energy = new THREE.Mesh(energyGeo, glowMat(baseColor, 0.6));
  energy.position.y = 0.15 + stemH / 2;
  group.add(energy);

  // =====================
  // Seat pan (wide disc, slightly concave look via two stacked cylinders)
  // =====================
  const seatY = 0.15 + stemH; // top of stem
  const seatThickness = 0.08;

  const seatGeo = new THREE.CylinderGeometry(0.7, 0.75, seatThickness, 24);
  const seat = new THREE.Mesh(seatGeo, solidMat(baseColor, 0.35, 0.85));
  seat.position.y = seatY + seatThickness / 2;
  group.add(seat);

  // Seat edge glow ring
  const seatRingGeo = new THREE.TorusGeometry(0.72, 0.015, 8, 32);
  const seatRing = new THREE.Mesh(seatRingGeo, glowMat(baseColor, 0.7));
  seatRing.rotation.x = Math.PI / 2;
  seatRing.position.y = seatY + seatThickness;
  group.add(seatRing);

  const seatTopY = seatY + seatThickness;

  // =====================
  // Backrest (curved panel using extruded arc)
  // =====================
  const backrestH = 0.65;
  const backrestW = 0.55;
  const backrestDepth = 0.04;

  // Use a box with slight curve approximation via two angled panels
  const backGeo = new THREE.BoxGeometry(backrestW * 2, backrestH, backrestDepth);
  const back = new THREE.Mesh(backGeo, solidMat(baseColor, 0.25, 0.75));
  // Position behind seat center, tilted back slightly
  back.position.set(0, seatTopY + backrestH / 2, -0.5);
  back.rotation.x = -0.15; // slight tilt backward
  group.add(back);

  // Backrest glow strip (vertical energy line)
  const backGlowGeo = new THREE.BoxGeometry(0.03, backrestH * 0.7, backrestDepth + 0.01);
  const backGlow = new THREE.Mesh(backGlowGeo, glowMat(baseColor, 0.8));
  backGlow.position.set(0, seatTopY + backrestH / 2, -0.52);
  backGlow.rotation.x = -0.15;
  group.add(backGlow);

  // Side glow strips on backrest
  for (const side of [-1, 1]) {
    const sideGlowGeo = new THREE.BoxGeometry(0.02, backrestH * 0.5, backrestDepth + 0.01);
    const sideGlow = new THREE.Mesh(sideGlowGeo, glowMat(baseColor, 0.5));
    sideGlow.position.set(side * backrestW * 0.85, seatTopY + backrestH * 0.55, -0.5);
    sideGlow.rotation.x = -0.15;
    group.add(sideGlow);
  }

  // =====================
  // Armrests (angled bars extending from backrest sides)
  // =====================
  for (const side of [-1, 1]) {
    // Arm support post (vertical)
    const postGeo = new THREE.CylinderGeometry(0.03, 0.035, 0.3, 8);
    const post = new THREE.Mesh(postGeo, solidMat(dimColor, 0.2, 0.8));
    post.position.set(side * 0.6, seatTopY + 0.15, -0.15);
    group.add(post);

    // Arm rest bar (horizontal)
    const barGeo = new THREE.BoxGeometry(0.06, 0.03, 0.45);
    const bar = new THREE.Mesh(barGeo, solidMat(baseColor, 0.3, 0.8));
    bar.position.set(side * 0.6, seatTopY + 0.30, -0.25);
    group.add(bar);

    // Armrest tip glow
    const tipGeo = new THREE.SphereGeometry(0.04, 8, 8);
    const tip = new THREE.Mesh(tipGeo, glowMat(baseColor, 0.9));
    tip.position.set(side * 0.6, seatTopY + 0.30, -0.02);
    group.add(tip);
  }

  // =====================
  // Ground glow disc
  // =====================
  const groundGlowGeo = new THREE.CircleGeometry(0.8, 24);
  const groundGlow = new THREE.Mesh(groundGlowGeo, glowMat(baseColor, 0.12));
  groundGlow.rotation.x = -Math.PI / 2;
  groundGlow.position.y = 0.01;
  group.add(groundGlow);

  // =====================
  // Floating energy ring around base
  // =====================
  const baseRingGeo = new THREE.TorusGeometry(0.5, 0.01, 8, 24);
  const baseRing = new THREE.Mesh(baseRingGeo, glowMat(baseColor, 0.4));
  baseRing.rotation.x = Math.PI / 2;
  baseRing.position.y = 0.25;
  group.add(baseRing);

  // Apply uniform scale
  group.scale.set(scale, scale, scale);

  // Store seat surface Y for character positioning (in local space before scale)
  group.userData.seatTopY = seatTopY;
  group.userData.chairColor = color;

  return group;
}
