/**
 * Wordle Spectator Viewer Orchestrator
 *
 * Pure DOM viewer — no Three.js. Renders one agent grid card per player
 * (6x5 tiles with feedback colors) plus a chat feed below. Mirrors the
 * internal WordleAgentGrids + WordleChatPanel layout.
 */

import { WordleState, WORD_LENGTH, MAX_GUESSES } from './wordle-state.js';
import { PLAYER_COLORS } from './shared/shared-colors.js';

function hexToCssColor(hex) {
  return `#${hex.toString(16).padStart(6, '0')}`;
}

class WordleViewer {
  constructor() {
    const url = new URL(window.location.href);
    this.matchId = url.searchParams.get('matchId') || url.searchParams.get('match_id');

    this.turnCounter = document.getElementById('turn-counter');
    this.phaseIndicator = document.getElementById('phase-indicator');
    this.connBadge = document.getElementById('conn-badge');
    this.gridsEl = document.getElementById('agent-grids');
    this.gridsEmpty = document.getElementById('agent-grids-empty');
    this.chatFeed = document.getElementById('chat-feed');
    this.gameOverEl = document.getElementById('game-over-overlay');
    this.winnerTitle = document.getElementById('winner-title');
    this.winnerName = document.getElementById('winner-name');
    this.winnerReason = document.getElementById('winner-reason');
    this.connStatus = document.getElementById('connection-status');

    this.state = null;
    this.cards = new Map(); // playerId -> { card, rows[][], statusEl }
    this.colorByPlayer = new Map();
    this._chatRendered = 0;
  }

  async init() {
    if (!this.matchId) {
      this._showError('Missing matchId in URL.');
      return;
    }
    this.state = new WordleState(this.matchId);

    try {
      await this.state.loadInitialState();
    } catch (err) {
      console.error('[WordleViewer] Failed to load initial state:', err);
      this._showError('Failed to connect to match');
      return;
    }

    this.state.onConnectionChange = (status) => this._setConnectionStatus(status);
    this.state.onGuessAdded = ({ playerId, guess, totalGuesses }) =>
      this._handleGuessAdded(playerId, guess, totalGuesses);
    this.state.onChatMessage = (msg) => this._renderChatMessage(msg);
    this.state.onPlayerProgress = (player) => this._updatePlayerStatus(player);
    this.state.onPhaseChange = () => this._updatePhaseIndicator();
    this.state.onGameOver = (solveOrder, reason, players) =>
      this._handleGameOver(solveOrder, reason, players);

    // Render initial snapshot
    this._renderPlayers();
    this._renderInitialGuesses();
    this._renderInitialChat();
    this._updateHeader();

    if (this.state.isTerminal) {
      this._handleGameOver(
        this.state.solveOrder,
        this.state.terminalReason,
        this.state.players,
      );
    }

    this.state.connect();
  }

  // ─── Player card rendering ───

  _renderPlayers() {
    if (!this.gridsEl) return;
    if (!this.state.players.length) return;

    // Hide empty state
    if (this.gridsEmpty) this.gridsEmpty.style.display = 'none';

    for (let i = 0; i < this.state.players.length; i += 1) {
      const p = this.state.players[i];
      if (this.cards.has(p.player_id)) continue;

      const color = hexToCssColor(PLAYER_COLORS[i % PLAYER_COLORS.length]);
      this.colorByPlayer.set(p.player_id, color);

      const card = document.createElement('div');
      card.className = 'agent-card';
      card.dataset.playerId = String(p.player_id);

      // Header
      const header = document.createElement('div');
      header.className = 'agent-header';

      const swatch = document.createElement('div');
      swatch.className = 'agent-swatch';
      swatch.style.background = color;
      swatch.style.color = color;
      header.appendChild(swatch);

      const meta = document.createElement('div');
      meta.className = 'agent-meta';

      const nameEl = document.createElement('div');
      nameEl.className = 'agent-name';
      nameEl.style.color = color;
      nameEl.textContent = this.state.displayName(p.player_id);
      meta.appendChild(nameEl);

      const statusEl = document.createElement('div');
      statusEl.className = 'agent-status';
      meta.appendChild(statusEl);

      header.appendChild(meta);
      card.appendChild(header);

      // Tile grid (MAX_GUESSES rows × WORD_LENGTH tiles)
      const grid = document.createElement('div');
      grid.className = 'grid-rows';
      const rows = [];
      for (let r = 0; r < MAX_GUESSES; r += 1) {
        const row = document.createElement('div');
        row.className = 'grid-row';
        const tiles = [];
        for (let c = 0; c < WORD_LENGTH; c += 1) {
          const tile = document.createElement('div');
          tile.className = 'tile';
          row.appendChild(tile);
          tiles.push(tile);
        }
        grid.appendChild(row);
        rows.push(tiles);
      }
      card.appendChild(grid);

      this.gridsEl.appendChild(card);
      this.cards.set(p.player_id, { card, rows, statusEl });
      this._updatePlayerStatus(p);
    }
  }

