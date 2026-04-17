/**
 * Coup Spectator Viewer - Main Orchestrator
 * Integrates state management, 3D rendering, and animations
 */

import { CoupStateManager, formatAction, getRoleInfo } from './coup-state.js';
import { CoupRenderer } from './coup-render.js';
import { ReplayController } from './shared/replay-controller.js';
import {
  AnimationManager,
  CardRevealAnimation,
  CoinTransferAnimation,
  ChallengeLightningAnimation,
  BlockBarrierAnimation,
  EliminationAnimation,
  VictoryBurstAnimation,
  ActionLabelAnimation,
  StealBeamAnimation,
  AssassinateSlashAnimation,
  CoupStrikeAnimation,
  CoinGainAnimation,
  TIMING,
} from './coup-anim.js';
import { typeText, setupBubbleHover } from './shared/shared-bubble-effects.js';
import { CameraPresetController } from './shared/shared-camera-controls.js';
import * as THREE from 'three';

class CoupViewer {
  constructor() {
    this.matchId = null;
    this.isEmbed = false;

    this.stateManager = null;
    this.renderer = null;
    this.animationManager = null;
    this.replayController = null;
    this.isReplay = false;
    this.sceneInitialized = false;
    this.readySent = false;
    this.reasoningBubbles = new Map(); // playerId → { element, hideTimer }
    this.cameraPresets = null;

    // DOM elements
    this.elements = {
      turnNumber: document.getElementById('turn-number'),
      phaseIndicator: document.getElementById('phase-indicator'),
      actionRibbon: document.getElementById('action-ribbon'),
      actionText: document.getElementById('action-text'),
      actionTimer: document.getElementById('action-timer'),
      timerBar: document.getElementById('timer-bar'),
      playerPanels: document.getElementById('player-panels'),
      resolutionOverlay: document.getElementById('resolution-overlay'),
      resolutionIcon: document.getElementById('resolution-icon'),
      resolutionText: document.getElementById('resolution-text'),
      gameOverOverlay: document.getElementById('game-over-overlay'),
      winnerName: document.getElementById('winner-name'),
      connectionStatus: document.getElementById('connection-status'),
      reasoningContainer: document.getElementById('reasoning-bubbles'),
    };

    this.init();
  }

  async init() {
    // Parse URL parameters
    const params = new URLSearchParams(window.location.search);
    const matchId = params.get('matchId');
    this.isEmbed = params.get('embed') === '1';

    // Validate match ID format (ULID: 26 alphanumeric characters)
    if (!matchId) {
      this.showError('No match ID provided');
      return;
    }
    if (!/^[0-9A-HJKMNP-TV-Z]{26}$/i.test(matchId)) {
      this.showError('Invalid match ID format');
      return;
    }
    this.matchId = matchId;

    // Apply embed mode styling
    if (this.isEmbed) {
      document.body.classList.add('embed-mode');
    }

    // Reasoning toggle
    this.setupReasoningToggle();

    // Initialize renderer
    const canvas = document.getElementById('three-canvas');
    this.renderer = new CoupRenderer(canvas);

    // Initialize animation manager
    this.animationManager = new AnimationManager();

    // Initialize state manager
    this.stateManager = new CoupStateManager(
      this.matchId,
      (eventType, data) => this.handleEvent(eventType, data),
      (state) => this.handleStateChange(state)
    );

    this.stateManager.setConnectionStatusCallback((status) => {
      this.updateConnectionStatus(status);
    });

    // Load initial state and check if this is a completed match (replay)
    try {
      await this.stateManager.loadInitialState();
      await this.initializeScene();

      // Detect replay mode from initial state
      const state = this.stateManager.getState();
      const phase = state.currentPhase;
      this.isReplay = (typeof phase === 'object' && phase?.game_over) || state.winner !== null;

      if (this.isReplay) {
        this.setupReplayController();
      }

      this.stateManager.connect();
      this.startRenderLoop();
    } catch (error) {
      console.error('Failed to initialize viewer:', error);
      this.showError('Failed to connect to match');
    }
  }

