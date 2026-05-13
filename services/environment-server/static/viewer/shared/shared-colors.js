/**
 * Shared Player Color Palette
 *
 * Default 6-player palette used across all game viewers.
 * Game-specific viewers may override with their own palette
 * (e.g. Vibe Check uses team-grouped cool/warm split).
 */

export const PLAYER_COLORS = [
  0x00ffcc,  // Player 0: Teal/Cyan
  0xff8c00,  // Player 1: Orange
  0x4488ff,  // Player 2: Blue
  0x44ff88,  // Player 3: Green
  0xcc44ff,  // Player 4: Purple
  0xffcc00,  // Player 5: Yellow/Gold
];

/**
 * Get player color by ID with fallback to first color.
 * @param {number} id - Player index
 * @param {number[]} [palette] - Optional custom palette
 * @returns {number} Hex color
 */
export function getPlayerColor(id, palette = PLAYER_COLORS) {
  return palette[id] ?? palette[0];
}
