/**
 * Coup Animation System Module
 * Handles visual effects for game events
 * Enhanced with spring physics and instanced particles
 */

import * as THREE from 'three';

// Timing constants (seconds)
export const TIMING = {
  cardFlip: 0.4,
  cardMove: 0.3,
  coinTransfer: 0.5,
  challengeLightning: 0.8,
  blockBarrier: 0.6,
  eliminationDissolve: 1.2,
  resolutionDisplay: 1.5,
  victoryBurst: 1.0,
  actionLabel: 1.2,
  stealBeam: 0.8,
  assassinateSlash: 0.7,
  coupStrike: 1.0,
  coinGain: 0.5,
};

/**
 * Spring physics for smooth, natural animations
 */
export class Spring {
  constructor(stiffness = 120, damping = 12) {
    this.stiffness = stiffness;
    this.damping = damping;
    this.position = 0;
    this.velocity = 0;
    this.target = 0;
  }

  setTarget(target) {
    this.target = target;
  }

  update(dt) {
    const force = -this.stiffness * (this.position - this.target);
    const dampingForce = -this.damping * this.velocity;
    this.velocity += (force + dampingForce) * dt;
    this.position += this.velocity * dt;
    return this.position;
  }

  isSettled(threshold = 0.001) {
    return Math.abs(this.position - this.target) < threshold &&
           Math.abs(this.velocity) < threshold;
  }
}

/**
 * 3D Spring for vector properties
 */
export class Spring3D {
  constructor(stiffness = 120, damping = 12) {
    this.x = new Spring(stiffness, damping);
    this.y = new Spring(stiffness, damping);
    this.z = new Spring(stiffness, damping);
  }

  setTarget(vec3) {
    this.x.setTarget(vec3.x);
    this.y.setTarget(vec3.y);
    this.z.setTarget(vec3.z);
  }

  setPosition(vec3) {
    this.x.position = vec3.x;
    this.y.position = vec3.y;
    this.z.position = vec3.z;
  }

  update(dt, target) {
    if (target) this.setTarget(target);
    return new THREE.Vector3(
      this.x.update(dt),
      this.y.update(dt),
      this.z.update(dt)
    );
  }

  isSettled(threshold = 0.001) {
    return this.x.isSettled(threshold) &&
           this.y.isSettled(threshold) &&
           this.z.isSettled(threshold);
  }
}

/**
 * Base animation class with easing functions
 */
class Animation {
  constructor(duration, onComplete) {
    this.duration = duration;
    this.elapsed = 0;
    this.onComplete = onComplete;
    this.isComplete = false;
  }

  update(delta) {
    if (this.isComplete) return;

    this.elapsed += delta;
    const progress = Math.min(this.elapsed / this.duration, 1);

    this.animate(progress);

    if (progress >= 1) {
      this.isComplete = true;
      if (this.onComplete) this.onComplete();
    }
  }

  animate(progress) {
    // Override in subclasses
  }

  // Easing functions
  static easeOutQuad(t) {
    return t * (2 - t);
  }

  static easeInOutQuad(t) {
    return t < 0.5 ? 2 * t * t : -1 + (4 - 2 * t) * t;
  }

  static easeOutBack(t) {
    const c1 = 1.70158;
    const c3 = c1 + 1;
    return 1 + c3 * Math.pow(t - 1, 3) + c1 * Math.pow(t - 1, 2);
  }

  static easeOutElastic(t) {
    const c4 = (2 * Math.PI) / 3;
    return t === 0 ? 0 : t === 1 ? 1 :
      Math.pow(2, -10 * t) * Math.sin((t * 10 - 0.75) * c4) + 1;
  }
}

// ============================================================================
// Canvas Text Sprite Helper
// ============================================================================

/**
 * Creates a canvas-textured sprite for 3D text labels.
 * No font loading required — uses system fonts via canvas.
 */
function createTextSprite(text, color = 0xffffff, fontSize = 64) {
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d');
  const font = `bold ${fontSize}px Arial, Helvetica, sans-serif`;
  ctx.font = font;
  const metrics = ctx.measureText(text);
  const textWidth = metrics.width;

  canvas.width = Math.ceil(textWidth) + 20;
  canvas.height = fontSize + 20;

  // Redraw after resize
  ctx.font = font;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';

  // Glow / outline
  const c = new THREE.Color(color);
  const cssColor = `rgb(${Math.round(c.r * 255)}, ${Math.round(c.g * 255)}, ${Math.round(c.b * 255)})`;
  ctx.shadowColor = cssColor;
  ctx.shadowBlur = 12;
  ctx.fillStyle = cssColor;
  ctx.fillText(text, canvas.width / 2, canvas.height / 2);
  // Second pass for brighter center
  ctx.shadowBlur = 0;
  ctx.fillStyle = '#ffffff';
  ctx.fillText(text, canvas.width / 2, canvas.height / 2);

  const texture = new THREE.CanvasTexture(canvas);
  texture.needsUpdate = true;

  const material = new THREE.SpriteMaterial({
    map: texture,
    transparent: true,
    opacity: 1,
    depthWrite: false,
  });

  const sprite = new THREE.Sprite(material);
  // Scale sprite to reasonable world size
  const aspect = canvas.width / canvas.height;
  sprite.scale.set(aspect * 0.8, 0.8, 1);

  return sprite;
}

// ============================================================================
// Existing Animations (Card, Coin Transfer)
// ============================================================================

/**
 * Card reveal animation with flip effect
 */
export class CardRevealAnimation extends Animation {
  constructor(card, role, onComplete) {
    super(TIMING.cardFlip, onComplete);
    this.card = card;
    this.role = role;
    this.startRotation = card.rotation.x;
    this.hasFlipped = false;
  }

  animate(progress) {
    const easedProgress = Animation.easeInOutQuad(progress);

    // Flip rotation on Y axis
    this.card.rotation.y = easedProgress * Math.PI;

    // Scale effect at midpoint
    if (progress > 0.3 && progress < 0.7) {
      const scaleProgress = (progress - 0.3) / 0.4;
      const scale = 1 + Math.sin(scaleProgress * Math.PI) * 0.2;
      this.card.scale.set(scale, 1, scale);
    } else {
      this.card.scale.set(1, 1, 1);
    }

    // Change material at midpoint
    if (progress >= 0.5 && !this.hasFlipped) {
      this.hasFlipped = true;
      this.card.material.color.setHex(0x2a1a1a);
      this.card.material.emissive.setHex(0xff4444);
      this.card.material.emissiveIntensity = 0.15;
    }
  }
}

