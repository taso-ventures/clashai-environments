/**
 * Shared Camera Preset Controller
 *
 * Provides smooth camera transitions between a home position and
 * per-player focus positions. Camera markers float above each player's
 * head as world-projected DOM elements, clickable to focus the camera.
 */

export class CameraPresetController {
  /**
   * @param {THREE.PerspectiveCamera} camera
   * @param {{ position: {x,y,z}, lookAt: {x,y,z} }} homeConfig
   * @param {{ x: number, y: number, z: number }} tableCenter
   */
  constructor(camera, homeConfig, tableCenter) {
    this.camera = camera;
    this.homeConfig = homeConfig;
    this.tableCenter = tableCenter;

    // Player data: Map<id, { worldPos, color, markerEl }>
    this.players = new Map();

    // Transition state
    this.transitioning = false;
    this.transitionProgress = 0;
    this.transitionDuration = 0.8;

    this.startPos = { x: 0, y: 0, z: 0 };
    this.targetPos = { x: 0, y: 0, z: 0 };
    this.startLookAt = { x: 0, y: 0, z: 0 };
    this.targetLookAt = { x: 0, y: 0, z: 0 };

    this.currentLookAt = { ...homeConfig.lookAt };

    this.focusedPlayerId = null;
    this._isHome = true;

    // DOM references
    this.markerContainer = null;
    this.homeBtn = null;
  }

  /**
   * Register a player for camera focus.
   * @param {number} id
   * @param {{ x: number, y: number, z: number }} worldPos
   * @param {number} color - Hex color (e.g. 0x00ffcc).
   */
  addPlayer(id, worldPos, color) {
    this.players.set(id, {
      worldPos: { x: worldPos.x, y: worldPos.y, z: worldPos.z },
      color,
      markerEl: null,
    });
  }

  /**
   * Build floating markers above each player + a small home button.
   * @param {HTMLElement} overlay - The #ui-overlay element.
   */
  createUI(overlay) {
    // Container for floating markers (same layer as reasoning bubbles)
    const container = document.createElement('div');
    container.id = 'camera-markers';
    container.className = 'camera-markers';
    overlay.appendChild(container);
    this.markerContainer = container;

    // Create per-player floating markers
    for (const [id, data] of this.players) {
      const hex = `#${(data.color || 0x888888).toString(16).padStart(6, '0')}`;

      const marker = document.createElement('button');
      marker.className = 'camera-marker';
      marker.dataset.playerId = id;
      marker.style.setProperty('--marker-color', hex);
      marker.title = `Focus Player ${id}`;

      // Inner dot + ring
      marker.innerHTML = `<span class="camera-marker-dot"></span><span class="camera-marker-ring"></span>`;

      marker.addEventListener('click', () => {
        if (this.focusedPlayerId === id) {
          this.goHome();
        } else {
          this.focusPlayer(id);
        }
      });

      container.appendChild(marker);
      data.markerEl = marker;
    }

    // Small home button (bottom-right, only shown when focused on a player)
    const homeBtn = document.createElement('button');
    homeBtn.className = 'camera-home-btn hidden';
    homeBtn.title = 'Return to overview';
    homeBtn.innerHTML = '<span class="camera-home-icon">\u2302</span>';
    homeBtn.addEventListener('click', () => this.goHome());
    overlay.appendChild(homeBtn);
    this.homeBtn = homeBtn;
  }

  /**
   * Update floating marker screen positions. Call every frame.
   * @param {function} getWorldPos - (playerId) => THREE.Vector3
   * @param {number} markerOffsetY - World-space Y offset above player (e.g. 4.0).
   */
  updateMarkerPositions(getWorldPos, markerOffsetY = 4.0) {
    if (!this.markerContainer) return;

    const w = window.innerWidth;
    const h = window.innerHeight;

    for (const [id, data] of this.players) {
      if (!data.markerEl) continue;

      const worldPos = getWorldPos(id);
      if (!worldPos) {
        data.markerEl.style.left = '-9999px';
        continue;
      }

      worldPos.y += markerOffsetY;

      const projected = worldPos.project(this.camera);

      if (projected.z > 0 && projected.z < 1) {
        const sx = (projected.x * 0.5 + 0.5) * w;
        const sy = (-projected.y * 0.5 + 0.5) * h;
        data.markerEl.style.left = `${sx}px`;
        data.markerEl.style.top = `${sy}px`;
      } else {
        data.markerEl.style.left = '-9999px';
      }
    }
  }

  _computeFocusPosition(playerId) {
    const data = this.players.get(playerId);
    if (!data) return this.homeConfig.position;

    const playerPos = data.worldPos;
    const tc = this.tableCenter;

    const dx = playerPos.x - tc.x;
    const dz = playerPos.z - tc.z;
    const dist = Math.sqrt(dx * dx + dz * dz) || 1;
    const nx = dx / dist;
    const nz = dz / dist;

    const behindDist = 6.0;
    const elevation = 6.0;

    return {
      x: playerPos.x + nx * behindDist,
      y: playerPos.y + elevation,
      z: playerPos.z + nz * behindDist,
    };
  }

  focusPlayer(id) {
    if (!this.players.has(id)) return;

    this.focusedPlayerId = id;
    this._isHome = false;

    this._startTransition(
      this._computeFocusPosition(id),
      this.tableCenter,
    );

    this._updateUIActive(id);
  }

  goHome() {
    this.focusedPlayerId = null;
    this._isHome = true;

    this._startTransition(
      this.homeConfig.position,
      this.homeConfig.lookAt,
    );

    this._updateUIActive(null);
  }

  /** @private */
  _startTransition(targetPos, targetLookAt) {
    this.startPos = {
      x: this.camera.position.x,
      y: this.camera.position.y,
      z: this.camera.position.z,
    };
    this.targetPos = { ...targetPos };

    this.startLookAt = { ...this.currentLookAt };
    this.targetLookAt = { ...targetLookAt };

    this.transitionProgress = 0;
    this.transitioning = true;
  }

  /**
   * Per-frame update for camera transitions.
   * @param {number} delta
   * @returns {boolean} true while transition is active or camera is focused
   */
  update(delta) {
    if (!this.transitioning) return !this._isHome;

    this.transitionProgress += delta / this.transitionDuration;

    if (this.transitionProgress >= 1) {
      this.transitionProgress = 1;
      this.transitioning = false;
    }

    const t = 1 - Math.pow(1 - this.transitionProgress, 3);

    this.camera.position.x = this.startPos.x + (this.targetPos.x - this.startPos.x) * t;
    this.camera.position.y = this.startPos.y + (this.targetPos.y - this.startPos.y) * t;
    this.camera.position.z = this.startPos.z + (this.targetPos.z - this.startPos.z) * t;

    this.currentLookAt.x = this.startLookAt.x + (this.targetLookAt.x - this.startLookAt.x) * t;
    this.currentLookAt.y = this.startLookAt.y + (this.targetLookAt.y - this.startLookAt.y) * t;
    this.currentLookAt.z = this.startLookAt.z + (this.targetLookAt.z - this.startLookAt.z) * t;

    this.camera.lookAt(this.currentLookAt.x, this.currentLookAt.y, this.currentLookAt.z);

    return true;
  }

  get isHome() {
    return this._isHome && !this.transitioning;
  }

  /** @private */
  _updateUIActive(playerId) {
    // Update marker states
    for (const [id, data] of this.players) {
      if (!data.markerEl) continue;
      data.markerEl.classList.toggle('focused', id === playerId);
    }

    // Show/hide home button
    if (this.homeBtn) {
      this.homeBtn.classList.toggle('hidden', playerId === null);
    }
  }
}
