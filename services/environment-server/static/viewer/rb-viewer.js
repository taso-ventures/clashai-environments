/**
 * Red Button Spectator Viewer - Main Orchestrator
 *
 * Initializes the 3D renderer, state manager, and WebSocket connection.
 * Wires state callbacks to renderer methods and DOM updates.
 * Supports replay mode for completed matches with paced playback.
 */

import * as THREE from 'three';
import { RedButtonRenderer } from './rb-render.js';
import { RedButtonState } from './rb-state.js';
import { ReplayController } from './shared/replay-controller.js';
import { createShockwave, createVictoryParticles } from './rb-anim.js';
import { typeText, setupBubbleHover } from './shared/shared-bubble-effects.js';

class RedButtonViewer {
  constructor() {
    this.matchId = null;
    this.renderer = null;
    this.state = null;
    this.ws = null;
    this.activeAnimations = [];
    this.reasoningBubbles = new Map(); // role → { element, hideTimer, typingCancel }
    this.replayController = null;
    this.isReplay = false;

    // DOM refs
    this.elements = {
      roundCounter: document.getElementById('round-counter'),
      turnIndicator: document.getElementById('turn-indicator'),
      connBadge: document.getElementById('conn-badge'),
      conversationFeed: document.getElementById('conversation-feed'),
      reasoningContainer: document.getElementById('reasoning-bubbles'),
      gameOverOverlay: document.getElementById('game-over-overlay'),
      winnerTitle: document.getElementById('winner-title'),
      winnerName: document.getElementById('winner-name'),
      winnerReason: document.getElementById('winner-reason'),
      connectionStatus: document.getElementById('connection-status'),
    };

    this.init();
  }

  async init() {
    // Parse URL params
    const params = new URLSearchParams(window.location.search);
    const matchId = params.get('match_id') || params.get('matchId');

    if (!matchId) {
      this._setBadge('connecting', 'NO MATCH ID');
      return;
    }

    this.matchId = matchId;

    // Apply optional agent color overrides from query params (hex without #)
    const p1Color = params.get('p1_color');
    const p2Color = params.get('p2_color');
    if (p1Color) {
      document.documentElement.style.setProperty('--persuader', `#${p1Color}`);
      document.documentElement.style.setProperty('--persuader-dim', `rgba(${parseInt(p1Color.slice(0,2),16)}, ${parseInt(p1Color.slice(2,4),16)}, ${parseInt(p1Color.slice(4,6),16)}, 0.12)`);
    }
    if (p2Color) {
      document.documentElement.style.setProperty('--resistor', `#${p2Color}`);
      document.documentElement.style.setProperty('--resistor-dim', `rgba(${parseInt(p2Color.slice(0,2),16)}, ${parseInt(p2Color.slice(2,4),16)}, ${parseInt(p2Color.slice(4,6),16)}, 0.12)`);
    }

    // Reasoning toggle
    this.setupReasoningToggle();

    // Init renderer
    const canvas = document.getElementById('three-canvas');
    this.renderer = new RedButtonRenderer(canvas, {
      persuaderColor: p1Color ? parseInt(p1Color, 16) : undefined,
      resistorColor: p2Color ? parseInt(p2Color, 16) : undefined,
    });

    // Wait for characters to load
    await this.renderer.charactersLoaded;

    // Init state manager
    this.state = new RedButtonState();
    this._wireStateCallbacks();

    // Check match status to detect replay mode
    await this._detectReplayMode();

    // Connect WebSocket
    this._connect();

    // Start render loop
    this._startRenderLoop();
  }

  async _detectReplayMode() {
    try {
      const wsBase =
        new URLSearchParams(window.location.search).get('ws_base') ||
        new URLSearchParams(window.location.search).get('wsBase') ||
        window.location.origin;
      const resp = await fetch(`${wsBase}/matches/${this.matchId}/status`);
      if (resp.ok) {
        const status = await resp.json();
        this.isReplay = !!status.is_terminal;
        if (this.isReplay) {
          this._setupReplayController();
        }
      }
    } catch (e) {
      console.warn('[RBViewer] Could not fetch match status:', e);
    }
  }