/**
 * Coin transfer animation with arc trajectory
 */
export class CoinTransferAnimation extends Animation {
  constructor(scene, fromPos, toPos, count, onComplete) {
    super(TIMING.coinTransfer, onComplete);
    this.scene = scene;
    this.fromPos = fromPos.clone();
    this.toPos = toPos.clone();
    this.count = count;

    // Create coin for animation
    const geometry = new THREE.CylinderGeometry(0.15, 0.15, 0.05, 16);
    const material = new THREE.MeshStandardMaterial({
      color: 0xd4af37,
      metalness: 0.8,
      roughness: 0.2,
      emissive: 0xd4af37,
      emissiveIntensity: 0.2,
    });
    this.coin = new THREE.Mesh(geometry, material);
    this.coin.position.copy(fromPos);
    scene.add(this.coin);

    // Create particle trail
    this.particles = this.createParticleTrail();
  }

  createParticleTrail() {
    const particleCount = 20;
    const geometry = new THREE.BufferGeometry();
    const positions = new Float32Array(particleCount * 3);

    for (let i = 0; i < particleCount; i++) {
      positions[i * 3] = this.fromPos.x;
      positions[i * 3 + 1] = this.fromPos.y;
      positions[i * 3 + 2] = this.fromPos.z;
    }

    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));

    const material = new THREE.PointsMaterial({
      color: 0xd4af37,
      size: 0.08,
      transparent: true,
      opacity: 0.6,
    });

    const particles = new THREE.Points(geometry, material);
    this.scene.add(particles);
    return particles;
  }

  animate(progress) {
    const easedProgress = Animation.easeOutQuad(progress);

    // Arc trajectory
    const x = THREE.MathUtils.lerp(this.fromPos.x, this.toPos.x, easedProgress);
    const z = THREE.MathUtils.lerp(this.fromPos.z, this.toPos.z, easedProgress);
    const arcHeight = 2;
    const y = THREE.MathUtils.lerp(this.fromPos.y, this.toPos.y, easedProgress) +
      Math.sin(progress * Math.PI) * arcHeight;

    this.coin.position.set(x, y, z);

    // Spin the coin
    this.coin.rotation.y += 0.3;

    // Update particle trail
    const positions = this.particles.geometry.attributes.position.array;
    for (let i = positions.length - 3; i >= 3; i -= 3) {
      positions[i] = positions[i - 3];
      positions[i + 1] = positions[i - 2];
      positions[i + 2] = positions[i - 1];
    }
    positions[0] = x;
    positions[1] = y;
    positions[2] = z;
    this.particles.geometry.attributes.position.needsUpdate = true;

    // Fade out particles near end
    if (progress > 0.8) {
      this.particles.material.opacity = (1 - progress) * 3;
    }
  }

  cleanup() {
    this.scene.remove(this.coin);
    this.scene.remove(this.particles);
    this.coin.geometry.dispose();
    this.coin.material.dispose();
    this.particles.geometry.dispose();
    this.particles.material.dispose();
  }
}

// ============================================================================
// Challenge Lightning (Improved)
// ============================================================================

/**
 * Challenge lightning effect between two players
 * Uses TubeGeometry for visible bolt thickness, more sparks
 */
export class ChallengeLightningAnimation extends Animation {
  constructor(scene, fromPos, toPos, onComplete) {
    super(TIMING.challengeLightning, onComplete);
    this.scene = scene;
    this.fromPos = fromPos.clone();
    this.toPos = toPos.clone();
    this.bolts = [];
    this.glows = [];
    this.sparks = null;
    this.impactRing = null;

    this.geometryPool = [];

    this.createLightningBolts();
    this.createGeometryPool();
    this.createSparks();
    this.createImpactRing();
  }

  createLightningBolts() {
    const boltCount = 5;
    const mainColor = 0xff9500;
    const secondaryColor = 0xffcc00;

    for (let b = 0; b < boltCount; b++) {
      const isMain = b < 2;
      const segments = isMain ? 12 : 8;
      const radius = isMain ? 0.06 : 0.03;

      const points = this.generateBoltPath(segments, isMain ? 0.6 : 1.0);

      // Use TubeGeometry for visible bolt thickness
      const curve = new THREE.CatmullRomCurve3(points);
      const geometry = new THREE.TubeGeometry(curve, segments * 2, radius, 6, false);
      const material = new THREE.MeshBasicMaterial({
        color: isMain ? mainColor : secondaryColor,
        transparent: true,
        opacity: isMain ? 1.0 : 0.6,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
      });

      const bolt = new THREE.Mesh(geometry, material);
      bolt.userData.points = points;
      bolt.userData.isMain = isMain;
      bolt.userData.regenerateRate = isMain ? 0.15 : 0.25;
      bolt.userData.radius = radius;
      bolt.userData.segments = segments;
      this.scene.add(bolt);
      this.bolts.push(bolt);
    }

    // Glow spheres at endpoints (larger)
    const glowGeometry = new THREE.SphereGeometry(0.5, 16, 16);
    const glowMaterial = new THREE.MeshBasicMaterial({
      color: mainColor,
      transparent: true,
      opacity: 0.8,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });

    const glow1 = new THREE.Mesh(glowGeometry, glowMaterial);
    glow1.position.copy(this.fromPos);
    this.scene.add(glow1);
    this.glows.push(glow1);

    const glow2 = new THREE.Mesh(glowGeometry.clone(), glowMaterial.clone());
    glow2.position.copy(this.toPos);
    this.scene.add(glow2);
    this.glows.push(glow2);

    // Center collision glow
    const centerGlow = new THREE.Mesh(
      new THREE.SphereGeometry(0.5, 16, 16),
      new THREE.MeshBasicMaterial({
        color: 0xffffff,
        transparent: true,
        opacity: 0,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
      })
    );
    const midPoint = this.fromPos.clone().add(this.toPos).multiplyScalar(0.5);
    centerGlow.position.copy(midPoint);
    this.scene.add(centerGlow);
    this.glows.push(centerGlow);
  }

