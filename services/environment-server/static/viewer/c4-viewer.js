/**
 * Connect Four Spectator Viewer Orchestrator
 *
 * Wires the renderer, state manager, and DOM overlay together. Same
 * bootstrap pattern as ttt-viewer.js — REST initial state, render the
 * snapshot, connect WS, reconcile board on each action event.
 */

import { ConnectFourRenderer } from './c4-render.js';
import { ConnectFourState } from './c4-state.js';

class ConnectFourViewer {
  constructor() {
    const url = new URL(window.location.href);
    this.matchId = url.searchParams.get('matchId') || url.searchParams.get('match_id');

    this.canvas = document.getElementById('three-canvas');
    this.turnCounter = document.getElementById('turn-counter');
    this.turnIndicator = document.getElementById('turn-indicator');
    this.connBadge = document.getElementById('conn-badge');
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

    this.state = new ConnectFourState(this.matchId);

    try {
      await this.state.loadInitialState();
    } catch (err) {
      console.error('[C4Viewer] Failed to load initial state:', err);
      this._showError('Failed to connect to match');
      return;
    }

    this.renderer = new ConnectFourRenderer(this.canvas);

    this.state.onConnectionChange = (status) => this._setConnectionStatus(status);
    this.state.onMoveMade = () => this._handleMove();
    this.state.onTurnChanged = (playerId, disc) => this._handleTurnChange(playerId, disc);
    this.state.onGameOver = (winner, disc, reason) => this._handleGameOver(winner, disc, reason);

    await this.renderer.charactersLoaded;
    this.renderer.syncBoard(this.state.board);
    if (this.state.phase === 'game_over') {
      this.renderer.setWinHighlight(this.state.board);
    }
    this._updateTurnUi();
    if (this.state.phase === 'game_over') {
      this._handleGameOver(
        this.state.winner,
        this.state.winner !== null ? this.state.discFor(this.state.winner) : null,
        this.state.terminalReason,
      );
    }

    const tick = () => {
      this.renderer.update();
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);

    this.state.connect();
  }

  _handleMove() {
    this.renderer.syncBoard(this.state.board);
    this._updateTurnUi();
    if (this.turnCounter) {
      this.turnCounter.textContent = `Turn ${this.state.turn}`;
    }
  }

  _handleTurnChange(_playerId, disc) {
    if (!this.turnIndicator) return;
    this.turnIndicator.classList.remove('blue-turn', 'orange-turn');
    if (disc === 'blue') {
      this.turnIndicator.classList.add('blue-turn');
      this.turnIndicator.textContent = "Blue's Turn";
    } else if (disc === 'orange') {
      this.turnIndicator.classList.add('orange-turn');
      this.turnIndicator.textContent = "Orange's Turn";
    } else {
      this.turnIndicator.textContent = '';
    }
  }

  _updateTurnUi() {
    if (this.turnCounter) {
      this.turnCounter.textContent = `Turn ${this.state.turn}`;
    }
    if (this.state.phase === 'game_over') {
      if (this.turnIndicator) {
        this.turnIndicator.classList.remove('blue-turn', 'orange-turn');
        this.turnIndicator.textContent = '';
      }
    } else if (this.state.currentPlayer !== null && this.state.currentPlayer !== undefined) {
      const disc = this.state.discFor(this.state.currentPlayer);
      this._handleTurnChange(this.state.currentPlayer, disc);
    }
  }

  _handleGameOver(winner, winnerDisc, reason) {
    if (!this.gameOverEl) return;
    this.gameOverEl.classList.remove('hidden', 'blue-wins', 'orange-wins', 'draw');

    if (winner === null || winner === undefined) {
      this.gameOverEl.classList.add('draw');
      if (this.winnerTitle) this.winnerTitle.textContent = 'DRAW';
      if (this.winnerName) this.winnerName.textContent = '';
    } else {
      const cls = winnerDisc === 'blue' ? 'blue-wins' : 'orange-wins';
      this.gameOverEl.classList.add(cls);
      if (this.winnerTitle) this.winnerTitle.textContent = 'VICTORY';
      if (this.winnerName) {
        this.winnerName.textContent = `${this.state.displayName(winner)}`;
      }
    }
    if (this.winnerReason) {
      this.winnerReason.textContent = reason ? reason.replace(/_/g, ' ') : '';
    }

    this.renderer?.setWinHighlight(this.state.board);
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

const viewer = new ConnectFourViewer();
viewer.init().catch((err) => {
  console.error('[C4Viewer] init failed:', err);
});
