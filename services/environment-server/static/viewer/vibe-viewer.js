/**
 * Vibe Check Viewer — Main Orchestrator
 *
 * Initializes renderer + state manager, wires spectator events to
 * render updates, HTML HUD, and reasoning bubbles.
 * Mirrors coup-viewer.js pattern.
 */
import { typeText, setupBubbleHover } from './shared/shared-bubble-effects.js';
import { CameraPresetController } from './shared/shared-camera-controls.js';
import { getPlayerColor } from './vibe-characters.js';
import { VibeCheckRenderer } from './vibe-render.js';
import { VibeCheckStateManager } from './vibe-state.js';
import { ReplayController } from './shared/replay-controller.js';

// Phase label mapping
const PHASE_LABELS = {
  waiting: 'WAITING',
  game_started: 'GAME STARTED',
  clue_phase: 'CLUE PHASE',
  guess_phase: 'GUESS PHASE',
  steal_phase: 'STEAL PHASE',
  resolving: 'RESOLVING',
  game_over: 'GAME OVER',
};

class VibeCheckViewer {
  constructor() {
    this.renderer = null;
    this.stateManager = null;
    this.replayController = null;
    this.isReplay = false;
    this.matchId = null;
    this.sceneInitialized = false;
    this.players = [];
    this.teams = [];
    this.currentActiveTeam = null;
    this.currentCluegiver = null;

    // HTML HUD element refs
    this.hudElements = {};

    // Reasoning bubbles
    this.reasoningContainer = null;
    this.activeBubbles = new Map(); // playerId -> { el, timer }

    // Camera presets
    this.cameraPresets = null;
  }

  async init() {
    // Parse match ID from URL
    const params = new URLSearchParams(window.location.search);
    this.matchId = params.get('matchId');

    if (!this.matchId || !/^[0-9A-HJKMNP-TV-Z]{26}$/i.test(this.matchId)) {
      console.error('[VibeViewer] Invalid or missing matchId');
      this.showConnectionStatus('failed', 'Invalid match ID');
      return;
    }

    // Check embed mode
    if (params.get('embed') === '1') {
      document.body.classList.add('embed-mode');
    }

    // Cache HTML HUD elements
    this.hudElements = {
      roundNumber: document.getElementById('round-number'),
      teamAScore: document.querySelector('.team-a-score'),
      teamBScore: document.querySelector('.team-b-score'),
      phaseIndicator: document.getElementById('phase-indicator'),
    };
    this.reasoningContainer = document.getElementById('reasoning-bubbles');

    // Reasoning toggle
    this.setupReasoningToggle();

    // Initialize renderer
    const canvas = document.getElementById('three-canvas');
    this.renderer = new VibeCheckRenderer(canvas);

    // Initialize state manager
    this.stateManager = new VibeCheckStateManager(
      this.matchId,
      (type, event) => this.handleEvent(type, event),
      (state) => this.handleStateChange(state),
    );

    this.stateManager.setConnectionStatusCallback((status) => {
      this.handleConnectionStatus(status);
    });

    // Bootstrap: REST state -> scene init -> replay check -> WebSocket -> render loop
    await this.stateManager.loadInitialState();
    this.initializeFromState();

    // Detect replay mode from initial state
    const state = this.stateManager.getState();
    this.isReplay = state.isGameOver || state.phase === 'game_over';

    if (this.isReplay) {
      this.setupReplayController();
    }

    this.stateManager.connect();
    this.startRenderLoop();
  }