  async initializeScene() {
    const state = this.stateManager.getState();
    const playerCount = state.players.size;

    // Create 3D players
    this.renderer.initializePlayers(playerCount);

    // Wait for GLB characters to finish loading before applying visual state
    await this.renderer.charactersLoaded;

    // Update initial state
    for (const [playerId, playerState] of state.players) {
      this.renderer.updatePlayerCoins(playerId, playerState.coins);
      if (playerState.eliminated) {
        this.renderer.setPlayerEliminated(playerId);
      }
    }

    // Set active player
    this.renderer.setPlayerActive(state.activePlayer, true);

    // Initialize camera presets
    this.initCameraPresets();

    // Create player panels
    this.createPlayerPanels(playerCount);
    this.updatePlayerPanels(state);

    // Update phase
    this.updatePhaseDisplay(state);
  }

  startRenderLoop() {
    const clock = new THREE.Clock();

    const animate = () => {
      requestAnimationFrame(animate);

      const delta = clock.getDelta();

      // Update animations
      this.animationManager.update(delta);

      // Position reasoning bubbles above player heads
      this.updateBubblePositions();

      // Position camera markers above player heads
      if (this.cameraPresets) {
        this.cameraPresets.updateMarkerPositions(
          (id) => this.renderer.getPlayerPosition(id),
          3.5,
        );
      }

      // Render scene
      this.renderer.render();
    };

    animate();
  }

  async handleEvent(eventType, data) {
    console.log(`[Viewer] Event: ${eventType}`, data);

    switch (eventType) {
      case 'game_started':
        await this.handleGameStarted(data);
        break;

      case 'turn_advanced':
        // State update handled in coup-state.js; clear action ribbon for new turn
        this.elements.actionRibbon.classList.add('hidden');
        this.revertAllPlayerShapes();
        break;

      case 'agent_reasoning':
        this.showReasoningBubble(data.player, data.reasoning);
        break;

      case 'action_declared':
        await this.handleActionDeclared(data);
        break;

      case 'challenge_issued':
        await this.handleChallengeIssued(data);
        break;

      case 'block_declared':
        await this.handleBlockDeclared(data);
        break;

      case 'card_revealed':
        await this.handleCardRevealed(data);
        break;

      case 'influence_lost':
        await this.handleInfluenceLost(data);
        break;

      case 'player_eliminated':
        await this.handlePlayerEliminated(data);
        break;

      case 'game_over':
        await this.handleGameOver(data);
        break;
    }
  }

  handleStateChange(state) {
    this.updatePlayerPanels(state);
    this.updatePhaseDisplay(state);
  }

  async handleGameStarted(data) {
    // On reconnect the server resends GameStarted as part of the event
    // history. Skip re-initialization to preserve the 3D scene and panels
    // that were already set up from the REST state load.
    if (this.sceneInitialized) return;

    const playerCount = data.players.length;
    this.renderer.initializePlayers(playerCount);
    this.createPlayerPanels(playerCount);
    this.sceneInitialized = true;

    // Flash effect for game start
    this.showResolution('GAME START', 'success');
    await this.delay(TIMING.resolutionDisplay * 1000);
    this.hideResolution();
  }

