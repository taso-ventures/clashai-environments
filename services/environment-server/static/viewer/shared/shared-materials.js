/**
 * Shared Holographic & Wireframe Shader Materials
 *
 * Factory functions for the translucent glass + wireframe overlay look
 * shared across all game viewers. Supports both SkinnedMesh (GLB models)
 * and regular Mesh (procedural geometry) via options flags.
 */

import * as THREE from 'three';

// ============================================================================
// Holographic Body Material
// ============================================================================

/**
 * Creates a translucent holographic material with fresnel rim glow,
 * subsurface scattering approximation, and scanlines.
 *
 * @param {number} color - Hex color for the hologram
 * @param {object} [options]
 * @param {boolean} [options.skinning=false] - Include skinning chunks for SkinnedMesh
 * @param {boolean} [options.roleBlend=false] - Include roleColor/roleBlend uniforms
 * @returns {THREE.ShaderMaterial}
 */
export function createHolographicMaterial(color, { skinning = false, roleBlend = false } = {}) {
  const uniforms = {
    time: { value: 0 },
    baseColor: { value: new THREE.Color(color) },
    fresnelPower: { value: 2.5 },
    glowIntensity: { value: skinning ? 0.4 : 0.35 },
    scanlineIntensity: { value: skinning ? 0.04 : 0.02 },
  };

  if (roleBlend) {
    uniforms.roleColor = { value: new THREE.Color(color) };
    uniforms.roleBlend = { value: 0.0 };
  }

  // Vertex shader — skinning variant uses Three.js include chunks
  const vertexShader = skinning
    ? `
      #include <common>
      #include <skinning_pars_vertex>

      varying vec3 vNormal;
      varying vec3 vViewPosition;
      varying vec3 vWorldPosition;

      void main() {
        #include <skinbase_vertex>
        #include <begin_vertex>
        #include <skinning_vertex>
        #include <beginnormal_vertex>
        #include <skinnormal_vertex>

        vNormal = normalize(normalMatrix * objectNormal);
        vec4 mvPosition = modelViewMatrix * vec4(transformed, 1.0);
        vViewPosition = mvPosition.xyz;
        vWorldPosition = (modelMatrix * vec4(transformed, 1.0)).xyz;
        gl_Position = projectionMatrix * mvPosition;
      }
    `
    : `
      varying vec3 vNormal;
      varying vec3 vViewPosition;
      varying vec2 vUv;
      varying vec3 vWorldPosition;

      void main() {
        vUv = uv;
        vNormal = normalize(normalMatrix * normal);
        vec4 mvPosition = modelViewMatrix * vec4(position, 1.0);
        vViewPosition = mvPosition.xyz;
        vWorldPosition = (modelMatrix * vec4(position, 1.0)).xyz;
        gl_Position = projectionMatrix * mvPosition;
      }
    `;

  // Fragment shader — roleBlend variant mixes toward roleColor
  const roleBlendDecl = roleBlend
    ? `uniform vec3 roleColor;\nuniform float roleBlend;`
    : '';
  const roleBlendMix = roleBlend
    ? 'vec3 effectiveColor = mix(baseColor, roleColor, roleBlend);'
    : 'vec3 effectiveColor = baseColor;';

  // Scanline frequency varies by style
  const scanlineFreq = skinning ? '40.0' : '30.0';
  // Pulse vs flicker
  const pulseCode = skinning
    ? 'float pulse = sin(time * 1.0) * 0.02 + 1.0;'
    : 'float pulse = sin(time * 10.0) * 0.01 + 1.0;';
  // Alpha tuning
  const alphaBase = skinning ? '0.55' : '0.72';
  const alphaFresnel = skinning ? '0.25' : '0.20';
  // Rim glow factor
  const rimFactor = skinning ? '0.3' : '0.25';

  const fragmentShader = `
    uniform float time;
    uniform vec3 baseColor;
    uniform float fresnelPower;
    uniform float glowIntensity;
    uniform float scanlineIntensity;
    ${roleBlendDecl}

    varying vec3 vNormal;
    varying vec3 vViewPosition;
    varying vec3 vWorldPosition;

    void main() {
      vec3 viewDir = normalize(-vViewPosition);
      float NdotV = max(0.0, dot(viewDir, vNormal));

      // Fresnel rim glow
      float fresnel = smoothstep(0.0, 1.0, 1.0 - NdotV);
      fresnel = pow(fresnel, fresnelPower);

      // Subsurface scattering approximation
      float sss = NdotV * NdotV * 0.18;
      vec3 sssColor = baseColor * 1.5 * sss;

      // Horizontal scanlines
      float scanline = sin(vWorldPosition.y * ${scanlineFreq} + time * 2.0) * 0.5 + 0.5;
      scanline = mix(1.0, scanline, scanlineIntensity);

      // Subtle pulse/flicker
      ${pulseCode}

      ${roleBlendMix}

      // Combine
      vec3 color = effectiveColor * (0.5 + fresnel * glowIntensity) * scanline * pulse;
      color += effectiveColor * fresnel * ${rimFactor};
      color += sssColor;

      float alpha = ${alphaBase} + fresnel * ${alphaFresnel};

      gl_FragColor = vec4(color, alpha);
    }
  `;

  return new THREE.ShaderMaterial({
    uniforms,
    vertexShader,
    fragmentShader,
    transparent: true,
    side: THREE.DoubleSide,
    depthWrite: true,
  });
}

// ============================================================================
// Wireframe Overlay Material
// ============================================================================

/**
 * Creates a wireframe overlay material for mesh grid lines.
 * Paired with the holographic body for a "technical blueprint" look.
 *
 * @param {number} color - Hex color
 * @param {object} [options]
 * @param {boolean} [options.skinning=false] - Include skinning chunks
 * @returns {THREE.ShaderMaterial}
 */
export function createWireframeMaterial(color, { skinning = false } = {}) {
  const vertexShader = skinning
    ? `
      #include <common>
      #include <skinning_pars_vertex>

      varying vec3 vWorldPosition;

      void main() {
        #include <skinbase_vertex>
        #include <begin_vertex>
        #include <skinning_vertex>

        vWorldPosition = (modelMatrix * vec4(transformed, 1.0)).xyz;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(transformed, 1.0);
      }
    `
    : `
      varying vec3 vWorldPosition;

      void main() {
        vWorldPosition = (modelMatrix * vec4(position, 1.0)).xyz;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
      }
    `;

  const fragmentShader = `
    uniform float time;
    uniform vec3 baseColor;
    varying vec3 vWorldPosition;
    void main() {
      float pulse = sin(time * 1.5 + vWorldPosition.y * 3.0) * 0.15 + 0.85;
      gl_FragColor = vec4(baseColor * 1.2 * pulse, 0.22);
    }
  `;

  return new THREE.ShaderMaterial({
    uniforms: {
      time: { value: 0 },
      baseColor: { value: new THREE.Color(color) },
    },
    vertexShader,
    fragmentShader,
    transparent: true,
    wireframe: true,
    depthWrite: false,
  });
}