  _setupReplayController() {
    this.replayController = new ReplayController({
      onEvent: (type, event) => {
        // ReplayController passes the raw event; process it through state
        this.state.processEvent(event);
      },
      onSilentEvent: (type, event) => {
        this.state.applySilentEvent(type, event);
      },
      onReset: () => {
        this.state.resetState();
        this._clearConversation();
        this._clearReasoningBubbles();
      },
      onProgress: (current, total) => this._updateReplayUI(current, total),
      onPlayStateChange: (playing) => this._updatePlayPauseButton(playing),
    });

    // Override getEventType for Red Button's UnifiedEvent envelope format
    // Extract inner event_name from action for delay lookup
    this.replayController.getEventType = (event) => {
      const action = event.action;
      if (Array.isArray(action) && action.length > 0) return action[0].event_name || 'default';
      if (action?.event_name) return action.event_name;
      if (event.event_type === 'system' && action?.reasoning) return 'agent_reasoning';
      return event.event_type || 'default';
    };

    // Add Red Button specific event delays
    Object.assign(this.replayController.eventDelays, {
      message_spoken: 1500,
      button_pressed: 2000,
      match_start: 500,
      terminal: 2000,
      system: 1500,
    });

    // Wire replay controller into state manager
    this.state.replayController = this.replayController;

    // Show replay controls
    const controls = document.getElementById('replay-controls');
    if (controls) {
      controls.classList.remove('hidden');
      this._wireReplayControls();
    }

    this._setBadge('terminal', 'REPLAY');
    console.log('[RBViewer] Replay mode enabled');
  }

  _wireReplayControls() {
    const playPause = document.getElementById('replay-play-pause');
    const stepBack = document.getElementById('replay-step-back');
    const stepFwd = document.getElementById('replay-step-fwd');
    const scrubber = document.getElementById('replay-scrubber');
    const speedSelect = document.getElementById('replay-speed');

    playPause?.addEventListener('click', () => this.replayController.togglePlayPause());
    stepBack?.addEventListener('click', () => this.replayController.stepBackward());
    stepFwd?.addEventListener('click', () => this.replayController.stepForward());

    scrubber?.addEventListener('input', (e) => {
      const index = parseInt(e.target.value, 10);
      this.replayController.seek(index);
    });

    speedSelect?.addEventListener('change', (e) => {
      this.replayController.setSpeed(parseFloat(e.target.value));
    });
  }