  createGeometryPool() {
    // Pre-generate geometry variants per bolt to cycle through.
    // Include the initial geometry as variants[0] so it is disposed with the pool.
    const extraVariants = 3;
    for (const bolt of this.bolts) {
      const variants = [bolt.geometry];
      for (let v = 0; v < extraVariants; v++) {
        const points = this.generateBoltPath(
          bolt.userData.segments,
          bolt.userData.isMain ? 0.6 : 1.0
        );
        const curve = new THREE.CatmullRomCurve3(points);
        const geometry = new THREE.TubeGeometry(
          curve, bolt.userData.segments * 2, bolt.userData.radius, 6, false
        );
        variants.push(geometry);
      }
      this.geometryPool.push({ variants, index: 0 });
    }
  }

  generateBoltPath(segments, randomness) {
    const points = [];
    const direction = this.toPos.clone().sub(this.fromPos);
    const length = direction.length();
    direction.normalize();

    const up = new THREE.Vector3(0, 1, 0);
    const perp1 = direction.clone().cross(up).normalize();
    const perp2 = direction.clone().cross(perp1).normalize();

    for (let i = 0; i <= segments; i++) {
      const t = i / segments;
      const basePos = this.fromPos.clone().add(direction.clone().multiplyScalar(t * length));

      if (i > 0 && i < segments) {
        const offset1 = (Math.random() - 0.5) * randomness;
        const offset2 = (Math.random() - 0.5) * randomness;
        basePos.add(perp1.clone().multiplyScalar(offset1));
        basePos.add(perp2.clone().multiplyScalar(offset2));
      }

      points.push(basePos);
    }

    return points;
  }

  createSparks() {
    const sparkCount = 80;
    const midPoint = this.fromPos.clone().add(this.toPos).multiplyScalar(0.5);

    const geometry = new THREE.BufferGeometry();
    const positions = new Float32Array(sparkCount * 3);
    const velocities = [];

    for (let i = 0; i < sparkCount; i++) {
      positions[i * 3] = midPoint.x;
      positions[i * 3 + 1] = midPoint.y;
      positions[i * 3 + 2] = midPoint.z;

      velocities.push(new THREE.Vector3(
        (Math.random() - 0.5) * 8,
        (Math.random() - 0.5) * 8,
        (Math.random() - 0.5) * 8
      ));
    }

    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));

    const material = new THREE.PointsMaterial({
      color: 0xffdd00,
      size: 0.12,
      transparent: true,
      opacity: 0,
    });

    this.sparks = new THREE.Points(geometry, material);
    this.sparks.userData.velocities = velocities;
    this.sparks.userData.midPoint = midPoint;
    this.scene.add(this.sparks);
  }

  createImpactRing() {
    const midPoint = this.fromPos.clone().add(this.toPos).multiplyScalar(0.5);

    const geometry = new THREE.RingGeometry(0.1, 0.15, 32);
    const material = new THREE.MeshBasicMaterial({
      color: 0xff9500,
      transparent: true,
      opacity: 0,
      side: THREE.DoubleSide,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });

    this.impactRing = new THREE.Mesh(geometry, material);
    this.impactRing.position.copy(midPoint);
    const direction = this.toPos.clone().sub(this.fromPos).normalize();
    this.impactRing.lookAt(midPoint.clone().add(direction));
    this.scene.add(this.impactRing);
  }

  animate(progress) {
    const impactPhase = Math.max(0, Math.min((progress - 0.2) * 2, 1));
    const fadePhase = Math.max(0, (progress - 0.6) / 0.4);
    const flicker = Math.sin(progress * Math.PI * 15) * 0.3 + 0.7;
    const flash = progress < 0.3 ? Math.sin(progress * Math.PI * 30) * 0.5 + 0.5 : 0;

    // Animate bolts
    for (let b = 0; b < this.bolts.length; b++) {
      const bolt = this.bolts[b];
      const baseOpacity = bolt.userData.isMain ? 1.0 : 0.6;
      bolt.material.opacity = baseOpacity * flicker * (1 - fadePhase * 0.8);

      // Cycle pre-allocated geometry from pool instead of creating new
      if (Math.random() < bolt.userData.regenerateRate) {
        const poolEntry = this.geometryPool[b];
        poolEntry.index = (poolEntry.index + 1) % poolEntry.variants.length;
        bolt.geometry = poolEntry.variants[poolEntry.index];
      }
    }

    // Pulse glow spheres
    for (let i = 0; i < this.glows.length; i++) {
      const glow = this.glows[i];
      const isCenter = i === 2;

      if (isCenter) {
        const scale = 0.5 + impactPhase * 2 * (1 - fadePhase);
        glow.scale.set(scale, scale, scale);
        glow.material.opacity = impactPhase * 0.8 * (1 - fadePhase);
      } else {
        const scale = 1 + flicker * 0.4 + flash * 0.5;
        glow.scale.set(scale, scale, scale);
        glow.material.opacity = 0.8 * (1 - fadePhase * 0.7);
      }
    }

    // Animate sparks
    if (impactPhase > 0) {
      const positions = this.sparks.geometry.attributes.position.array;
      const velocities = this.sparks.userData.velocities;
      const midPoint = this.sparks.userData.midPoint;
      const sparkTime = impactPhase * 0.5;

      for (let i = 0; i < velocities.length; i++) {
        const vel = velocities[i];
        positions[i * 3] = midPoint.x + vel.x * sparkTime;
        positions[i * 3 + 1] = midPoint.y + vel.y * sparkTime - 2 * sparkTime * sparkTime;
        positions[i * 3 + 2] = midPoint.z + vel.z * sparkTime;
      }

      this.sparks.geometry.attributes.position.needsUpdate = true;
      this.sparks.material.opacity = 0.8 * impactPhase * (1 - fadePhase);
    }

    // Impact ring expansion
    if (impactPhase > 0) {
      const ringScale = 1 + impactPhase * 5;
      this.impactRing.scale.set(ringScale, ringScale, 1);
      this.impactRing.material.opacity = 0.6 * impactPhase * (1 - fadePhase);
    }
  }

  cleanup() {
    for (const bolt of this.bolts) {
      this.scene.remove(bolt);
      bolt.material.dispose();
    }
    // Dispose all pool geometries
    for (const poolEntry of this.geometryPool) {
      for (const geometry of poolEntry.variants) {
        geometry.dispose();
      }
    }
    for (const glow of this.glows) {
      this.scene.remove(glow);
      glow.geometry.dispose();
      glow.material.dispose();
    }
    if (this.sparks) {
      this.scene.remove(this.sparks);
      this.sparks.geometry.dispose();
      this.sparks.material.dispose();
    }
    if (this.impactRing) {
      this.scene.remove(this.impactRing);
      this.impactRing.geometry.dispose();
      this.impactRing.material.dispose();
    }
  }
}