  async handleActionDeclared(data) {
    const action = data.action;
    const actorName = this.getPlayerName(data.player);
    const actionName = formatAction(action);

    let targetText = '';
    if (action.target !== undefined) {
      targetText = ` targeting <span class="target">${this.getPlayerName(action.target)}</span>`;
    }

    // Show action ribbon
    this.elements.actionText.innerHTML =
      `<span class="actor">${actorName}</span> declares <span class="action">${actionName}</span>${targetText}`;
    this.elements.actionRibbon.classList.remove('hidden');

    // Revert any previously morphed players before new morph
    this.revertAllPlayerShapes();

    // Show role sigil, aura, and morph body if action claims a role
    const claimedRole = this.getClaimedRole(action);
    if (claimedRole) {
      this.renderer.createRoleSigil(data.player, claimedRole);
      this.renderer.setPlayerRoleAura(data.player, claimedRole);
      const morphAnim = this.renderer.morphPlayerToRole(data.player, claimedRole);
      if (morphAnim) this.animationManager.add(morphAnim);
    }

    // Highlight actor
    this.renderer.setPlayerActive(data.player, true);

    // Action tag on reasoning bubble
    const actionType = action.action_type || Object.keys(action)[0];
    const tagLabel = actionName.toUpperCase();
    const tagClass = actionType.replace(/_/g, '-');
    this.setBubbleActionTag(data.player, tagLabel, tagClass);

    // Trigger reaction on non-actor players
    for (const [pid, p] of this.renderer.players) {
      if (pid !== data.player && !p.eliminated) {
        this.renderer.triggerReaction(pid);
      }
    }

    // Action-specific 3D effects
    const actorPos = this.renderer.getPlayerPosition(data.player);

    switch (actionType) {
      case 'steal': {
        const targetPos = this.renderer.getPlayerPosition(action.target);
        const midPos = actorPos.clone().add(targetPos).multiplyScalar(0.5);
        midPos.y += 2.5;
        this.animationManager.add(new ActionLabelAnimation(
          this.renderer.scene, midPos, 'STEAL', 0xd4af37, () => {}
        ));
        this.animationManager.add(new StealBeamAnimation(
          this.renderer.scene, actorPos, targetPos, () => {}
        ));
        break;
      }
      case 'assassinate': {
        const targetPos = this.renderer.getPlayerPosition(action.target);
        const midPos = actorPos.clone().add(targetPos).multiplyScalar(0.5);
        midPos.y += 2.5;
        this.animationManager.add(new ActionLabelAnimation(
          this.renderer.scene, midPos, 'ASSASSINATE', 0xff2222, () => {}
        ));
        this.animationManager.add(new AssassinateSlashAnimation(
          this.renderer.scene, actorPos, targetPos, () => {}
        ));
        break;
      }
      case 'coup': {
        const targetPos = this.renderer.getPlayerPosition(action.target);
        const midPos = actorPos.clone().add(targetPos).multiplyScalar(0.5);
        midPos.y += 2.5;
        this.animationManager.add(new ActionLabelAnimation(
          this.renderer.scene, midPos, 'COUP', 0xff4444, () => {}
        ));
        this.animationManager.add(new CoupStrikeAnimation(
          this.renderer.scene, targetPos, () => {}
        ));
        break;
      }
      case 'income': {
        this.animationManager.add(new CoinGainAnimation(
          this.renderer.scene, actorPos, 1, () => {}
        ));
        break;
      }
      case 'tax': {
        const labelPos = actorPos.clone();
        labelPos.y += 2.5;
        this.animationManager.add(new ActionLabelAnimation(
          this.renderer.scene, labelPos, 'TAX', 0x9b59b6, () => {}
        ));
        this.animationManager.add(new CoinGainAnimation(
          this.renderer.scene, actorPos, 3, () => {}
        ));
        break;
      }
      case 'foreign_aid': {
        this.animationManager.add(new CoinGainAnimation(
          this.renderer.scene, actorPos, 2, () => {}
        ));
        break;
      }
      case 'exchange': {
        const labelPos = actorPos.clone();
        labelPos.y += 2.5;
        this.animationManager.add(new ActionLabelAnimation(
          this.renderer.scene, labelPos, 'EXCHANGE', 0x27ae60, () => {}
        ));
        break;
      }
    }
  }

  async handleChallengeIssued(data) {
    const challengerPos = this.renderer.getPlayerPosition(data.challenger);
    const targetPos = this.renderer.getPlayerPosition(data.against);

    // Elevate Y for effect
    challengerPos.y += 1.5;
    targetPos.y += 1.5;

    // Action tag on challenger's bubble
    this.setBubbleActionTag(data.challenger, 'CHALLENGE', 'challenge');

    // Trigger reaction on the challenged player
    this.renderer.triggerReaction(data.against);

    // Show resolution overlay
    this.showResolution('CHALLENGE!', 'challenge');

    // 3D label at midpoint between players
    const midPos = challengerPos.clone().add(targetPos).multiplyScalar(0.5);
    midPos.y += 1.0;
    this.animationManager.add(new ActionLabelAnimation(
      this.renderer.scene, midPos, 'CHALLENGE', 0xff9500, () => {}
    ));

    // Play lightning animation
    const lightning = new ChallengeLightningAnimation(
      this.renderer.scene,
      challengerPos,
      targetPos,
      () => {}
    );
    this.animationManager.add(lightning);

    await this.delay(TIMING.challengeLightning * 1000);
    this.hideResolution();
  }