  _updateReplayUI(current, total) {
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

  _updatePlayPauseButton(playing) {
    const btn = document.getElementById('replay-play-pause');
    if (btn) {
      btn.innerHTML = playing ? '&#x23F8;' : '&#x25B6;';
      btn.title = playing ? 'Pause' : 'Play';
    }
  }

  _wireStateCallbacks() {
    this.state.onGameStarted = (playerNames) => {
      console.log('[RBViewer] Game started:', playerNames);
    };

    this.state.onRoundAdvance = (round) => {
      this.elements.roundCounter.textContent = `Round ${round}`;
    };

    this.state.onTurnIndicator = (actor) => {
      const el = this.elements.turnIndicator;
      const label = actor === 'persuader' ? "PERSUADER'S TURN" : "RESISTOR'S TURN";
      el.textContent = label;
      el.className = actor === 'persuader' ? 'persuader-turn' : 'resistor-turn';
      this.renderer.setActivePlayer(actor);
    };

    this.state.onMessageSpoken = (role, text, turn) => {
      this._addChatMessage(role, text, turn);
    };

    this.state.onActionTaken = (role, actionType, turn) => {
      // Show non-message actions (ignore) as system messages in the chat
      if (actionType === 'ignore_other_agent') {
        this._addActionIndicator(role, 'ignored the message', turn);
      }
    };

    this.state.onReasoning = (role, text, turn) => {
      if (!text) return;
      this._showReasoningBubble(role, text);
    };

    this.state.onButtonPressed = (turn) => {
      this.renderer.animateButtonPress();
      // Add shockwave
      const shockwave = createShockwave(this.renderer.scene);
      this.activeAnimations.push(shockwave);
    };

    this.state.onGameOver = (winnerRole, reason) => {
      this._handleGameOver(winnerRole, reason);
    };
  }

  _connect() {
    const wsBase =
      new URLSearchParams(window.location.search).get('ws_base') ||
      new URLSearchParams(window.location.search).get('wsBase') ||
      window.location.origin.replace(/^http/, 'ws');

    const wsUrl = `${wsBase}/matches/${this.matchId}/spectator/ws`;
    this.ws = new WebSocket(wsUrl);

    this.ws.addEventListener('open', () => {
      if (!this.isReplay) {
        this._setBadge('running', 'LIVE');
      }
      this.elements.connectionStatus.classList.add('hidden');
    });

    this.ws.addEventListener('message', (evt) => {
      try {
        const event = JSON.parse(evt.data);
        // Use queueEvent for replay support (buffers events when in replay mode)
        this.state.queueEvent(event);
      } catch (e) {
        console.warn('Failed to parse event:', e);
      }
    });

    this.ws.addEventListener('close', () => {
      if (!this.state.isTerminal && !this.isReplay) {
        this._setBadge('connecting', 'RECONNECTING');
        this.elements.connectionStatus.classList.remove('hidden');
        setTimeout(() => this._connect(), 3000);
      } else if (!this.isReplay) {
        this._setBadge('terminal', 'ENDED');
      }
    });

    this.ws.addEventListener('error', () => {
      this._setBadge('connecting', 'ERROR');
    });
  }

  _startRenderLoop() {
    const clock = new THREE.Clock();

    const animate = () => {
      requestAnimationFrame(animate);
      const delta = clock.getDelta();

      // Update renderer
      this.renderer.update(delta);

      // Update active animations (shockwave, particles, etc.)
      this.activeAnimations = this.activeAnimations.filter((anim) =>
        anim.update(delta),
      );

      // Update reasoning bubble positions
      this._updateBubblePositions();

      // Render
      this.renderer.render();
    };

    animate();
  }

  // =================== DOM Helpers ===================

  _setBadge(cls, text) {
    const badge = this.elements.connBadge;
    badge.className = `conn-badge ${cls}`;
    badge.textContent = text;
  }

  _addChatMessage(role, text, turn) {
    const feed = this.elements.conversationFeed;

    // Clear empty state on first message
    const empties = feed.querySelectorAll('.empty-state');
    empties.forEach((e) => e.remove());

    const div = document.createElement('div');
    div.className = `chat-msg ${role}`;

    const meta = document.createElement('div');
    meta.className = 'chat-meta';
    meta.textContent = `${role} \u00b7 Turn ${turn || this.state.round}`;

    const bubble = document.createElement('div');
    bubble.className = `chat-bubble ${role}`;
    bubble.textContent = text;

    div.appendChild(meta);
    div.appendChild(bubble);
    feed.appendChild(div);
    feed.scrollTop = feed.scrollHeight;
  }

  _addActionIndicator(role, label, turn) {
    const feed = this.elements.conversationFeed;

    const empties = feed.querySelectorAll('.empty-state');
    empties.forEach((e) => e.remove());

    const div = document.createElement('div');
    div.className = 'chat-action-indicator';

    const text = document.createElement('span');
    text.className = `action-label ${role}`;
    text.textContent = `${role} ${label} · Turn ${turn || this.state.round}`;

    div.appendChild(text);
    feed.appendChild(div);
    feed.scrollTop = feed.scrollHeight;
  }

  _clearConversation() {
    const feed = this.elements.conversationFeed;
    feed.innerHTML = '<div class="empty-state">Waiting for messages...</div>';

    // Reset game-over overlay
    this.elements.gameOverOverlay.classList.add('hidden');

    // Reset round counter and turn indicator
    this.elements.roundCounter.textContent = 'Round 1';
    this.elements.turnIndicator.textContent = '';
    this.elements.turnIndicator.className = '';

    // Reset renderer state
    this.renderer.buttonPressed = false;
    this.renderer.buttonPressT = 0;
  }

  _clearReasoningBubbles() {
    for (const [, bubble] of this.reasoningBubbles) {
      if (bubble.typingCancel) bubble.typingCancel();
      if (bubble.hideTimer) clearTimeout(bubble.hideTimer);
      bubble.element.remove();
    }
    this.reasoningBubbles.clear();
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

  _showReasoningBubble(role, reasoning) {
    let bubble = this.reasoningBubbles.get(role);

    if (!bubble) {
      const el = document.createElement('div');
      el.className = 'reasoning-bubble';
      el.dataset.role = role;
      el.innerHTML = `
        <div class="bubble-name">${role}</div>
        <div class="bubble-text"></div>
      `;
      this.elements.reasoningContainer.appendChild(el);
      bubble = { element: el, hideTimer: null, typingCancel: null };
      this.reasoningBubbles.set(role, bubble);

      // Hover-to-persist
      setupBubbleHover(el, bubble, (delay) =>
        this._scheduleBubbleHide(role, delay),
      );
    }

    // Cancel in-progress typing
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

    const { promise, cancel } = typeText(textEl, reasoning, 40);
    bubble.typingCancel = cancel;

    promise.then(() => {
      bubble.typingCancel = null;
      bubble.element.classList.remove('typing');
      this._scheduleBubbleHide(role, 4500);
    });
  }

  _scheduleBubbleHide(role, delayMs) {
    const bubble = this.reasoningBubbles.get(role);
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

  _updateBubblePositions() {
    for (const [role, bubble] of this.reasoningBubbles) {
      const worldPos = this.renderer.getCharacterPosition(role);
      const screen = this._worldToScreen(worldPos);
      bubble.element.style.left = `${screen.x}px`;
      bubble.element.style.top = `${screen.y}px`;
    }
  }

  _worldToScreen(worldPos) {
    const vec = worldPos.clone();
    vec.project(this.renderer.camera);
    return {
      x: (vec.x * 0.5 + 0.5) * window.innerWidth,
      y: (-vec.y * 0.5 + 0.5) * window.innerHeight,
    };
  }

  _handleGameOver(winnerRole, reason) {
    // Note: button press + shockwave already triggered by the onButtonPressed
    // callback when the engine emits ButtonPressed before GameOver.

    // Victory particles
    const charPos = this.renderer.getCharacterPosition(winnerRole);
    const char = this.renderer.characters[winnerRole];
    const color = char?.color ??
      (winnerRole === 'persuader' ? 0x3b82f6 : 0x22c55e);
    const particles = createVictoryParticles(
      this.renderer.scene,
      charPos,
      color,
    );
    this.activeAnimations.push(particles);

    // Renderer victory effect
    this.renderer.animateVictory(winnerRole);

    // Update badge
    if (!this.isReplay) {
      this._setBadge('terminal', 'ENDED');
    }

    // Show game-over overlay after short delay
    setTimeout(() => {
      const overlay = this.elements.gameOverOverlay;
      overlay.className = winnerRole === 'persuader'
        ? 'persuader-wins'
        : 'resistor-wins';

      const titles = {
        persuader: 'BUTTON PRESSED!',
        resistor: 'BUTTON HELD!',
      };
      this.elements.winnerTitle.textContent = titles[winnerRole] || 'GAME OVER';
      this.elements.winnerName.textContent =
        winnerRole === 'persuader' ? 'Persuader Wins' : 'Resistor Wins';
      this.elements.winnerReason.textContent = reason || '';
      overlay.classList.remove('hidden');
    }, 500);
  }
}

// Initialize when DOM ready
document.addEventListener('DOMContentLoaded', () => {
  window.rbViewer = new RedButtonViewer();
});
