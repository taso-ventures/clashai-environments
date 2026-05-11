/**
 * Poker Spectator Viewer Orchestrator
 *
 * Wires renderer + state manager + DOM overlay. Same bootstrap pattern as
 * ttt/c4/wordle — REST initial snapshot, WS subscribe, reconcile the
 * holographic table on every action event.
 */

import { PokerRenderer } from './poker-render.js';
import { PokerState } from './poker-state.js';

const ROUND_LABELS = {
  pre_flop: 'PRE-FLOP',
  flop: 'FLOP',
  turn: 'TURN',
  river: 'RIVER',
  showdown: 'SHOWDOWN',
  preflop: 'PRE-FLOP',
};

class PokerViewer {
  constructor() {
    const url = new URL(window.location.href);
    this.matchId = url.searchParams.get('matchId') || url.searchParams.get('match_id');

    this.canvas = document.getElementById('three-canvas');
    this.handCounter = document.getElementById('hand-counter');
    this.roundIndicator = document.getElementById('round-indicator');
    this.connBadge = document.getElementById('conn-badge');
    this.potValue = document.getElementById('pot-value');
    this.stack0 = document.querySelector('.stack.stack-0');
    this.stack1 = document.querySelector('.stack.stack-1');
    this.stackName0 = document.getElementById('stack-name-0');
    this.stackName1 = document.getElementById('stack-name-1');
    this.stackValue0 = document.getElementById('stack-value-0');
    this.stackValue1 = document.getElementById('stack-value-1');
    this.gameOverEl = document.getElementById('game-over-overlay');
    this.winnerTitle = document.getElementById('winner-title');
    this.winnerName = document.getElementById('winner-name');
    this.winnerReason = document.getElementById('winner-reason');
    this.connStatus = document.getElementById('connection-status');

    this.renderer = null;
    this.state = null;
  }

  async init() {
    if (!this.matchId) {
      this._showError('Missing matchId in URL.');
      return;
    }
    this.state = new PokerState(this.matchId);

    try {
      await this.state.loadInitialState();
    } catch (err) {
      console.error('[PokerViewer] Failed to load initial state:', err);
      this._showError('Failed to connect to match');
      return;
    }

    this.renderer = new PokerRenderer(this.canvas);

    this.state.onConnectionChange = (s) => this._setConnectionStatus(s);
    this.state.onHandSnapshot = (snapshot) => this._renderSnapshot(snapshot);
    this.state.onMatchOver = (winner, profits) => this._handleMatchOver(winner, profits);

    await this.renderer.charactersLoaded;

    // Set player labels from name map (or fallback) once characters loaded.
    if (this.stackName0) this.stackName0.textContent = this.state.displayName(0);
    if (this.stackName1) this.stackName1.textContent = this.state.displayName(1);

    // Render initial snapshot
    this._renderSnapshot({
      handNumber: this.state.handNumber,
      maxHands: this.state.maxHands,
      profits: this.state.profits,
      phase: this.state.phase,
      button: this.state.button,
      currentHand: this.state.currentHand,
    });

    if (this.state.isTerminal()) {
      let winner = null;
      if (this.state.profits[0] > this.state.profits[1]) winner = 0;
      else if (this.state.profits[1] > this.state.profits[0]) winner = 1;
      this._handleMatchOver(winner, this.state.profits);
    }

    const tick = () => {
      this.renderer.update();
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);

    this.state.connect();
  }

  _renderSnapshot(snapshot) {
    const { handNumber, maxHands, profits, currentHand, button } = snapshot;

    // Header — hand counter + round indicator
    if (this.handCounter) {
      this.handCounter.textContent = maxHands > 0
        ? `Hand ${handNumber}/${maxHands}`
        : `Hand ${handNumber}`;
    }
    if (this.roundIndicator) {
      if (currentHand && !currentHand.finished) {
        const roundKey = currentHand.round?.toLowerCase?.() ?? '';
        this.roundIndicator.textContent = ROUND_LABELS[roundKey] ?? roundKey.toUpperCase();
      } else {
        this.roundIndicator.textContent = '';
      }
    }

    // HUD — pot + stacks + acting indicator
    if (this.potValue) {
      this.potValue.textContent = currentHand?.pot ?? 0;
    }
    if (this.stackValue0) {
      this.stackValue0.textContent = currentHand?.stacks?.[0] ?? '–';
    }
    if (this.stackValue1) {
      this.stackValue1.textContent = currentHand?.stacks?.[1] ?? '–';
    }
    const actionOn = currentHand?.action_on;
    if (this.stack0) this.stack0.classList.toggle('acting', actionOn === 0 && !currentHand?.finished);
    if (this.stack1) this.stack1.classList.toggle('acting', actionOn === 1 && !currentHand?.finished);

    // 3D scene
    this.renderer.syncHand({ currentHand, button });
  }

  _handleMatchOver(winner, profits) {
    if (!this.gameOverEl) return;
    this.gameOverEl.classList.remove('hidden', 'p1-wins', 'p2-wins');

    if (winner === 0) this.gameOverEl.classList.add('p1-wins');
    else if (winner === 1) this.gameOverEl.classList.add('p2-wins');

    if (this.winnerTitle) {
      this.winnerTitle.textContent = winner === null ? 'TIE' : 'VICTORY';
    }
    if (this.winnerName) {
      if (winner === null) {
        this.winnerName.textContent = 'Profits tied';
      } else {
        const margin = Math.abs(profits[0] - profits[1]);
        this.winnerName.textContent = `${this.state.displayName(winner)} (+${margin})`;
      }
    }
    if (this.winnerReason) {
      this.winnerReason.textContent = `Final ${profits[0]} : ${profits[1]}`;
    }
  }

  _setConnectionStatus(status) {
    if (!this.connBadge) return;
    this.connBadge.classList.remove('connecting', 'running', 'terminal');
    switch (status) {
      case 'connecting':
        this.connBadge.classList.add('connecting');
        this.connBadge.textContent = 'CONNECTING';
        if (this.connStatus) this.connStatus.classList.add('hidden');
        break;
      case 'running':
        this.connBadge.classList.add('running');
        this.connBadge.textContent = 'LIVE';
        if (this.connStatus) this.connStatus.classList.add('hidden');
        break;
      case 'reconnecting':
        this.connBadge.classList.add('connecting');
        this.connBadge.textContent = 'RECONNECTING';
        if (this.connStatus) this.connStatus.classList.remove('hidden');
        break;
      case 'terminal':
        this.connBadge.classList.add('terminal');
        this.connBadge.textContent = 'COMPLETE';
        if (this.connStatus) this.connStatus.classList.add('hidden');
        break;
      default:
        break;
    }
  }

  _showError(msg) {
    if (this.gameOverEl) {
      this.gameOverEl.classList.remove('hidden');
      if (this.winnerTitle) this.winnerTitle.textContent = 'ERROR';
      if (this.winnerName) this.winnerName.textContent = msg;
      if (this.winnerReason) this.winnerReason.textContent = '';
    }
  }
}

const viewer = new PokerViewer();
viewer.init().catch((err) => {
  console.error('[PokerViewer] init failed:', err);
});