  async handleBlockDeclared(data) {
    const blockerPos = this.renderer.getPlayerPosition(data.blocker);

    // Action tag on blocker's bubble
    this.setBubbleActionTag(data.blocker, 'BLOCK', 'block');

    // Trigger reaction on the blocker
    this.renderer.triggerReaction(data.blocker);

    // Show role sigil, aura, and morph body for claimed block role
    this.renderer.createRoleSigil(data.blocker, data.role);
    this.renderer.setPlayerRoleAura(data.blocker, data.role);
    const morphAnim = this.renderer.morphPlayerToRole(data.blocker, data.role);
    if (morphAnim) this.animationManager.add(morphAnim);

    // Show resolution overlay
    const roleInfo = getRoleInfo(data.role);
    this.showResolution(`BLOCK! (${roleInfo.name})`, 'block');

    // 3D label at blocker position
    const labelPos = blockerPos.clone();
    labelPos.y += 2.5;
    this.animationManager.add(new ActionLabelAnimation(
      this.renderer.scene, labelPos, 'BLOCK', 0x00e5ff, () => {}
    ));

    // Play barrier animation
    const barrier = new BlockBarrierAnimation(
      this.renderer.scene,
      blockerPos,
      () => {}
    );
    this.animationManager.add(barrier);

    await this.delay(TIMING.blockBarrier * 1000);
    this.hideResolution();
  }

  async handleCardRevealed(data) {
    // Update renderer
    this.renderer.revealCard(data.player, 0, data.role);

    // Show role info
    const roleInfo = getRoleInfo(data.role);
    this.showResolution(`${roleInfo.icon} ${roleInfo.name}`, 'success');

    await this.delay(TIMING.cardFlip * 1000);
    this.hideResolution();
  }

  async handleInfluenceLost(data) {
    // Reveal the lost card
    this.renderer.revealCard(data.player, 0, data.role);

    const roleInfo = getRoleInfo(data.role);
    this.showResolution(`INFLUENCE LOST: ${roleInfo.name}`, 'failure');

    await this.delay(TIMING.cardFlip * 1000);
    this.hideResolution();

    // Clear role sigils, aura, and revert body shape
    this.renderer.removeRoleSigil(data.player);
    this.renderer.clearPlayerRoleAura(data.player);
    const revertAnim = this.renderer.revertPlayerShape(data.player);
    if (revertAnim) this.animationManager.add(revertAnim);
  }

  async handlePlayerEliminated(data) {
    const player = this.renderer.players.get(data.player);
    if (!player) return;

    // Trigger reaction on all remaining alive players
    for (const [pid, p] of this.renderer.players) {
      if (pid !== data.player && !p.eliminated) {
        this.renderer.triggerReaction(pid);
      }
    }

    // Show elimination overlay
    this.showResolution(`${this.getPlayerName(data.player)} ELIMINATED`, 'failure');

    // 3D label at player position
    const labelPos = this.renderer.getPlayerPosition(data.player);
    labelPos.y += 2.5;
    this.animationManager.add(new ActionLabelAnimation(
      this.renderer.scene, labelPos, 'ELIMINATED', 0xff4444, () => {}
    ));

    // Clear any role aura
    this.renderer.clearPlayerRoleAura(data.player);

    // Mark dissolving so idle behaviors (breathing scale) don't overwrite the animation
    player.dissolving = true;

    // Play dissolution animation (only if character GLB has loaded)
    if (player.humanoid) {
      const elimination = new EliminationAnimation(
        this.renderer.scene,
        player,
        () => {}
      );
      this.animationManager.add(elimination);
    }

    await this.delay(TIMING.eliminationDissolve * 1000);

    // Mark as eliminated in renderer
    this.renderer.setPlayerEliminated(data.player);

    this.hideResolution();
  }