  initializeFromState() {
    const state = this.stateManager.getState();

    // Set up player sprites from REST-bootstrapped data
    const players = Object.values(state.players);
    const teams = Object.values(state.teams);
    if (players.length > 0) {
      this.players = players;
      this.teams = teams;
      this.renderer.setupPlayers(players, teams);
      this.sceneInitialized = true;
    }

    // Update HTML HUD from initial state
    if (state.round > 0) {
      this.updateHtmlRound(state.round);
    }
    this.updateHtmlPhase(state.phase);

    // Team scores
    const teamAScore = state.teams[0]?.score || 0;
    const teamBScore = state.teams[1]?.score || 0;
    this.updateHtmlScores(teamAScore, teamBScore);

    // Spectrum labels
    if (state.spectrum) {
      this.renderer.updateSpectrumLabels(
        state.spectrum.left_endpoint,
        state.spectrum.right_endpoint,
      );
    }

    // Clue
    if (state.clue) {
      this.renderer.showClue(state.clue);
    }

    // Guess position
    if (state.guessPosition !== null && state.guessPosition !== undefined) {
      this.renderer.setGuessMarkerPosition(state.guessPosition);
    }

    this.sceneInitialized = true;

    // Initialize camera presets
    this.initCameraPresets();
  }

  initCameraPresets() {
    const homeConfig = {
      position: { x: 12, y: 10, z: 16 },
      lookAt: { x: 0, y: 1, z: 0 },
    };

    this.cameraPresets = new CameraPresetController(
      this.renderer.camera,
      homeConfig,
      { x: 0, y: 1, z: 0 },
    );

    // Register player positions and colors from renderer
    for (const [playerId] of this.renderer.playerGroups) {
      const pos = this.renderer.getPlayerPosition(playerId);
      const color = getPlayerColor(playerId);
      this.cameraPresets.addPlayer(playerId, pos, color);
    }

    // Wire into renderer so orbital drift is gated
    this.renderer.cameraPresetController = this.cameraPresets;

    // Build UI in the overlay
    const overlay = document.getElementById('ui-overlay');
    this.cameraPresets.createUI(overlay);
  }

  // ─── HTML HUD Updates ───

  updateHtmlRound(round) {
    if (this.hudElements.roundNumber) {
      this.hudElements.roundNumber.textContent = `Round ${round}`;
    }
  }

  updateHtmlPhase(phase) {
    if (this.hudElements.phaseIndicator) {
      this.hudElements.phaseIndicator.textContent = PHASE_LABELS[phase] || phase.toUpperCase();
    }
  }

  updateHtmlScores(teamAScore, teamBScore) {
    if (this.hudElements.teamAScore) {
      this.hudElements.teamAScore.textContent = `TEAM A: ${teamAScore}`;
    }
    if (this.hudElements.teamBScore) {
      this.hudElements.teamBScore.textContent = `TEAM B: ${teamBScore}`;
    }
  }

  // ─── Reasoning Bubbles ───

  setupReasoningToggle() {
    const btn = document.getElementById('reasoning-toggle');
    if (!btn) return;
    btn.addEventListener('click', () => {
      const overlay = document.getElementById('ui-overlay');
      const isHidden = overlay.classList.toggle('reasoning-hidden');
      btn.classList.toggle('off', isHidden);
    });
  }

  /**
   * Show a reasoning bubble above a player's character.
   * @param {number} playerId
   * @param {string} reasoning
   * @param {object} [opts]
   * @param {boolean} [opts.persistent] — if true, bubble stays until manually removed (no auto-fade)
   */
  showReasoningBubble(playerId, reasoning, opts = {}) {
    if (!this.reasoningContainer) return;

    // Remove existing bubble for this player
    this.removeReasoningBubble(playerId);

    // Determine team from state manager
    const teamId = this.stateManager.getPlayerTeam(playerId);
    const team = teamId === 0 ? 'A' : 'B';

    // Create bubble element
    const bubble = document.createElement('div');
    bubble.className = 'reasoning-bubble';
    bubble.dataset.team = team;
    bubble.dataset.playerId = playerId;

    // Player label — use agent display name from state manager
    const label = document.createElement('div');
    label.className = 'bubble-player';
    label.textContent = this.stateManager.getPlayerName(playerId);
    bubble.appendChild(label);

    // Reasoning text element (typed in char-by-char)
    const text = document.createElement('div');
    text.className = 'bubble-text';
    bubble.appendChild(text);

    this.reasoningContainer.appendChild(bubble);

    const entry = { el: bubble, timer: null, typingCancel: null };
    this.activeBubbles.set(playerId, entry);

    // Wire hover-to-persist
    setupBubbleHover(bubble, entry, (delay) => this._scheduleBubbleHide(playerId, delay));

    // Trigger fade-in on next frame
    requestAnimationFrame(() => {
      bubble.classList.add('visible', 'typing');
    });

    // Start typing animation
    const { promise, cancel } = typeText(text, reasoning, 40);
    entry.typingCancel = cancel;

    promise.then(() => {
      entry.typingCancel = null;
      bubble.classList.remove('typing');
      // Persistent bubbles stay until the next round clears them
      if (!opts.persistent) {
        this._scheduleBubbleHide(playerId, 4500);
      }
    });
  }