// ============================================================================
// Block Barrier (Improved — larger shield/rings)
// ============================================================================

/**
 * Block shield/barrier effect
 */
export class BlockBarrierAnimation extends Animation {
  constructor(scene, playerPos, onComplete) {
    super(TIMING.blockBarrier, onComplete);
    this.scene = scene;
    this.playerPos = playerPos.clone();
    this.barrier = null;
    this.rings = [];

    this.createBarrier();
  }

  createBarrier() {
    // Hexagonal shield (larger radius)
    const shape = new THREE.Shape();
    const sides = 6;
    const radius = 1.8;
    for (let i = 0; i < sides; i++) {
      const angle = (i / sides) * Math.PI * 2 + Math.PI / 6;
      const x = Math.cos(angle) * radius;
      const y = Math.sin(angle) * radius;
      if (i === 0) shape.moveTo(x, y);
      else shape.lineTo(x, y);
    }
    shape.closePath();

    const geometry = new THREE.ShapeGeometry(shape);
    const material = new THREE.MeshBasicMaterial({
      color: 0x00e5ff,
      transparent: true,
      opacity: 0.5,
      side: THREE.DoubleSide,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });

    this.barrier = new THREE.Mesh(geometry, material);
    this.barrier.position.copy(this.playerPos);
    this.barrier.position.y += 1;
    this.barrier.rotation.y = Math.PI / 2;
    this.barrier.scale.set(0, 0, 0);
    this.scene.add(this.barrier);

    // Expanding rings (larger)
    for (let i = 0; i < 3; i++) {
      const ringGeometry = new THREE.RingGeometry(1.5, 1.65, 6);
      const ringMaterial = new THREE.MeshBasicMaterial({
        color: 0x00e5ff,
        transparent: true,
        opacity: 0.7,
        side: THREE.DoubleSide,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
      });
      const ring = new THREE.Mesh(ringGeometry, ringMaterial);
      ring.position.copy(this.playerPos);
      ring.position.y += 1;
      ring.rotation.y = Math.PI / 2;
      ring.userData.delay = i * 0.1;
      this.scene.add(ring);
      this.rings.push(ring);
    }
  }

  animate(progress) {
    const easedProgress = Animation.easeOutBack(progress);

    // Shield expansion
    const scale = easedProgress * 1.5;
    this.barrier.scale.set(scale, scale, 1);
    this.barrier.material.opacity = 0.5 * (1 - progress * 0.5);

    // Rings expansion
    for (let i = 0; i < this.rings.length; i++) {
      const ring = this.rings[i];
      const ringProgress = Math.max(0, progress - ring.userData.delay);
      if (ringProgress > 0) {
        const ringScale = Animation.easeOutQuad(Math.min(ringProgress * 2, 1)) * (1 + i * 0.3);
        ring.scale.set(ringScale, ringScale, 1);
        ring.material.opacity = 0.7 * (1 - ringProgress);
      }
    }

    // Pulse effect
    const pulse = Math.sin(progress * Math.PI * 4) * 0.1 + 1;
    this.barrier.scale.multiplyScalar(pulse);
  }

  cleanup() {
    this.scene.remove(this.barrier);
    this.barrier.geometry.dispose();
    this.barrier.material.dispose();

    for (const ring of this.rings) {
      this.scene.remove(ring);
      ring.geometry.dispose();
      ring.material.dispose();
    }
  }
}

// ============================================================================
// Elimination (Improved — rising effect, more particles)
// ============================================================================

/**
 * Player elimination dissolution effect
 * Player rises 1.4 units before dissolving; 500 particles
 */
export class EliminationAnimation extends Animation {
  constructor(scene, player, onComplete) {
    super(TIMING.eliminationDissolve, onComplete);
    this.scene = scene;
    this.player = player;
    this.instancedMesh = null;
    this.particleData = [];
    this.playerColor = player.color || 0x00ffcc;
    this.startY = player.humanoid.position.y;
    this.startScaleY = player.humanoid.scale.y;

    this.createDissolveParticles();
  }

  createDissolveParticles() {
    const particleCount = 500;
    const playerPos = this.player.group.position;

    const particleGeometry = new THREE.BoxGeometry(0.06, 0.06, 0.06);
    const particleMaterial = new THREE.MeshBasicMaterial({
      color: this.playerColor,
      transparent: true,
      opacity: 0.9,
    });

    this.instancedMesh = new THREE.InstancedMesh(
      particleGeometry,
      particleMaterial,
      particleCount
    );

    const dummy = new THREE.Object3D();

    for (let i = 0; i < particleCount; i++) {
      const theta = Math.random() * Math.PI * 2;
      const phi = Math.random() * Math.PI;
      const r = 0.3 + Math.random() * 0.5;

      const startPos = new THREE.Vector3(
        playerPos.x + Math.sin(phi) * Math.cos(theta) * r,
        playerPos.y + 0.5 + Math.cos(phi) * r + Math.random() * 1.5,
        playerPos.z + Math.sin(phi) * Math.sin(theta) * r
      );

      const vel = new THREE.Vector3(
        (Math.random() - 0.5) * 3,
        2 + Math.random() * 4,
        (Math.random() - 0.5) * 3
      );

      const rotVel = new THREE.Vector3(
        (Math.random() - 0.5) * 10,
        (Math.random() - 0.5) * 10,
        (Math.random() - 0.5) * 10
      );

      this.particleData.push({
        startPos: startPos.clone(),
        velocity: vel,
        rotVel: rotVel,
        scale: 0.5 + Math.random() * 1.0,
        delay: Math.random() * 0.2,
      });

      dummy.position.copy(startPos);
      dummy.scale.setScalar(0);
      dummy.updateMatrix();
      this.instancedMesh.setMatrixAt(i, dummy.matrix);

      const colorVariation = new THREE.Color(this.playerColor);
      colorVariation.offsetHSL(0, 0, (Math.random() - 0.5) * 0.3);
      this.instancedMesh.setColorAt(i, colorVariation);
    }

    this.instancedMesh.instanceMatrix.needsUpdate = true;
    if (this.instancedMesh.instanceColor) {
      this.instancedMesh.instanceColor.needsUpdate = true;
    }

    this.scene.add(this.instancedMesh);
  }

