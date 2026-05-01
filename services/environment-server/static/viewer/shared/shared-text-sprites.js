/**
 * Shared Canvas-Based 3D Text Sprite Factory
 *
 * Creates text labels as THREE.Sprite using CanvasTexture.
 * Used by both viewers for HUD elements, role badges, and floating labels.
 */

import * as THREE from 'three';

/**
 * Create a 3D text sprite rendered via canvas.
 *
 * @param {string} text - Display text
 * @param {number} color - Hex color (e.g. 0xffffff)
 * @param {number} [scale=1] - Sprite scale multiplier
 * @param {object} [options]
 * @param {number} [options.canvasWidth=512]
 * @param {number} [options.canvasHeight=128]
 * @param {string} [options.font='bold 42px "Geist Sans", sans-serif']
 * @param {number} [options.shadowBlur=12]
 * @returns {THREE.Sprite}
 */
export function createTextSprite(text, color, scale = 1, options = {}) {
  const {
    canvasWidth = 512,
    canvasHeight = 128,
    font = 'bold 42px "Geist Sans", sans-serif',
    shadowBlur = 12,
  } = options;

  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d');
  canvas.width = canvasWidth;
  canvas.height = canvasHeight;

  drawTextToCanvas(ctx, text, color, canvasWidth, canvasHeight, font, shadowBlur);

  const texture = new THREE.CanvasTexture(canvas);
  texture.minFilter = THREE.LinearFilter;

  const material = new THREE.SpriteMaterial({
    map: texture,
    transparent: true,
    depthTest: false,
  });

  const sprite = new THREE.Sprite(material);
  sprite.scale.set(4 * scale, 1 * scale, 1);
  sprite.userData.canvas = canvas;
  sprite.userData.ctx = ctx;
  sprite.userData.texture = texture;
  sprite.userData.color = color;

  return sprite;
}

/**
 * Update the text on an existing sprite.
 *
 * @param {THREE.Sprite} sprite - Previously created via createTextSprite
 * @param {string} text - New text
 * @param {number} [color] - Optional new color; defaults to existing
 */
export function updateSpriteText(sprite, text, color) {
  if (!sprite?.userData?.ctx) return;
  const { ctx, canvas, texture } = sprite.userData;
  const c = color !== undefined ? color : sprite.userData.color;
  drawTextToCanvas(ctx, text, c, canvas.width, canvas.height);
  texture.needsUpdate = true;
  sprite.userData.color = c;
}

/**
 * Draw text onto a canvas context.
 * @private
 */
function drawTextToCanvas(ctx, text, colorHex, width, height, font, shadowBlur) {
  ctx.clearRect(0, 0, width, height);
  const hex = typeof colorHex === 'number'
    ? '#' + colorHex.toString(16).padStart(6, '0')
    : colorHex;
  ctx.font = font || 'bold 42px "Geist Sans", sans-serif';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillStyle = hex;
  ctx.shadowColor = hex;
  ctx.shadowBlur = shadowBlur ?? 12;
  ctx.fillText(text, width / 2, height / 2);
  ctx.shadowBlur = 0;
}