  _scheduleBubbleHide(playerId, delayMs) {
    const entry = this.activeBubbles.get(playerId);
    if (!entry) return;

    if (entry.timer) {
      clearTimeout(entry.timer);
    }

    entry.timer = setTimeout(() => {
      entry.el.classList.remove('visible');
      entry.el.classList.add('fade-out');
      entry.timer = null;
      setTimeout(() => this.removeReasoningBubble(playerId), 500);
    }, delayMs);
  }

  removeReasoningBubble(playerId) {
    const entry = this.activeBubbles.get(playerId);
    if (!entry) return;
    if (entry.timer) clearTimeout(entry.timer);
    if (entry.typingCancel) entry.typingCancel();
    entry.el.remove();
    this.activeBubbles.delete(playerId);
  }

  removeAllReasoningBubbles() {
    for (const [id] of this.activeBubbles) {
      this.removeReasoningBubble(id);
    }
  }

  /**
   * Project player world positions to screen coordinates and reposition bubbles.
   * Called every frame in the render loop.
   */
  updateBubblePositions() {
    if (this.activeBubbles.size === 0) return;

    const camera = this.renderer.camera;
    const w = window.innerWidth;
    const h = window.innerHeight;

    // First pass: compute raw screen positions for all visible bubbles
    const positions = [];
    for (const [playerId, { el }] of this.activeBubbles) {
      const worldPos = this.renderer.getPlayerPosition(playerId);
      if (!worldPos) continue;

      // Offset above character head
      worldPos.y += 5.5;

      const projected = worldPos.project(camera);

      if (projected.z > 0 && projected.z < 1) {
        const sx = (projected.x * 0.5 + 0.5) * w;
        const sy = (-projected.y * 0.5 + 0.5) * h;
        positions.push({ el, sx, sy });
      } else {
        el.style.left = '-9999px';
      }
    }

    // Second pass: resolve overlaps with multiple iterations.
    // Sort top-to-bottom (lowest sy first) so upper bubbles stay put
    // and lower ones get pushed down.
    positions.sort((a, b) => a.sy - b.sy);

    const BUBBLE_W = 260;  // max-width from CSS
    // TODO: use el.getBoundingClientRect().height for exact per-bubble height
    const BUBBLE_H = 120;  // approximate rendered height (header + 3 lines)
    const PAD = 12;         // minimum gap between bubbles
    const MAX_ITERS = 4;    // multiple passes to resolve cascading overlaps

    for (let iter = 0; iter < MAX_ITERS; iter++) {
      let anyOverlap = false;
      for (let i = 0; i < positions.length; i++) {
        for (let j = i + 1; j < positions.length; j++) {
          const a = positions[i];
          const b = positions[j];

          const overlapX = Math.abs(a.sx - b.sx) < BUBBLE_W;
          const overlapY = Math.abs(a.sy - b.sy) < BUBBLE_H + PAD;

          if (overlapX && overlapY) {
            anyOverlap = true;
            // Push lower bubble (higher sy) downward
            const needed = BUBBLE_H + PAD - Math.abs(a.sy - b.sy);
            if (a.sy <= b.sy) {
              b.sy += needed;
            } else {
              a.sy += needed;
            }
          }
        }
      }
      if (!anyOverlap) break;
    }

    // Clamp to viewport so bubbles never go off-screen.
    // CSS uses `transform: translate(-50%, -100%)` so sx is bubble center
    // and sy is bubble bottom (content extends upward).
    const HALF_W = BUBBLE_W / 2;
    for (const pos of positions) {
      pos.sx = Math.max(HALF_W, Math.min(pos.sx, w - HALF_W));
      pos.sy = Math.max(BUBBLE_H, Math.min(pos.sy, h));
    }

    // Apply final positions
    for (const { el, sx, sy } of positions) {
      el.style.left = `${sx}px`;
      el.style.top = `${sy}px`;
    }
  }