  animate(progress) {
    // Rising effect: humanoid rises 0.8 units in first 40% of animation
    const riseProgress = Math.min(progress / 0.4, 1);
    const riseAmount = Animation.easeOutQuad(riseProgress) * 1.4;
    this.player.humanoid.position.y = this.startY + riseAmount;
    const stretchFactor = 1.0 + riseProgress * 0.15;
    this.player.humanoid.scale.y = this.startScaleY * stretchFactor;

    // Fade out player with glitch effect
    const glitchIntensity = Math.sin(progress * 50) * 0.2 * (1 - progress);
    this.player.humanoid.traverse((child) => {
      if (child.isMesh && child.material) {
        if (child.material.uniforms) {
          const baseOpacity = Math.max(0, 0.7 - progress * 1.2);
          child.material.uniforms.glowIntensity.value = baseOpacity + glitchIntensity;
        } else if (child.material.opacity !== undefined) {
          child.material.opacity = Math.max(0, 0.7 - progress * 1.2);
        }
      }
    });

    // Animate particles
    const dummy = new THREE.Object3D();
    const gravity = -2;

    for (let i = 0; i < this.particleData.length; i++) {
      const data = this.particleData[i];
      const localProgress = Math.max(0, (progress - data.delay) / (1 - data.delay));

      if (localProgress <= 0) {
        dummy.scale.setScalar(0);
      } else {
        const t = localProgress * TIMING.eliminationDissolve;
        const pos = data.startPos.clone();
        pos.x += data.velocity.x * t;
        pos.y += data.velocity.y * t + 0.5 * gravity * t * t;
        pos.z += data.velocity.z * t;

        dummy.position.copy(pos);
        dummy.rotation.x = data.rotVel.x * t;
        dummy.rotation.y = data.rotVel.y * t;
        dummy.rotation.z = data.rotVel.z * t;

        const scaleProgress = 1 - Math.pow(localProgress, 2);
        dummy.scale.setScalar(data.scale * scaleProgress * 0.20);
      }

      dummy.updateMatrix();
      this.instancedMesh.setMatrixAt(i, dummy.matrix);
    }

    this.instancedMesh.instanceMatrix.needsUpdate = true;
    this.instancedMesh.material.opacity = 0.9 * (1 - Math.pow(progress, 2));
  }

  cleanup() {
    this.scene.remove(this.instancedMesh);
    this.instancedMesh.geometry.dispose();
    this.instancedMesh.material.dispose();
  }
}

// ============================================================================
// Victory Burst
// ============================================================================

/**
 * Victory burst particle effect for game winner
 */
export class VictoryBurstAnimation extends Animation {
  constructor(scene, playerPos, playerColor, onComplete) {
    super(TIMING.victoryBurst, onComplete);
    this.scene = scene;
    this.playerPos = playerPos.clone();
    this.playerColor = playerColor;
    this.particles = null;
    this.rings = [];

    this.createBurstParticles();
    this.createVictoryRings();
  }

  createBurstParticles() {
    const particleCount = 150;
    const geometry = new THREE.BufferGeometry();
    const positions = new Float32Array(particleCount * 3);
    const colors = new Float32Array(particleCount * 3);
    const velocities = [];

    const baseColor = new THREE.Color(this.playerColor);
    const goldColor = new THREE.Color(0xffd700);

    for (let i = 0; i < particleCount; i++) {
      positions[i * 3] = this.playerPos.x;
      positions[i * 3 + 1] = this.playerPos.y + 1.5;
      positions[i * 3 + 2] = this.playerPos.z;

      const color = Math.random() > 0.5 ? baseColor : goldColor;
      colors[i * 3] = color.r;
      colors[i * 3 + 1] = color.g;
      colors[i * 3 + 2] = color.b;

      const angle = Math.random() * Math.PI * 2;
      const upwardBias = 2 + Math.random() * 4;
      const outwardSpeed = 1 + Math.random() * 3;

      velocities.push(new THREE.Vector3(
        Math.cos(angle) * outwardSpeed,
        upwardBias,
        Math.sin(angle) * outwardSpeed
      ));
    }

    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    geometry.setAttribute('color', new THREE.BufferAttribute(colors, 3));

    const material = new THREE.PointsMaterial({
      size: 0.12,
      transparent: true,
      opacity: 1,
      vertexColors: true,
    });

    this.particles = new THREE.Points(geometry, material);
    this.particles.userData.velocities = velocities;
    this.scene.add(this.particles);
  }

  createVictoryRings() {
    for (let i = 0; i < 3; i++) {
      const geometry = new THREE.TorusGeometry(0.5, 0.05, 8, 32);
      const material = new THREE.MeshBasicMaterial({
        color: i === 1 ? 0xffd700 : this.playerColor,
        transparent: true,
        opacity: 0,
      });

      const ring = new THREE.Mesh(geometry, material);
      ring.position.copy(this.playerPos);
      ring.position.y += 1.5;
      ring.rotation.x = Math.PI / 2;
      ring.userData.delay = i * 0.15;
      ring.userData.index = i;

      this.scene.add(ring);
      this.rings.push(ring);
    }
  }