  async handleGameOver(data) {
    // Clear action ribbon
    this.elements.actionRibbon.classList.add('hidden');

    // Get winner player info
    const player = this.renderer.players.get(data.winner);
    if (player) {
      // Play victory burst animation
      const victoryBurst = new VictoryBurstAnimation(
        this.renderer.scene,
        player.group.position,
        player.color,
        () => {}
      );
      this.animationManager.add(victoryBurst);
    }

    // Highlight winner
    this.renderer.setPlayerActive(data.winner, true);

    // Wait for animation then show overlay
    await this.delay(TIMING.victoryBurst * 500);

    // Show game over overlay
    this.elements.winnerName.textContent = this.getPlayerName(data.winner);
    this.elements.gameOverOverlay.classList.remove('hidden');
  }

  initCameraPresets() {
    const homeConfig = {
      position: { x: 0, y: 6.5, z: 12 },
      lookAt: { x: 0, y: 0.5, z: 0 },
    };

    this.cameraPresets = new CameraPresetController(
      this.renderer.camera,
      homeConfig,
      { x: 0, y: 0.5, z: 0 },
    );

    // Register player positions and colors
    for (const [playerId, player] of this.renderer.players) {
      const pos = this.renderer.getPlayerPosition(playerId);
      this.cameraPresets.addPlayer(playerId, pos, player.color);
    }

    // Wire into renderer so orbital drift is gated
    this.renderer.cameraPresetController = this.cameraPresets;

    // Build UI in the overlay
    const overlay = document.getElementById('ui-overlay');
    this.cameraPresets.createUI(overlay);
  }

  // UI Helper Methods

  createPlayerPanels(playerCount) {
    this.elements.playerPanels.innerHTML = '';

    for (let i = 0; i < playerCount; i++) {
      const panel = document.createElement('div');
      panel.className = 'player-panel';
      panel.dataset.player = i;

      panel.innerHTML = `
        <span class="player-color-dot"></span>
        <span class="player-name">${this.getPlayerName(i)}</span>
        <span class="player-coins">
          <span class="coin-icon"></span>
          <span class="coin-count">2</span>
        </span>
        <span class="player-influence">
          <span class="influence-pip"></span>
          <span class="influence-pip"></span>
        </span>
      `;

      this.elements.playerPanels.appendChild(panel);
    }
  }

  updatePlayerPanels(state) {
    for (const [playerId, playerState] of state.players) {
      const panel = document.querySelector(`.player-panel[data-player="${playerId}"]`);
      if (!panel) continue;

      // Update coins
      const coinCount = panel.querySelector('.coin-count');
      coinCount.textContent = playerState.coins;

      // Update influence pips
      const pips = panel.querySelectorAll('.influence-pip');
      playerState.cards.forEach((card, i) => {
        if (pips[i]) {
          pips[i].classList.toggle('lost', card.revealed);
        }
      });

      // Update active state
      panel.classList.toggle('active', state.activePlayer === playerId);

      // Update eliminated state
      panel.classList.toggle('eliminated', playerState.eliminated);
    }
  }

  updatePhaseDisplay(state) {
    // Update turn number
    this.elements.turnNumber.textContent = `Turn ${state.turnNumber}`;

    // Update phase indicator
    let phaseText = 'Waiting...';
    const phase = state.currentPhase;

    if (typeof phase === 'string') {
      phaseText = this.formatPhaseName(phase);
    } else if (phase && typeof phase === 'object') {
      const phaseKey = Object.keys(phase)[0];
      phaseText = this.formatPhaseName(phaseKey);
    }

    this.elements.phaseIndicator.textContent = phaseText;
  }

  formatPhaseName(phase) {
    const names = {
      awaiting_action: 'Awaiting Action',
      challenge_window: 'Challenge Window',
      block_window: 'Block Window',
      block_challenge_window: 'Block Challenge',
      revealing_card: 'Revealing Card',
      selecting_card_to_lose: 'Selecting Card',
      exchange_selection: 'Exchange',
      action_resolving: 'Resolving...',
      game_over: 'Game Over',
    };
    return names[phase] || phase;
  }

  showResolution(text, type) {
    this.elements.resolutionText.textContent = text;
    this.elements.resolutionOverlay.className = type;
    this.elements.resolutionOverlay.classList.remove('hidden');
  }