  _renderInitialGuesses() {
    for (const p of this.state.players) {
      const entry = this.cards.get(p.player_id);
      if (!entry) continue;
      for (let i = 0; i < (p.guesses?.length ?? 0); i += 1) {
        this._paintGuess(entry, p.guesses[i], { animate: false });
      }
    }
  }

  _handleGuessAdded(playerId, guess, totalGuesses) {
    const entry = this.cards.get(playerId);
    if (!entry) return;
    this._paintGuess(entry, guess, { animate: true });
    this._updateHeader();
  }

  _paintGuess(entry, guess, { animate }) {
    const rowIdx = guess.turn != null ? guess.turn - 1 : 0;
    // The turn field counts from 1 in the engine; fall back to scanning for
    // the first un-filled row if it's missing/out of range.
    let row = entry.rows[rowIdx];
    if (!row) {
      row = entry.rows.find((r) => !r[0]?.classList.contains('correct')
        && !r[0]?.classList.contains('present')
        && !r[0]?.classList.contains('absent'));
    }
    if (!row) return;
    const word = (guess.word ?? '').toUpperCase();
    const feedback = guess.feedback ?? [];
    for (let i = 0; i < WORD_LENGTH; i += 1) {
      const tile = row[i];
      if (!tile) continue;
      tile.textContent = word[i] ?? '';
      tile.classList.remove('correct', 'present', 'absent', 'has-letter', 'flip-in');
      tile.classList.add('has-letter');
      const fb = feedback[i];
      if (fb) tile.classList.add(fb);
      if (animate) {
        // stagger by column so the flip cascades L→R
        setTimeout(() => tile.classList.add('flip-in'), i * 100);
      }
    }
  }

  _updatePlayerStatus(player) {
    const entry = this.cards.get(player.player_id);
    if (!entry) return;
    entry.statusEl.classList.remove('solved', 'failed');
    const made = player.guesses?.length ?? 0;
    if (player.solved) {
      entry.statusEl.classList.add('solved');
      entry.statusEl.textContent = `Solved ${made}/${MAX_GUESSES}`;
    } else if (player.eliminated) {
      entry.statusEl.classList.add('failed');
      entry.statusEl.textContent = made > 0 ? `Failed X/${MAX_GUESSES}` : 'DNF';
    } else {
      entry.statusEl.textContent = `Guessing... ${made}/${MAX_GUESSES}`;
    }
  }

  // ─── Chat rendering ───

  _renderInitialChat() {
    if (!this.chatFeed) return;
    for (const msg of this.state.chatMessages) this._renderChatMessage(msg, { initial: true });
  }

  _renderChatMessage(msg) {
    if (!this.chatFeed) return;
    // Clear empty-state if present
    const empty = this.chatFeed.querySelector('.empty-state');
    if (empty) empty.remove();

    const wrap = document.createElement('div');
    wrap.className = 'chat-msg';

    const meta = document.createElement('div');
    meta.className = 'chat-meta';
    const author = document.createElement('span');
    author.className = 'author';
    author.textContent = msg.player_name || this.state.displayName(msg.player_id) || '';
    const color = this.colorByPlayer.get(msg.player_id);
    if (color) author.style.color = color;
    meta.appendChild(author);
    if (msg.phase) {
      const phase = document.createElement('span');
      phase.className = 'phase';
      phase.textContent = msg.phase;
      meta.appendChild(phase);
    }
    wrap.appendChild(meta);

    const bubble = document.createElement('div');
    bubble.className = 'chat-bubble';
    if (msg.phase === 'win') bubble.classList.add('phase-win');
    else if (msg.phase === 'banter') bubble.classList.add('phase-banter');
    bubble.textContent = msg.text ?? '';
    wrap.appendChild(bubble);

    this.chatFeed.appendChild(wrap);
    this.chatFeed.scrollTop = this.chatFeed.scrollHeight;
  }

  // ─── Header / phase indicator ───

  _updateHeader() {
    if (this.turnCounter) this.turnCounter.textContent = `Turn ${this.state.turn}`;
    this._updatePhaseIndicator();
  }

  _updatePhaseIndicator() {
    if (!this.phaseIndicator) return;
    const phase = this.state.phase;
    if (!phase || phase === 'game_over') {
      this.phaseIndicator.textContent = '';
    } else {
      this.phaseIndicator.textContent = phase.toUpperCase();
    }
  }

  _handleGameOver(solveOrder, reason, players) {
    if (!this.gameOverEl) return;
    this.gameOverEl.classList.remove('hidden');
    if (this.winnerTitle) this.winnerTitle.textContent = 'RESULTS';

    // Build a leaderboard summary: solvers first in solve_order, then unsolved.
    const solverNames = (solveOrder ?? [])
      .map((pid, idx) => `${idx + 1}. ${this.state.displayName(pid)}`)
      .join(' · ');
    if (this.winnerName) {
      this.winnerName.textContent = solverNames || 'No solver';
    }
    if (this.winnerReason) {
      const reasonText = reason ? reason.replace(/_/g, ' ') : '';
      this.winnerReason.textContent = reasonText;
    }
    this._updatePhaseIndicator();
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

const viewer = new WordleViewer();
viewer.init().catch((err) => {
  console.error('[WordleViewer] init failed:', err);
});