  animate(progress) {
    const gravity = -8;

    const positions = this.particles.geometry.attributes.position.array;
    const velocities = this.particles.userData.velocities;
    const t = progress * TIMING.victoryBurst;

    for (let i = 0; i < velocities.length; i++) {
      const vel = velocities[i];
      positions[i * 3] = this.playerPos.x + vel.x * t;
      positions[i * 3 + 1] = this.playerPos.y + 1.5 + vel.y * t + 0.5 * gravity * t * t;
      positions[i * 3 + 2] = this.playerPos.z + vel.z * t;
    }

    this.particles.geometry.attributes.position.needsUpdate = true;
    this.particles.material.opacity = 1 - Math.pow(progress, 2);

    for (const ring of this.rings) {
      const localProgress = Math.max(0, (progress - ring.userData.delay) / (1 - ring.userData.delay));
      if (localProgress > 0) {
        const scale = 1 + localProgress * 4;
        ring.scale.set(scale, scale, 1);
        ring.material.opacity = 0.8 * (1 - localProgress);
        ring.position.y = this.playerPos.y + 1.5 + localProgress * 2;
      }
    }
  }

  cleanup() {
    this.scene.remove(this.particles);
    this.particles.geometry.dispose();
    this.particles.material.dispose();

    for (const ring of this.rings) {
      this.scene.remove(ring);
      ring.geometry.dispose();
      ring.material.dispose();
    }
  }
}

// ============================================================================
// NEW: Action Label Animation
// ============================================================================

/**
 * 3D text label that pops in, floats up, and fades out.
 * Uses canvas-textured sprite (no font loading).
 */
export class ActionLabelAnimation extends Animation {
  constructor(scene, position, text, color = 0xffffff, onComplete) {
    super(TIMING.actionLabel, onComplete);
    this.scene = scene;
    this.startPos = position.clone();
    this.sprite = createTextSprite(text, color);
    this.sprite.position.copy(this.startPos);
    this.sprite.scale.set(0, 0, 1);
    this.scene.add(this.sprite);
  }

  animate(progress) {
    // Pop-in during first 20%
    const popIn = Math.min(progress / 0.2, 1);
    const scaleEased = Animation.easeOutBack(popIn);
    const aspect = this.sprite.material.map.image.width / this.sprite.material.map.image.height;
    const s = scaleEased * 0.8;
    this.sprite.scale.set(aspect * s, s, 1);

    // Float upward
    this.sprite.position.y = this.startPos.y + progress * 1.5;

    // Fade out in last 30%
    if (progress > 0.7) {
      this.sprite.material.opacity = (1 - progress) / 0.3;
    }
  }

  cleanup() {
    this.scene.remove(this.sprite);
    this.sprite.material.map.dispose();
    this.sprite.material.dispose();
  }
}

// ============================================================================
// NEW: Steal Beam Animation
// ============================================================================

/**
 * Gold energy beam + coins flying from target to stealer
 */
export class StealBeamAnimation extends Animation {
  constructor(scene, fromPos, toPos, onComplete) {
    super(TIMING.stealBeam, onComplete);
    this.scene = scene;
    this.fromPos = fromPos.clone();
    this.toPos = toPos.clone();
    this.beam = null;
    this.coins = [];
    this.beamGlow = null;

    this.createBeam();
    this.createCoins();
  }

  createBeam() {
    // Beam from actor to target
    const direction = this.toPos.clone().sub(this.fromPos);
    const midPoint = this.fromPos.clone().add(direction.clone().multiplyScalar(0.5));
    midPoint.y += 0.5; // slight arc

    const curve = new THREE.QuadraticBezierCurve3(this.fromPos, midPoint, this.toPos);
    const geometry = new THREE.TubeGeometry(curve, 20, 0.05, 8, false);
    const material = new THREE.MeshBasicMaterial({
      color: 0xd4af37,
      transparent: true,
      opacity: 0,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });

    this.beam = new THREE.Mesh(geometry, material);
    this.scene.add(this.beam);

    // Glow around beam
    const glowGeometry = new THREE.TubeGeometry(curve, 20, 0.12, 8, false);
    const glowMaterial = new THREE.MeshBasicMaterial({
      color: 0xd4af37,
      transparent: true,
      opacity: 0,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    this.beamGlow = new THREE.Mesh(glowGeometry, glowMaterial);
    this.scene.add(this.beamGlow);
  }

  createCoins() {
    const coinGeometry = new THREE.CylinderGeometry(0.10, 0.10, 0.04, 12);
    const coinMaterial = new THREE.MeshBasicMaterial({
      color: 0xd4af37,
      transparent: true,
      opacity: 0,
    });

    for (let i = 0; i < 3; i++) {
      const coin = new THREE.Mesh(coinGeometry.clone(), coinMaterial.clone());
      coin.userData.delay = i * 0.15;
      this.scene.add(coin);
      this.coins.push(coin);
    }
  }

  animate(progress) {
    // Beam appears in first 30%, fades after 60%
    const beamIn = Math.min(progress / 0.3, 1);
    const beamFade = Math.max(0, (progress - 0.6) / 0.4);
    this.beam.material.opacity = 0.8 * beamIn * (1 - beamFade);
    this.beamGlow.material.opacity = 0.3 * beamIn * (1 - beamFade);

    // Coins fly from target to stealer (reverse direction)
    for (let i = 0; i < this.coins.length; i++) {
      const coin = this.coins[i];
      const coinProgress = Math.max(0, (progress - coin.userData.delay) / (1 - coin.userData.delay));

      if (coinProgress > 0 && coinProgress <= 1) {
        const eased = Animation.easeInOutQuad(coinProgress);
        // Fly from target to actor
        const x = THREE.MathUtils.lerp(this.toPos.x, this.fromPos.x, eased);
        const z = THREE.MathUtils.lerp(this.toPos.z, this.fromPos.z, eased);
        const y = THREE.MathUtils.lerp(this.toPos.y, this.fromPos.y, eased) +
          Math.sin(coinProgress * Math.PI) * 1.5;

        coin.position.set(x, y + 1, z);
        coin.rotation.y += 0.4;
        coin.material.opacity = coinProgress < 0.8 ? 0.9 : 0.9 * ((1 - coinProgress) / 0.2);
      }
    }
  }

  cleanup() {
    this.scene.remove(this.beam);
    this.beam.geometry.dispose();
    this.beam.material.dispose();
    this.scene.remove(this.beamGlow);
    this.beamGlow.geometry.dispose();
    this.beamGlow.material.dispose();
    for (const coin of this.coins) {
      this.scene.remove(coin);
      coin.geometry.dispose();
      coin.material.dispose();
    }
  }
}

// ============================================================================
// NEW: Assassinate Slash Animation
// ============================================================================

/**
 * Red blade shape + particle trail from attacker to target
 */
export class AssassinateSlashAnimation extends Animation {
  constructor(scene, fromPos, toPos, onComplete) {
    super(TIMING.assassinateSlash, onComplete);
    this.scene = scene;
    this.fromPos = fromPos.clone();
    this.toPos = toPos.clone();
    this.blade = null;
    this.trail = null;
    this.impactSphere = null;

    this.createBlade();
    this.createTrail();
    this.createImpact();
  }

  createBlade() {
    // Blade shape
    const shape = new THREE.Shape();
    shape.moveTo(0, 0.4);
    shape.lineTo(0.08, 0.1);
    shape.lineTo(0.05, -0.3);
    shape.lineTo(0, -0.4);
    shape.lineTo(-0.05, -0.3);
    shape.lineTo(-0.08, 0.1);
    shape.closePath();

    const geometry = new THREE.ShapeGeometry(shape);
    const material = new THREE.MeshBasicMaterial({
      color: 0xff2222,
      transparent: true,
      opacity: 0,
      side: THREE.DoubleSide,
    });

    this.blade = new THREE.Mesh(geometry, material);
    this.blade.position.copy(this.fromPos);
    this.blade.position.y += 1.5;
    this.scene.add(this.blade);
  }

  createTrail() {
    const particleCount = 40;
    const geometry = new THREE.BufferGeometry();
    const positions = new Float32Array(particleCount * 3);

    for (let i = 0; i < particleCount; i++) {
      positions[i * 3] = this.fromPos.x;
      positions[i * 3 + 1] = this.fromPos.y + 1.5;
      positions[i * 3 + 2] = this.fromPos.z;
    }

    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));

    const material = new THREE.PointsMaterial({
      color: 0xff4444,
      size: 0.1,
      transparent: true,
      opacity: 0,
      blending: THREE.AdditiveBlending,
    });

    this.trail = new THREE.Points(geometry, material);
    this.scene.add(this.trail);
  }

