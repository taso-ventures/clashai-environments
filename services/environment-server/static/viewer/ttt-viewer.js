/**
 * Tic-Tac-Toe Spectator Viewer Orchestrator
 *
 * Wires the renderer, state manager, and DOM overlay together. Bootstraps
 * from REST, opens the spectator WebSocket, and reconciles the rendered
 * pieces against engine board state on every action event.
 */

import { TicTacToeRenderer } from './ttt-render.js';
import { TicTacToeState } from './ttt-state.js';

class TicTacToeViewer {
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

    this.state = new TicTacToeState(this.matchId);

    // Bootstrap from REST before constructing the renderer so we can pick
    // initial player colors from match data if needed (for now, defaults).
    try {
      await this.state.loadInitialState();
    } catch (err) {
      console.error('[TTTViewer] Failed to load initial state:', err);
      this._showError('Failed to connect to match');
      return;
    }

    // Construct renderer with default palette (PLAYER_COLORS[0]/[1]).
    this.renderer = new TicTacToeRenderer(this.canvas);

    // Wire callbacks
    this.state.onConnectionChange = (status) => this._setConnectionStatus(status);
    this.state.onMoveMade = (move) => this._handleMove(move);
    this.state.onTurnChanged = (playerId, mark) => this._handleTurnChange(playerId, mark);
    this.state.onGameOver = (winner, mark, reason) => this._handleGameOver(winner, mark, reason);

    // Render the initial board snapshot — wait for characters first so the
    // first painted frame is fully populated.
    await this.renderer.charactersLoaded;
    this.renderer.syncBoard(this.state.board);
    if (this.state.phase === 'game_over') {
      this.renderer.setWinningLine(this.state.board);
    }
    this._updateTurnUi();
    if (this.state.phase === 'game_over') {
      this._handleGameOver(
        this.state.winner,
        this.state.winner !== null ? this.state.markFor(this.state.winner) : null,
        this.state.terminalReason,
      );
    }

    // Render loop
    const tick = () => {
      this.renderer.update();
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);

    // Connect WS for live updates
    this.state.connect();
  }

  _handleMove(_move) {
    // Reconcile the whole board (idempotent) — handles both the just-applied
    // move and any catchup-time backfill.
    this.renderer.syncBoard(this.state.board);
    this._updateTurnUi();
    if (this.turnCounter) {
      this.turnCounter.textContent = `Turn ${this.state.turn}`;
    }
  }

  _handleTurnChange(_playerId, mark) {
    if (!this.turnIndicator) return;
    this.turnIndicator.classList.remove('x-turn', 'o-turn');
    if (mark === 'x') {
      this.turnIndicator.classList.add('x-turn');
      this.turnIndicator.textContent = "X's Turn";
    } else if (mark === 'o') {
      this.turnIndicator.classList.add('o-turn');
      this.turnIndicator.textContent = "O's Turn";
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
        this.turnIndicator.classList.remove('x-turn', 'o-turn');
        this.turnIndicator.textContent = '';
      }
    } else if (this.state.currentPlayer !== null && this.state.currentPlayer !== undefined) {
      const mark = this.state.markFor(this.state.currentPlayer);
      this._handleTurnChange(this.state.currentPlayer, mark);
    }
  }

  _handleGameOver(winner, winnerMark, reason) {
    if (!this.gameOverEl) return;
    this.gameOverEl.classList.remove('hidden', 'x-wins', 'o-wins', 'draw');

    if (winner === null || winner === undefined) {
      this.gameOverEl.classList.add('draw');
      if (this.winnerTitle) this.winnerTitle.textContent = 'DRAW';
      if (this.winnerName) this.winnerName.textContent = '';
    } else {
      const cls = winnerMark === 'x' ? 'x-wins' : 'o-wins';
      this.gameOverEl.classList.add(cls);
      if (this.winnerTitle) this.winnerTitle.textContent = 'VICTORY';
      if (this.winnerName) {
        this.winnerName.textContent = `${this.state.displayName(winner)} (${winnerMark?.toUpperCase() ?? ''})`;
      }
    }
    if (this.winnerReason) {
      this.winnerReason.textContent = reason ? reason.replace(/_/g, ' ') : '';
    }

    this.renderer?.setWinningLine(this.state.board);
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

const viewer = new TicTacToeViewer();
viewer.init().catch((err) => {
  console.error('[TTTViewer] init failed:', err);
});