  // ─── Render Loop ───

  startRenderLoop() {
    const animate = () => {
      requestAnimationFrame(animate);
      this.renderer.render();
      this.updateBubblePositions();

      // Position camera markers above player heads
      if (this.cameraPresets) {
        this.cameraPresets.updateMarkerPositions(
          (id) => this.renderer.getPlayerPosition(id),
          5.5,
        );
      }
    };
    animate();
  }

  // ─── Event Handlers ───

  async handleEvent(type, event) {
    const data = event[type];

    switch (type) {
      case 'game_started':
        await this.handleGameStarted(data);
        break;

      case 'round_started':
        await this.handleRoundStarted(data);
        break;

      case 'clue_given':
        await this.handleClueGiven(data);
        break;

      case 'agent_reasoning':
        this.showReasoningBubble(data.player, data.reasoning);
        console.log(`[VibeViewer] Agent ${data.player} reasoning: ${data.reasoning}`);
        break;

      case 'guess_submitted':
        await this.handleGuessSubmitted(data);
        break;

      case 'steal_guess_submitted':
        await this.handleStealGuessSubmitted(data);
        break;

      case 'target_revealed':
        await this.handleTargetRevealed(data);
        break;

      case 'score_update':
        await this.handleScoreUpdate(data);
        break;

      case 'game_over':
        await this.handleGameOver(data);
        break;
    }
  }

  async handleGameStarted(data) {
    this.updateHtmlPhase('game_started');
    this.sceneInitialized = true;

    // Store player/team data for role assignment
    this.players = data.players || [];
    this.teams = data.teams || [];

    // Build player positions and characters based on actual game data
    this.renderer.setupPlayers(this.players, this.teams);

    // Update HTML HUD with team/player info
    const teamAScore = data.teams.find(t => t.team_id === 0)?.score || 0;
    const teamBScore = data.teams.find(t => t.team_id === 1)?.score || 0;
    this.updateHtmlScores(teamAScore, teamBScore);

    // Reset all character states
    this.renderer.resetAllPlayerStates();
    this.removeAllReasoningBubbles();

    console.log(`[VibeViewer] Game started: ${data.teams.length} teams, ${data.players.length} players, target score: ${data.target_score}`);

    await this.delay(500);
  }

  async handleRoundStarted(data) {
    // Reset markers and zones from previous round
    this.renderer.hideMarkers();
    this.renderer.hideScoringZones();
    this.renderer.hideClue();
    this.renderer.hideScorePopup();

    // Reset all characters to idle, clear roles
    this.renderer.resetAllPlayerStates();
    this.removeAllReasoningBubbles();

    // Update HTML HUD
    this.updateHtmlRound(data.round);
    this.updateHtmlPhase('clue_phase');

    // Update spectrum labels from the actual prompt endpoints
    if (data.spectrum) {
      this.renderer.updateSpectrumLabels(
        data.spectrum.left_endpoint,
        data.spectrum.right_endpoint,
      );
    }

    // Assign role labels from round data
    this.currentActiveTeam = data.active_team;
    this.currentCluegiver = data.cluegiver;
    if (data.cluegiver !== undefined && data.cluegiver !== null) {
      this.renderer.setPlayerRole(data.cluegiver, 'CLUEGIVER');
    }

    // Label active team members as GUESSER, opposing as STEAL TEAM
    if (data.active_team !== undefined && this.players) {
      for (const player of this.players) {
        if (player.player_id === data.cluegiver) continue;
        if (player.team === data.active_team) {
          this.renderer.setPlayerRole(player.player_id, 'GUESSER');
        } else {
          this.renderer.setPlayerRole(player.player_id, 'STEAL TEAM');
        }
      }
    }

    // Auto-hide role labels after 1 second
    setTimeout(() => {
      if (this.players) {
        for (const player of this.players) {
          this.renderer.setPlayerRole(player.player_id, null);
        }
      }
    }, 1000);

    console.log(`[VibeViewer] Round ${data.round}: Team ${data.active_team} active, Cluegiver: ${data.cluegiver}, Spectrum: ${data.spectrum?.left_endpoint} / ${data.spectrum?.right_endpoint}`);

    await this.delay(300);
  }