  createImpact() {
    const geometry = new THREE.SphereGeometry(0.3, 16, 16);
    const material = new THREE.MeshBasicMaterial({
      color: 0xff0000,
      transparent: true,
      opacity: 0,
    });
    this.impactSphere = new THREE.Mesh(geometry, material);
    this.impactSphere.position.copy(this.toPos);
    this.impactSphere.position.y += 1.5;
    this.scene.add(this.impactSphere);
  }

  animate(progress) {
    // Blade travels from actor to target in first 60%
    const travelPhase = Math.min(progress / 0.6, 1);
    const eased = Animation.easeInOutQuad(travelPhase);

    const x = THREE.MathUtils.lerp(this.fromPos.x, this.toPos.x, eased);
    const z = THREE.MathUtils.lerp(this.fromPos.z, this.toPos.z, eased);
    const y = THREE.MathUtils.lerp(this.fromPos.y, this.toPos.y, eased) + 1.5;

    this.blade.position.set(x, y, z);
    this.blade.rotation.z += 0.3;
    this.blade.material.opacity = travelPhase < 0.9 ? 0.9 : 0.9 * (1 - (travelPhase - 0.9) / 0.1);

    // Look at target
    this.blade.lookAt(this.toPos.x, this.toPos.y + 1.5, this.toPos.z);

    // Trail follows blade
    const positions = this.trail.geometry.attributes.position.array;
    for (let i = positions.length - 3; i >= 3; i -= 3) {
      positions[i] = positions[i - 3];
      positions[i + 1] = positions[i - 2];
      positions[i + 2] = positions[i - 1];
    }
    positions[0] = x;
    positions[1] = y;
    positions[2] = z;
    this.trail.geometry.attributes.position.needsUpdate = true;
    this.trail.material.opacity = 0.6 * (1 - Math.max(0, (progress - 0.5) / 0.5));

    // Impact expansion on hit
    if (progress > 0.55) {
      const impactProgress = (progress - 0.55) / 0.45;
      const scale = 1 + impactProgress * 3;
      this.impactSphere.scale.set(scale, scale, scale);
      this.impactSphere.material.opacity = 0.6 * (1 - impactProgress);
    }
  }

  cleanup() {
    this.scene.remove(this.blade);
    this.blade.geometry.dispose();
    this.blade.material.dispose();
    this.scene.remove(this.trail);
    this.trail.geometry.dispose();
    this.trail.material.dispose();
    this.scene.remove(this.impactSphere);
    this.impactSphere.geometry.dispose();
    this.impactSphere.material.dispose();
  }
}

// ============================================================================
// NEW: Coup Strike Animation
// ============================================================================

/**
 * Vertical energy column striking down on target from above
 */
export class CoupStrikeAnimation extends Animation {
  constructor(scene, targetPos, onComplete) {
    super(TIMING.coupStrike, onComplete);
    this.scene = scene;
    this.targetPos = targetPos.clone();
    this.beam = null;
    this.impactRing = null;
    this.sparks = null;

    this.createBeam();
    this.createImpactRing();
    this.createSparks();
  }