  hideResolution() {
    this.elements.resolutionOverlay.classList.add('hidden');
  }

  revertAllPlayerShapes() {
    for (const [playerId] of this.renderer.players) {
      const revertAnim = this.renderer.revertPlayerShape(playerId);
      if (revertAnim) this.animationManager.add(revertAnim);
    }
  }

  setupReasoningToggle() {
    const btn = document.getElementById('reasoning-toggle');
    if (!btn) return;
    btn.addEventListener('click', () => {
      const overlay = document.getElementById('ui-overlay');
      const isHidden = overlay.classList.toggle('reasoning-hidden');
      btn.classList.toggle('off', isHidden);
    });
  }

  showReasoningBubble(playerId, reasoning) {
    let bubble = this.reasoningBubbles.get(playerId);

    if (!bubble) {
      const el = document.createElement('div');
      el.className = 'reasoning-bubble';
      el.dataset.player = playerId;
      el.innerHTML = `
        <div class="bubble-name">${this.getPlayerName(playerId)}</div>
        <div class="bubble-text"></div>
      `;
      this.elements.reasoningContainer.appendChild(el);
      bubble = { element: el, hideTimer: null, typingCancel: null };
      this.reasoningBubbles.set(playerId, bubble);

      // Wire hover-to-persist
      setupBubbleHover(el, bubble, (delay) => this._scheduleBubbleHide(playerId, delay));
    }

    // Cancel any in-progress typing
    if (bubble.typingCancel) {
      bubble.typingCancel();
      bubble.typingCancel = null;
    }

    if (bubble.hideTimer) {
      clearTimeout(bubble.hideTimer);
      bubble.hideTimer = null;
    }

    const textEl = bubble.element.querySelector('.bubble-text');
    bubble.element.classList.remove('fade-out');
    void bubble.element.offsetHeight;
    bubble.element.classList.add('visible', 'typing');

    // Start typing animation
    const { promise, cancel } = typeText(textEl, reasoning, 40);
    bubble.typingCancel = cancel;

    promise.then(() => {
      bubble.typingCancel = null;
      bubble.element.classList.remove('typing');
      // Auto-hide 4.5s after typing completes
      this._scheduleBubbleHide(playerId, 4500);
    });
  }

  _scheduleBubbleHide(playerId, delayMs) {
    const bubble = this.reasoningBubbles.get(playerId);
    if (!bubble) return;

    if (bubble.hideTimer) {
      clearTimeout(bubble.hideTimer);
    }

    bubble.hideTimer = setTimeout(() => {
      bubble.element.classList.add('fade-out');
      bubble.element.classList.remove('visible');
      bubble.hideTimer = null;
    }, delayMs);
  }

  /**
   * Set or update an action tag badge on a player's reasoning bubble.
   * If no bubble exists yet, creates a minimal one so the tag is visible.
   */
  setBubbleActionTag(playerId, label, cssClass) {
    let bubble = this.reasoningBubbles.get(playerId);

    if (!bubble) {
      // Create a minimal bubble just for the action tag
      this.showReasoningBubble(playerId, '');
      bubble = this.reasoningBubbles.get(playerId);
      if (!bubble) return;
    }

    // Remove existing tag if any
    const existing = bubble.element.querySelector('.bubble-action-tag');
    if (existing) existing.remove();

    const tag = document.createElement('span');
    tag.className = `bubble-action-tag action-${cssClass}`;
    tag.textContent = label;

    // Insert after bubble-name, before bubble-text
    const textEl = bubble.element.querySelector('.bubble-text');
    bubble.element.insertBefore(tag, textEl);
  }

  worldToScreen(worldPos) {
    const vec = worldPos.clone();
    vec.project(this.renderer.camera);
    return {
      x: (vec.x * 0.5 + 0.5) * window.innerWidth,
      y: (-vec.y * 0.5 + 0.5) * window.innerHeight,
    };
  }

  updateBubblePositions() {
    for (const [playerId, bubble] of this.reasoningBubbles) {
      const pos = this.renderer.getPlayerPosition(playerId);
      pos.y += 3.5;
      const screen = this.worldToScreen(pos);
      bubble.element.style.left = `${screen.x}px`;
      bubble.element.style.top = `${screen.y}px`;
    }
  }