  async handleClueGiven(data) {
    this.renderer.showClue(data.clue);
    this.updateHtmlPhase('guess_phase');

    // Cluegiver speaks + persistent bubble showing the clue for the rest of the round
    if (data.cluegiver !== undefined) {
      this.renderer.setPlayerPose(data.cluegiver, 'speaking');
      this.showReasoningBubble(data.cluegiver, `"${data.clue}"`, { persistent: true });
    }

    console.log(`[VibeViewer] Clue given: "${data.clue}" by player ${data.cluegiver}`);

    await this.delay(800);
  }

  async handleGuessSubmitted(data) {
    this.renderer.setGuessMarkerPosition(data.position);
    this.updateHtmlPhase('steal_phase');

    console.log(`[VibeViewer] Team ${data.team} guessed at position ${data.position.toFixed(3)}`);

    await this.delay(500);
  }

  async handleStealGuessSubmitted(data) {
    this.updateHtmlPhase('resolving');

    // Show steal direction marker at the guess position
    const state = this.stateManager.getState();
    if (state.guessPosition !== null) {
      this.renderer.setStealMarker(data.direction, state.guessPosition);
    }

    // Active team reacts (worried), steal team gets speaking pose
    if (this.players) {
      for (const player of this.players) {
        if (player.team === this.currentActiveTeam) {
          this.renderer.setPlayerPose(player.player_id, 'reactive');
        } else {
          this.renderer.setPlayerPose(player.player_id, 'speaking');
        }
      }
    }

    console.log(`[VibeViewer] Team ${data.team} steal guess: ${data.direction}`);

    await this.delay(500);
  }

  async handleTargetRevealed(data) {
    // Show scoring zones at the target position
    const state = this.stateManager.getState();
    this.renderer.showScoringZones(data.target_position, state.zoneConfig);

    // Score popup text
    const zoneLabels = {
      bullseye: { text: '+4 BULLSEYE!', color: 0xff3333 },
      near: { text: '+3 NEAR!', color: 0xffcc00 },
      far: { text: '+2 FAR', color: 0x44ff88 },
      miss: { text: 'MISS!', color: 0xff4444 },
    };
    const zoneKey = typeof data.active_zone === 'string'
      ? data.active_zone.toLowerCase()
      : Object.keys(data.active_zone || {})[0]?.toLowerCase() || 'miss';
    const zoneInfo = zoneLabels[zoneKey] || zoneLabels.miss;
    this.renderer.showScorePopup(zoneInfo.text, zoneInfo.color);

    console.log(`[VibeViewer] Target revealed at ${data.target_position.toFixed(3)}, zone: ${zoneKey}, steal correct: ${data.steal_correct}`);

    await this.delay(1500);
  }

  async handleScoreUpdate(data) {
    // Find team scores
    let teamAScore = 0;
    let teamBScore = 0;
    for (const [teamId, score] of data.scores) {
      if (teamId === 0) teamAScore = score;
      if (teamId === 1) teamBScore = score;
    }
    this.updateHtmlScores(teamAScore, teamBScore);

    console.log(`[VibeViewer] Scores — Team A: ${teamAScore}, Team B: ${teamBScore} (active +${data.active_points}, steal +${data.steal_points})`);

    await this.delay(500);
  }