  createBeam() {
    const geometry = new THREE.CylinderGeometry(0.15, 0.4, 8, 12);
    const material = new THREE.MeshBasicMaterial({
      color: 0xff4444,
      transparent: true,
      opacity: 0,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    this.beam = new THREE.Mesh(geometry, material);
    this.beam.position.copy(this.targetPos);
    this.beam.position.y += 5;
    this.scene.add(this.beam);
  }

  createImpactRing() {
    const geometry = new THREE.RingGeometry(0.2, 0.4, 32);
    const material = new THREE.MeshBasicMaterial({
      color: 0xff6600,
      transparent: true,
      opacity: 0,
      side: THREE.DoubleSide,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    this.impactRing = new THREE.Mesh(geometry, material);
    this.impactRing.position.copy(this.targetPos);
    this.impactRing.position.y += 0.5;
    this.impactRing.rotation.x = -Math.PI / 2;
    this.scene.add(this.impactRing);
  }

  createSparks() {
    const sparkCount = 60;
    const geometry = new THREE.BufferGeometry();
    const positions = new Float32Array(sparkCount * 3);
    const velocities = [];

    for (let i = 0; i < sparkCount; i++) {
      positions[i * 3] = this.targetPos.x;
      positions[i * 3 + 1] = this.targetPos.y + 1;
      positions[i * 3 + 2] = this.targetPos.z;

      const angle = Math.random() * Math.PI * 2;
      const speed = 2 + Math.random() * 4;
      velocities.push(new THREE.Vector3(
        Math.cos(angle) * speed,
        1 + Math.random() * 3,
        Math.sin(angle) * speed
      ));
    }

    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));

    const material = new THREE.PointsMaterial({
      color: 0xff8800,
      size: 0.10,
      transparent: true,
      opacity: 0,
    });

    this.sparks = new THREE.Points(geometry, material);
    this.sparks.userData.velocities = velocities;
    this.scene.add(this.sparks);
  }

  animate(progress) {
    // Beam descends in first 40%
    const strikePhase = Math.min(progress / 0.4, 1);
    const strikeEased = Animation.easeInOutQuad(strikePhase);

    // Move beam down
    this.beam.position.y = this.targetPos.y + 5 - strikeEased * 4;
    this.beam.material.opacity = strikePhase < 0.8 ? 0.8 * strikePhase : 0.8;

    // Fade beam after impact
    if (progress > 0.4) {
      const fadeProgress = (progress - 0.4) / 0.6;
      this.beam.material.opacity = 0.8 * (1 - fadeProgress);
    }

    // Impact ring at strike moment
    if (progress > 0.35) {
      const impactProgress = (progress - 0.35) / 0.65;
      const ringScale = 1 + impactProgress * 6;
      this.impactRing.scale.set(ringScale, ringScale, 1);
      this.impactRing.material.opacity = 0.7 * (1 - impactProgress);
    }

    // Sparks after impact
    if (progress > 0.38) {
      const sparkProgress = (progress - 0.38) / 0.62;
      const positions = this.sparks.geometry.attributes.position.array;
      const velocities = this.sparks.userData.velocities;
      const t = sparkProgress * 0.6;

      for (let i = 0; i < velocities.length; i++) {
        const vel = velocities[i];
        positions[i * 3] = this.targetPos.x + vel.x * t;
        positions[i * 3 + 1] = this.targetPos.y + 1 + vel.y * t - 4 * t * t;
        positions[i * 3 + 2] = this.targetPos.z + vel.z * t;
      }

      this.sparks.geometry.attributes.position.needsUpdate = true;
      this.sparks.material.opacity = 0.8 * (1 - sparkProgress);
    }
  }

  cleanup() {
    this.scene.remove(this.beam);
    this.beam.geometry.dispose();
    this.beam.material.dispose();
    this.scene.remove(this.impactRing);
    this.impactRing.geometry.dispose();
    this.impactRing.material.dispose();
    this.scene.remove(this.sparks);
    this.sparks.geometry.dispose();
    this.sparks.material.dispose();
  }
}

// ============================================================================
// NEW: Coin Gain Animation
// ============================================================================

/**
 * Coins materializing at a player position (income/tax/foreign_aid)
 */
export class CoinGainAnimation extends Animation {
  constructor(scene, position, coinCount, onComplete) {
    super(TIMING.coinGain, onComplete);
    this.scene = scene;
    this.position = position.clone();
    this.coinCount = coinCount;
    this.coins = [];

    this.createCoins();
  }

  createCoins() {
    const geometry = new THREE.CylinderGeometry(0.12, 0.12, 0.04, 16);
    const material = new THREE.MeshBasicMaterial({
      color: 0xd4af37,
      transparent: true,
      opacity: 0,
    });

    for (let i = 0; i < this.coinCount; i++) {
      const coin = new THREE.Mesh(geometry.clone(), material.clone());
      coin.position.copy(this.position);
      coin.position.y += 3; // start above player
      coin.position.x += (Math.random() - 0.5) * 0.4;
      coin.position.z += (Math.random() - 0.5) * 0.4;
      coin.userData.startY = coin.position.y;
      coin.userData.targetY = this.position.y + 1.2 + i * 0.06;
      coin.userData.delay = i * 0.08;
      this.scene.add(coin);
      this.coins.push(coin);
    }
  }

  animate(progress) {
    for (const coin of this.coins) {
      const localProgress = Math.max(0, (progress - coin.userData.delay) / (1 - coin.userData.delay));

      if (localProgress > 0) {
        const eased = Animation.easeOutBack(Math.min(localProgress, 1));
        coin.position.y = THREE.MathUtils.lerp(coin.userData.startY, coin.userData.targetY, eased);
        coin.rotation.y += 0.2;

        // Fade in then out
        if (localProgress < 0.3) {
          coin.material.opacity = localProgress / 0.3;
        } else if (localProgress > 0.7) {
          coin.material.opacity = (1 - localProgress) / 0.3;
        } else {
          coin.material.opacity = 1;
        }
      }
    }
  }

  cleanup() {
    for (const coin of this.coins) {
      this.scene.remove(coin);
      coin.geometry.dispose();
      coin.material.dispose();
    }
  }
}

// ============================================================================
// Animation Manager
// ============================================================================

/**
 * Animation manager to handle multiple concurrent animations
 */
export class AnimationManager {
  constructor() {
    this.animations = [];
    this.pendingCleanups = [];
  }

  add(animation) {
    this.animations.push(animation);
  }

  update(delta) {
    // Process cleanups
    for (const cleanup of this.pendingCleanups) {
      cleanup();
    }
    this.pendingCleanups = [];

    // Update active animations
    this.animations = this.animations.filter((anim) => {
      anim.update(delta);
      if (anim.isComplete) {
        if (anim.cleanup) {
          this.pendingCleanups.push(() => anim.cleanup());
        }
        return false;
      }
      return true;
    });
  }

  clear() {
    for (const anim of this.animations) {
      if (anim.cleanup) anim.cleanup();
    }
    this.animations = [];
  }

  hasActiveAnimations() {
    return this.animations.length > 0;
  }

  /**
   * Wait for all current animations to complete
   */
  waitForCompletion() {
    return new Promise((resolve) => {
      const check = () => {
        if (this.animations.length === 0) {
          resolve();
        } else {
          requestAnimationFrame(check);
        }
      };
      check();
    });
  }
}
