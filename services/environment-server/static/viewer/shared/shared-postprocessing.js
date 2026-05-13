/**
 * Shared Post-Processing Pipeline
 *
 * EffectComposer setup with bloom, vignette, and optional chromatic aberration.
 * Both game viewers share identical bloom + vignette; Coup adds chromatic aberration.
 */

import * as THREE from 'three';
import { EffectComposer } from 'three/addons/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/addons/postprocessing/RenderPass.js';
import { UnrealBloomPass } from 'three/addons/postprocessing/UnrealBloomPass.js';
import { ShaderPass } from 'three/addons/postprocessing/ShaderPass.js';

// ============================================================================
// Custom Shader Definitions
// ============================================================================

export const VignetteShader = {
  uniforms: {
    tDiffuse: { value: null },
    darkness: { value: 0.75 },
  },
  vertexShader: `
    varying vec2 vUv;
    void main() {
      vUv = uv;
      gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
    }
  `,
  fragmentShader: `
    uniform sampler2D tDiffuse;
    uniform float darkness;
    varying vec2 vUv;

    void main() {
      vec4 color = texture2D(tDiffuse, vUv);
      vec2 uv = (vUv - 0.5) * 2.0;
      // Normalized distance: 0 at center, 1 at corners
      float dist = dot(uv, uv) * 0.5;
      // pow(1.5) concentrates darkening at edges — cinematic sci-fi look
      float vignette = 1.0 - darkness * pow(dist, 1.5);
      vignette = clamp(vignette, 0.0, 1.0);
      color.rgb *= vignette;
      gl_FragColor = color;
    }
  `,
};

export const ChromaticAberrationShader = {
  uniforms: {
    tDiffuse: { value: null },
    amount: { value: 0.003 },
  },
  vertexShader: `
    varying vec2 vUv;
    void main() {
      vUv = uv;
      gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
    }
  `,
  fragmentShader: `
    uniform sampler2D tDiffuse;
    uniform float amount;
    varying vec2 vUv;

    void main() {
      vec2 dir = vUv - 0.5;
      float dist = length(dir);

      float r = texture2D(tDiffuse, vUv - dir * amount * dist).r;
      float g = texture2D(tDiffuse, vUv).g;
      float b = texture2D(tDiffuse, vUv + dir * amount * dist).b;

      gl_FragColor = vec4(r, g, b, 1.0);
    }
  `,
};

// ============================================================================
// Pipeline Factory
// ============================================================================

/**
 * Creates a post-processing pipeline with bloom, optional chromatic aberration,
 * and vignette.
 *
 * @param {THREE.WebGLRenderer} renderer
 * @param {THREE.Scene} scene
 * @param {THREE.Camera} camera
 * @param {object} [options]
 * @param {object} [options.bloom] - { strength, radius, threshold }
 * @param {object} [options.vignette] - { darkness }
 * @param {boolean|object} [options.chromaticAberration] - false to disable, or { amount }
 * @returns {{ composer: EffectComposer, bloomPass: UnrealBloomPass }}
 */
export function createPostProcessing(renderer, scene, camera, options = {}) {
  const bloom = { strength: 0.5, radius: 0.5, threshold: 0.35, ...options.bloom };
  const vignette = { darkness: 0.15, ...options.vignette };
  const chromatic = options.chromaticAberration || false;

  const composer = new EffectComposer(renderer);

  // Render pass
  const renderPass = new RenderPass(scene, camera);
  composer.addPass(renderPass);

  // Bloom
  const bloomPass = new UnrealBloomPass(
    new THREE.Vector2(window.innerWidth, window.innerHeight),
    bloom.strength,
    bloom.radius,
    bloom.threshold,
  );
  composer.addPass(bloomPass);

  // Chromatic aberration (optional — Coup uses it, Vibe Check does not)
  if (chromatic) {
    const chromaticPass = new ShaderPass(ChromaticAberrationShader);
    chromaticPass.uniforms.amount.value = typeof chromatic === 'object' ? chromatic.amount : 0.001;
    composer.addPass(chromaticPass);
  }

  // Vignette
  const vignettePass = new ShaderPass(VignetteShader);
  vignettePass.uniforms.darkness.value = vignette.darkness;
  composer.addPass(vignettePass);

  return { composer, bloomPass };
}
