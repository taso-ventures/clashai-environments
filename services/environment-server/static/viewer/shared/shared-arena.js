/**
 * Shared Arena Construction
 *
 * Builds the curved sci-fi arena walls, floor, and grid shaders
 * common to all game viewers. Configurable wall/floor dimensions,
 * colors, and grid density.
 */

import * as THREE from 'three';

/**
 * Builds arena walls, grid overlay, floor, and floor grid.
 *
 * @param {THREE.Scene} scene
 * @param {object} [options]
 * @param {number} [options.wallRadiusTop=18]
 * @param {number} [options.wallRadiusBottom=20]
 * @param {number} [options.wallHeight=14]
 * @param {number} [options.wallY=5] - Vertical center of walls
 * @param {number} [options.wallSegments=32]
 * @param {number} [options.bgColor=0x0A1520] - Wall color
 * @param {number} [options.accentColor=0x00E5CC] - Grid line / glow color
 * @param {number} [options.floorColor=0x080E14]
 * @param {number} [options.floorRadius=20]
 * @param {number} [options.gridHDensity=32] - Horizontal grid lines per unit
 * @param {number} [options.gridVDensity=8] - Vertical grid lines
 * @param {number} [options.wallMetalness=0.8]
 * @param {number} [options.wallRoughness=0.4]
 * @param {number} [options.floorMetalness=0.9]
 * @param {number} [options.floorRoughness=0.3]
 * @returns {{ materials: THREE.ShaderMaterial[] }} - Shader materials needing time uniform updates
 */
export function buildArena(scene, options = {}) {
  const {
    wallRadiusTop = 18,
    wallRadiusBottom = 20,
    wallHeight = 14,
    wallY = 5,
    wallSegments = 32,
    bgColor = 0x0A1520,
    accentColor = 0x00E5CC,
    floorColor = 0x080E14,
    floorRadius = 20,
    gridHDensity = 32,
    gridVDensity = 8,
    wallMetalness = 0.8,
    wallRoughness = 0.4,
    floorMetalness = 0.9,
    floorRoughness = 0.3,
  } = options;

  const shaderMaterials = [];
  const accentThreeColor = new THREE.Color(accentColor);

  // --- Curved arena walls ---
  const wallGeo = new THREE.CylinderGeometry(wallRadiusTop, wallRadiusBottom, wallHeight, wallSegments, 1, true);
  const wallMat = new THREE.MeshStandardMaterial({
    color: bgColor,
    side: THREE.BackSide,
    metalness: wallMetalness,
    roughness: wallRoughness,
  });
  const wall = new THREE.Mesh(wallGeo, wallMat);
  wall.position.y = wallY;
  scene.add(wall);

  // --- Animated grid overlay on walls ---
  const gridMat = new THREE.ShaderMaterial({
    uniforms: {
      time: { value: 0 },
      gridColor: { value: accentThreeColor },
    },
    vertexShader: `
      varying vec2 vUv;
      varying vec3 vPosition;
      void main() {
        vUv = uv;
        vPosition = position;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
      }
    `,
    fragmentShader: `
      uniform float time;
      uniform vec3 gridColor;
      varying vec2 vUv;
      varying vec3 vPosition;

      void main() {
        float gridX = step(0.95, fract(vUv.x * ${gridHDensity.toFixed(1)}));
        float gridY = step(0.95, fract(vUv.y * ${gridVDensity.toFixed(1)}));
        float grid = max(gridX, gridY);
        float pulse = sin(vPosition.y * 2.0 - time * 0.5) * 0.5 + 0.5;
        float fade = smoothstep(0.0, 0.3, vUv.y) * smoothstep(1.0, 0.7, vUv.y);
        float alpha = grid * 0.15 * pulse * fade;
        gl_FragColor = vec4(gridColor, alpha);
      }
    `,
    transparent: true,
    side: THREE.BackSide,
    depthWrite: false,
  });
  shaderMaterials.push(gridMat);

  const gridGeo = new THREE.CylinderGeometry(
    wallRadiusTop - 0.1,
    wallRadiusBottom - 0.1,
    wallHeight,
    wallSegments,
    16,
    true,
  );
  const gridMesh = new THREE.Mesh(gridGeo, gridMat);
  gridMesh.position.y = wallY;
  scene.add(gridMesh);

  // --- Floor ---
  const floorGeo = new THREE.CircleGeometry(floorRadius, 64);
  const floorMat = new THREE.MeshStandardMaterial({
    color: floorColor,
    metalness: floorMetalness,
    roughness: floorRoughness,
  });
  const floor = new THREE.Mesh(floorGeo, floorMat);
  floor.rotation.x = -Math.PI / 2;
  floor.position.y = -0.1;
  floor.receiveShadow = true;
  scene.add(floor);

  // --- Floor radial grid ---
  const floorGridMat = new THREE.ShaderMaterial({
    uniforms: {
      gridColor: { value: accentThreeColor },
    },
    vertexShader: `
      varying vec2 vUv;
      void main() {
        vUv = uv;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
      }
    `,
    fragmentShader: `
      uniform vec3 gridColor;
      varying vec2 vUv;

      void main() {
        vec2 centered = (vUv - 0.5) * 2.0;
        float dist = length(centered);

        // Radial rings
        float radial = step(0.98, fract(dist * 10.0));
        // Angular sectors
        float angle = atan(centered.y, centered.x);
        float angular = step(0.98, fract(angle * 3.0 / 3.14159));

        float grid = max(radial, angular) * smoothstep(1.0, 0.3, dist);
        gl_FragColor = vec4(gridColor, grid * 0.10);
      }
    `,
    transparent: true,
    depthWrite: false,
  });

  const floorGridMesh = new THREE.Mesh(new THREE.CircleGeometry(floorRadius, 64), floorGridMat);
  floorGridMesh.rotation.x = -Math.PI / 2;
  floorGridMesh.position.y = -0.09;
  scene.add(floorGridMesh);

  return { materials: shaderMaterials };
}