  async handleGameOver(data) {
    this.renderer.hideScorePopup();
    this.updateHtmlPhase('game_over');
    this.removeAllReasoningBubbles();

    // Reset poses — winning team speaks, losing team reacts
    if (this.players && data.winner !== null && data.winner !== undefined) {
      for (const player of this.players) {
        if (player.team === data.winner) {
          this.renderer.setPlayerPose(player.player_id, 'speaking');
        } else {
          this.renderer.setPlayerPose(player.player_id, 'reactive');
        }
      }
    }

    // Show DOM game over overlay
    const overlay = document.getElementById('game-over-overlay');
    const winnerName = document.getElementById('winner-name');
    const finalScores = document.getElementById('final-scores');

    if (overlay) {
      if (data.winner !== null && data.winner !== undefined) {
        winnerName.textContent = `Team ${data.winner === 0 ? 'A' : 'B'} Wins!`;
        winnerName.style.color = data.winner === 0 ? '#00E5CC' : '#FF8844';
      } else {
        winnerName.textContent = 'Draw!';
        winnerName.style.color = '#ffffff';
      }

      if (data.final_scores) {
        const scoresText = data.final_scores
          .map(([tid, s]) => `Team ${tid === 0 ? 'A' : 'B'}: ${s}`)
          .join('  |  ');
        finalScores.textContent = scoresText;
      }

      overlay.classList.remove('hidden');
    }

    console.log(`[VibeViewer] Game over! Winner: ${data.winner !== null ? `Team ${data.winner}` : 'Draw'}`);

    await this.delay(2000);
  }

  // ─── State Change Handler ───

  handleStateChange(state) {
    // Sync HTML HUD with current state
    if (state.round > 0) {
      this.updateHtmlRound(state.round);
    }
    this.updateHtmlPhase(state.phase);

    const teamAScore = state.teams[0]?.score || 0;
    const teamBScore = state.teams[1]?.score || 0;
    this.updateHtmlScores(teamAScore, teamBScore);
  }

  // ─── Connection Status ───

  handleConnectionStatus(status) {
    const statusEl = document.getElementById('connection-status');
    const statusText = statusEl?.querySelector('.status-text');

    if (!statusEl) return;

    switch (status) {
      case 'connected':
        statusEl.classList.add('hidden');
        break;
      case 'reconnecting':
        statusEl.classList.remove('hidden');
        if (statusText) statusText.textContent = 'Reconnecting...';
        break;
      case 'failed':
        statusEl.classList.remove('hidden');
        if (statusText) statusText.textContent = 'Connection lost';
        break;
    }
  }

  showConnectionStatus(status, message) {
    const statusEl = document.getElementById('connection-status');
    const statusText = statusEl?.querySelector('.status-text');
    if (statusEl) {
      statusEl.classList.remove('hidden');
      if (statusText) statusText.textContent = message || status;
    }
  }

  // ─── Utilities ───

  setupReplayController() {
    this.replayController = new ReplayController({
      onEvent: (type, event) => this.handleEvent(type, event),
      onSilentEvent: (type, event) => this.stateManager.applySilentEvent(type, event),
      onReset: () => this.stateManager.resetState(),
      onProgress: (current, total) => this.updateReplayUI(current, total),
      onPlayStateChange: (playing) => this.updatePlayPauseButton(playing),
    });

    this.stateManager.replayController = this.replayController;

    // Show replay controls
    document.getElementById('replay-controls').classList.remove('hidden');
    this.wireReplayControls();

    console.log('[VibeViewer] Replay mode enabled');
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
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  dispose() {
    this.removeAllReasoningBubbles();
    if (this.stateManager) this.stateManager.dispose();
    if (this.renderer) this.renderer.dispose();
  }
}

// ─── Bootstrap ───

document.addEventListener('DOMContentLoaded', () => {
  const viewer = new VibeCheckViewer();
  window.vibeCheckViewer = viewer;
  viewer.init().catch(err => {
    console.error('[VibeViewer] Initialization failed:', err);
  });
});
