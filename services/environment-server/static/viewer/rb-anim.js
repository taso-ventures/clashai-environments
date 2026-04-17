/**
 * Red Button Animation Helpers
 *
 * Spring interpolation, shockwave ring, and victory particle burst.
 */

import * as THREE from 'three';

/**
 * Damped spring for smooth number transitions.
 * Copy of the pattern from coup-anim.js.
 */
export class SpringValue {
  /**
   * @param {number} initial - Starting value
   * @param {object} [opts]
   * @param {number} [opts.stiffness=180]
   * @param {number} [opts.damping=12]
   */
  constructor(initial, { stiffness = 180, damping = 12 } = {}) {
    this.value = initial;
    this.target = initial;
    this.velocity = 0;
    this.stiffness = stiffness;
    this.damping = damping;
  }

  /** Set a new target to animate towards. */
  setTarget(target) {
    this.target = target;
  }

  /** Snap immediately to a value (no animation). */
  snap(value) {
    this.value = value;
    this.target = value;
    this.velocity = 0;
  }

  /** Tick the spring forward by `dt` seconds. Returns current value. */
  update(dt) {
    const displacement = this.value - this.target;
    const springForce = -this.stiffness * displacement;
    const dampingForce = -this.damping * this.velocity;
    const acceleration = springForce + dampingForce;

    this.velocity += acceleration * dt;
    this.value += this.velocity * dt;

    return this.value;
  }

  /** Returns true if the spring has settled (within epsilon). */
  isSettled(epsilon = 0.001) {
    return (
      Math.abs(this.value - this.target) < epsilon &&
      Math.abs(this.velocity) < epsilon
    );
  }
}

/**
 * Create an expanding red shockwave ring on the floor.
 * Self-removes from scene after 1 second.
 *
 * @param {THREE.Scene} scene
 * @returns {{ update: (delta: number) => boolean }} - Returns false when done
 */
export function createShockwave(scene) {
  const geo = new THREE.RingGeometry(0.1, 0.3, 64);
  const mat = new THREE.MeshBasicMaterial({
    color: 0xff3333,
    transparent: true,
    opacity: 0.8,
    side: THREE.DoubleSide,
    depthWrite: false,
  });
  const ring = new THREE.Mesh(geo, mat);
  ring.rotation.x = -Math.PI / 2;
  ring.position.y = 0.05;
  scene.add(ring);

  let elapsed = 0;
  const duration = 1.0;
  const maxScale = 12;

  return {
    update(delta) {
      elapsed += delta;
      const t = Math.min(elapsed / duration, 1.0);

      const scale = 1 + t * maxScale;
      ring.scale.setScalar(scale);
      mat.opacity = 0.8 * (1 - t);

      if (t >= 1.0) {
        scene.remove(ring);
        geo.dispose();
        mat.dispose();
        return false;
      }
      return true;
    },
  };
}

/**
 * Create a burst of small glowing particles at a position.
 * Self-removes from scene after ~1.5 seconds.
 *
 * @param {THREE.Scene} scene
 * @param {THREE.Vector3} position
 * @param {number} color - Hex color
 * @returns {{ update: (delta: number) => boolean }}
 */
export function createVictoryParticles(scene, position, color) {
  const count = 40;
  const geo = new THREE.BufferGeometry();
  const positions = new Float32Array(count * 3);
  const velocities = [];

  for (let i = 0; i < count; i++) {
    positions[i * 3] = position.x;
    positions[i * 3 + 1] = position.y;
    positions[i * 3 + 2] = position.z;

    velocities.push(
      (Math.random() - 0.5) * 6,
      Math.random() * 4 + 2,
      (Math.random() - 0.5) * 6,
    );
  }

  geo.setAttribute('position', new THREE.BufferAttribute(positions, 3));

  const mat = new THREE.PointsMaterial({
    color,
    size: 0.15,
    transparent: true,
    opacity: 1.0,
    depthWrite: false,
    blending: THREE.AdditiveBlending,
  });

  const points = new THREE.Points(geo, mat);
  scene.add(points);

  let elapsed = 0;
  const duration = 1.5;

  return {
    update(delta) {
      elapsed += delta;
      const t = Math.min(elapsed / duration, 1.0);

      const posAttr = geo.attributes.position;
      for (let i = 0; i < count; i++) {
        posAttr.array[i * 3] += velocities[i * 3] * delta;
        posAttr.array[i * 3 + 1] += velocities[i * 3 + 1] * delta;
        posAttr.array[i * 3 + 2] += velocities[i * 3 + 2] * delta;

        // Gravity
        velocities[i * 3 + 1] -= 9.8 * delta;
      }
      posAttr.needsUpdate = true;

      mat.opacity = 1.0 - t;

      if (t >= 1.0) {
        scene.remove(points);
        geo.dispose();
        mat.dispose();
        return false;
      }
      return true;
    },
  };
}