  getPlayerName(playerId) {
    return this.stateManager?.getPlayerName(playerId) ?? `Player ${playerId}`;
  }

  getClaimedRole(action) {
    const actionType = action.action_type || Object.keys(action)[0];
    const roleMap = {
      tax: 'duke',
      assassinate: 'assassin',
      steal: 'captain',
      exchange: 'ambassador',
    };
    return roleMap[actionType] || null;
  }

  updateConnectionStatus(status) {
    if (status === 'connected') {
      this.elements.connectionStatus.classList.add('hidden');
      if (!this.readySent) {
        this.readySent = true;
        this.notifyParent('ready');
      }
    } else if (status === 'reconnecting') {
      this.elements.connectionStatus.classList.remove('hidden');
      this.elements.connectionStatus.querySelector('.status-text').textContent = 'Reconnecting...';
    } else if (status === 'failed') {
      this.elements.connectionStatus.classList.remove('hidden');
      this.elements.connectionStatus.querySelector('.status-text').textContent = 'Connection lost';
      this.elements.connectionStatus.querySelector('.status-dot').style.background = '#ff4444';
      this.notifyParent('error', { error: 'WebSocket connection failed' });
    }
  }

  showError(message) {
    this.elements.phaseIndicator.textContent = 'Error';
    this.elements.phaseIndicator.style.borderColor = '#ff4444';
    this.elements.phaseIndicator.style.color = '#ff4444';

    this.elements.actionText.textContent = message;
    this.elements.actionRibbon.classList.remove('hidden');
    this.elements.actionRibbon.style.borderColor = '#ff4444';

    this.notifyParent('error', { error: message });
  }

  setupReplayController() {
    this.replayController = new ReplayController({
      onEvent: (type, event) => this.handleEvent(type, event[type] || event),
      onSilentEvent: (type, event) => this.stateManager.applySilentEvent(type, event),
      onReset: () => this.stateManager.resetState(),
      onProgress: (current, total) => this.updateReplayUI(current, total),
      onPlayStateChange: (playing) => this.updatePlayPauseButton(playing),
    });

    this.stateManager.replayController = this.replayController;

    // Show replay controls
    document.getElementById('replay-controls').classList.remove('hidden');
    this.wireReplayControls();

    console.log('[CoupViewer] Replay mode enabled');
  }

  wireReplayControls() {
    const playPause = document.getElementById('replay-play-pause');
    const stepBack = document.getElementById('replay-step-back');
    const stepFwd = document.getElementById('replay-step-fwd');
    const scrubber = document.getElementById('replay-scrubber');
    const speedSelect = document.getElementById('replay-speed');

    playPause.addEventListener('click', () => this.replayController.togglePlayPause());
    stepBack.addEventListener('click', () => this.replayController.stepBackward());
    stepFwd.addEventListener('click', () => this.replayController.stepForward());

    scrubber.addEventListener('input', (e) => {
      const index = parseInt(e.target.value, 10);
      this.replayController.seek(index);
    });

    speedSelect.addEventListener('change', (e) => {
      this.replayController.setSpeed(parseFloat(e.target.value));
    });
  }

  updateReplayUI(current, total) {
    const scrubber = document.getElementById('replay-scrubber');
    const position = document.getElementById('replay-position');

    if (scrubber) {
      scrubber.max = total - 1;
      scrubber.value = current;
    }
    if (position) {
      position.textContent = `${current + 1} / ${total}`;
    }
  }

  updatePlayPauseButton(playing) {
    const btn = document.getElementById('replay-play-pause');
    if (btn) {
      btn.innerHTML = playing ? '&#x23F8;' : '&#x25B6;';
      btn.title = playing ? 'Pause' : 'Play';
    }
  }

  delay(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  notifyParent(type, extra = {}) {
    if (window.parent === window) return;
    window.parent.postMessage({
      source: 'coup-viewer',
      type,
      timestamp: Date.now(),
      matchId: this.matchId,
      ...extra,
    }, '*');
  }
}

// Initialize viewer when DOM is ready
document.addEventListener('DOMContentLoaded', () => {
  window.coupViewer = new CoupViewer();
});
