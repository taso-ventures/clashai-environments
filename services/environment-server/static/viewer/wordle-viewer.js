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
    this.spoilerToggle = document.getElementById('spoiler-toggle');
    this.tabsEl = document.getElementById('tabs');
    this.agentGridsEl = document.getElementById('agent-grids');
    this.userPlayEl = document.getElementById('user-play');
    this.userGridEl = document.getElementById('user-grid');
    this.userStatusEl = document.getElementById('user-status');
    this.userKeyboardEl = document.getElementById('user-keyboard');

    this.state = null;
    this.cards = new Map(); // playerId -> { card, rows[][], statusEl }
    this.colorByPlayer = new Map();
    this._chatRendered = 0;
    // Serial animation queue across players — a guess from player N waits
    // for player N-1's row cascade to complete before starting.
    this._animEndTime = 0;
    // Per-player end time so each player's status line updates when *their*
    // row finishes, not when the whole queue drains.
    this._playerAnimEnd = new Map();
    // Spoiler-mode: hide letter content on agent tiles while preserving
    // color feedback. Preference persists across page reloads.
    this._spoilerHidden = localStorage.getItem('wordle.spoilerHidden') === '1';

    // User-play state (the "Your Play" tab): spectator races the same
    // target the agents are guessing. Stateless on the server; localStorage
    // restores progress across reloads.
    this._activeTab = 'ai_replay';
    this._userRows = []; // 6 rows × 5 tiles refs
    this._userGuesses = []; // [{ word, feedback, solved }]
    this._userCurrentLetters = []; // letters in the in-progress row
    this._userTerminal = false;
    this._userSubmitting = false;
  }

  /** Run `fn` after the global animation queue has drained. */
  _afterGlobalAnim(fn) {
    const delay = Math.max(0, this._animEndTime - Date.now());
    if (delay > 0) setTimeout(fn, delay);
    else fn();
  }

  /** Run `fn` after this player's most recent row cascade has finished. */
  _afterPlayerAnim(playerId, fn) {
    const end = this._playerAnimEnd.get(playerId) ?? 0;
    const delay = Math.max(0, end - Date.now());
    if (delay > 0) setTimeout(fn, delay);
    else fn();
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

    if (this.spoilerToggle) {
      this.spoilerToggle.setAttribute('aria-pressed', String(this._spoilerHidden));
      this._updateSpoilerToggleTitle();
      this.spoilerToggle.addEventListener('click', () => this._toggleSpoilers());
    }

    this._initTabs();
    this._initUserPlay();

    // Render initial snapshot
    this._renderPlayers();
    this._renderInitialGuesses();
    this._renderInitialChat();
    this._renderPhaseIndicator();

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

      // Header — agent name in their assigned color + small status line below.
      // No swatch circle — matches the internal Wordle layout where the
      // model logo serves as the visual; we don't ship brand assets in OSS,
      // so the colored name carries the identity.
      const header = document.createElement('div');
      header.className = 'agent-header';

      const nameEl = document.createElement('div');
      nameEl.className = 'agent-name';
      nameEl.style.color = color;
      nameEl.textContent = this.state.displayName(p.player_id);
      header.appendChild(nameEl);

      const statusEl = document.createElement('div');
      statusEl.className = 'agent-status';
      header.appendChild(statusEl);

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
      this._renderPlayerStatus(p);
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

  _handleGuessAdded(playerId, guess, _totalGuesses) {
    const entry = this.cards.get(playerId);
    if (!entry) return;
    this._paintGuess(entry, guess, { animate: true, playerId });
  }

  _paintGuess(entry, guess, { animate, playerId }) {
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

    // Each tile spins to 90° edge-on; at the midpoint we reveal the letter
    // AND the color together — tile is invisible at that instant, so the
    // letter pops in with the flip rather than appearing pre-flip.
    const FLIP_MS = 600;
    const STAGGER_MS = 280;
    const ROW_DURATION_MS = FLIP_MS + (WORD_LENGTH - 1) * STAGGER_MS;
    const INTER_PLAYER_GAP_MS = 300;

    // Reset tile contents now (empty until each tile's flip reveals it).
    for (let i = 0; i < WORD_LENGTH; i += 1) {
      const tile = row[i];
      if (!tile) continue;
      tile.classList.remove('correct', 'present', 'absent', 'has-letter', 'flip-in');
      tile.textContent = '';
    }

    if (!animate) {
      for (let i = 0; i < WORD_LENGTH; i += 1) {
        const tile = row[i];
        if (!tile) continue;
        const letter = word[i] ?? '';
        tile.dataset.letter = letter;
        tile.textContent = this._spoilerHidden ? '' : letter;
        tile.classList.add('has-letter');
        const fb = feedback[i];
        if (fb) tile.classList.add(fb);
      }
      return;
    }

    // Serial across players: this row starts after the previously queued
    // row has fully completed (plus a small gap for legibility).
    const now = Date.now();
    const startAt = Math.max(now, this._animEndTime);
    const baseOffset = startAt - now;

    for (let i = 0; i < WORD_LENGTH; i += 1) {
      const tile = row[i];
      if (!tile) continue;
      const flipStart = baseOffset + i * STAGGER_MS;
      const revealAt = flipStart + FLIP_MS / 2;
      const fb = feedback[i];
      const letter = word[i] ?? '';
      setTimeout(() => tile.classList.add('flip-in'), flipStart);
      setTimeout(() => {
        tile.dataset.letter = letter;
        tile.textContent = this._spoilerHidden ? '' : letter;
        tile.classList.add('has-letter');
        if (fb) tile.classList.add(fb);
      }, revealAt);
    }

    const rowEndTime = startAt + ROW_DURATION_MS;
    if (playerId != null) {
      this._playerAnimEnd.set(playerId, rowEndTime);
    }
    this._animEndTime = rowEndTime + INTER_PLAYER_GAP_MS;
  }

  /** Defers the DOM update until that player's pending row cascade completes. */
  _updatePlayerStatus(player) {
    this._afterPlayerAnim(player.player_id, () => this._renderPlayerStatus(player));
  }

  _renderPlayerStatus(player) {
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

    // iMessage-style: author name above bubble in agent color, no inline
    // phase tag (the React reference shows phase grouping via UI sectioning,
    // not per-message badges).
    const meta = document.createElement('div');
    meta.className = 'chat-meta';
    const author = document.createElement('span');
    author.className = 'author';
    author.textContent = msg.player_name || this.state.displayName(msg.player_id) || '';
    const color = this.colorByPlayer.get(msg.player_id);
    if (color) author.style.color = color;
    meta.appendChild(author);
    wrap.appendChild(meta);

    const bubble = document.createElement('div');
    bubble.className = 'chat-bubble';
    bubble.textContent = msg.text ?? '';
    wrap.appendChild(bubble);

    this.chatFeed.appendChild(wrap);
    this.chatFeed.scrollTop = this.chatFeed.scrollHeight;
  }

  // ─── Phase indicator ───

  /** Defers phase-indicator changes until the animation queue drains. */
  _updatePhaseIndicator() {
    this._afterGlobalAnim(() => this._renderPhaseIndicator());
  }

  _renderPhaseIndicator() {
    if (!this.phaseIndicator) return;
    const phase = this.state.phase;
    if (!phase || phase === 'game_over') {
      this.phaseIndicator.textContent = '';
    } else {
      this.phaseIndicator.textContent = phase.toUpperCase();
    }
  }

  // ─── Spoiler toggle ───

  _toggleSpoilers() {
    this._spoilerHidden = !this._spoilerHidden;
    localStorage.setItem('wordle.spoilerHidden', this._spoilerHidden ? '1' : '0');
    if (this.spoilerToggle) {
      this.spoilerToggle.setAttribute('aria-pressed', String(this._spoilerHidden));
      this._updateSpoilerToggleTitle();
    }
    // Apply to already-painted tiles. Pending animation timeouts read
    // this._spoilerHidden when they fire, so newly-revealed tiles will
    // pick up the new state without extra plumbing.
    for (const { rows } of this.cards.values()) {
      for (const row of rows) {
        for (const tile of row) {
          const letter = tile.dataset.letter;
          if (letter) tile.textContent = this._spoilerHidden ? '' : letter;
        }
      }
    }
  }

  _updateSpoilerToggleTitle() {
    if (!this.spoilerToggle) return;
    this.spoilerToggle.title = this._spoilerHidden
      ? 'Show guessed letters'
      : 'Hide guessed letters';
  }

  _handleGameOver(solveOrder, reason, players) {
    // Defer until any in-flight tile-flip animations have completed so the
    // overlay doesn't race the cascade.
    const delay = Math.max(0, this._animEndTime - Date.now());
    if (delay > 0) {
      setTimeout(() => this._renderGameOver(solveOrder, reason, players), delay);
    } else {
      this._renderGameOver(solveOrder, reason, players);
    }
  }

  _renderGameOver(solveOrder, reason, _players) {
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
    this._renderPhaseIndicator();
    this._installGameOverDismiss();
  }

  /** Hide the results overlay on the first user interaction so the
   *  rest of the page (tabs, keyboard, spoiler toggle, chat) is no
   *  longer visually obscured. Re-appears only on page reload. */
  _installGameOverDismiss() {
    if (this._gameOverDismissArmed) return;
    this._gameOverDismissArmed = true;
    const dismiss = () => {
      if (this.gameOverEl) this.gameOverEl.classList.add('hidden');
      document.removeEventListener('pointerdown', dismiss, true);
      document.removeEventListener('keydown', dismiss, true);
    };
    // Capture-phase so the listener fires before any handler that might
    // stopPropagation (e.g. on the tab buttons or virtual keyboard keys).
    document.addEventListener('pointerdown', dismiss, true);
    document.addEventListener('keydown', dismiss, true);
  }

  // ─── Tabs ───

  _initTabs() {
    if (!this.tabsEl) return;
    for (const btn of this.tabsEl.querySelectorAll('button[data-tab]')) {
      btn.addEventListener('click', () => this._switchTab(btn.dataset.tab));
    }
  }

  _switchTab(tab) {
    if (tab !== 'ai_replay' && tab !== 'your_play') return;
    this._activeTab = tab;
    document.body.classList.toggle('tab-your-play', tab === 'your_play');
    if (this.agentGridsEl) this.agentGridsEl.hidden = tab !== 'ai_replay';
    if (this.userPlayEl) this.userPlayEl.hidden = tab !== 'your_play';
    for (const btn of this.tabsEl.querySelectorAll('button[data-tab]')) {
      btn.setAttribute('aria-selected', String(btn.dataset.tab === tab));
    }
  }

  // ─── Play-along (the "Your Play" tab) ───

  _initUserPlay() {
    this._buildUserGrid();
    this._buildUserKeyboard();
    this._loadUserGuessesFromStorage();
    document.addEventListener('keydown', (ev) => this._onPhysicalKey(ev));
  }

  _buildUserGrid() {
    if (!this.userGridEl) return;
    this.userGridEl.innerHTML = '';
    for (let r = 0; r < MAX_GUESSES; r += 1) {
      const row = document.createElement('div');
      row.className = 'grid-row';
      const tiles = [];
      for (let c = 0; c < WORD_LENGTH; c += 1) {
        const tile = document.createElement('div');
        tile.className = 'tile user-input';
        row.appendChild(tile);
        tiles.push(tile);
      }
      this.userGridEl.appendChild(row);
      this._userRows.push(tiles);
    }
  }

  _buildUserKeyboard() {
    if (!this.userKeyboardEl) return;
    const layout = [
      ['Q', 'W', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P'],
      ['A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L'],
      ['ENTER', 'Z', 'X', 'C', 'V', 'B', 'N', 'M', 'BACK'],
    ];
    for (const rowKeys of layout) {
      const rowEl = document.createElement('div');
      rowEl.className = 'kb-row';
      for (const k of rowKeys) {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'kb-key';
        btn.dataset.key = k;
        if (k === 'ENTER' || k === 'BACK') btn.classList.add('kb-wide');
        btn.textContent = k === 'BACK' ? '⌫' : k;
        btn.addEventListener('click', () => this._handleKey(k));
        rowEl.appendChild(btn);
      }
      this.userKeyboardEl.appendChild(rowEl);
    }
  }

  _onPhysicalKey(ev) {
    if (this._activeTab !== 'your_play') return;
    if (ev.metaKey || ev.ctrlKey || ev.altKey) return;
    if (ev.key === 'Enter') {
      ev.preventDefault();
      this._handleKey('ENTER');
    } else if (ev.key === 'Backspace') {
      ev.preventDefault();
      this._handleKey('BACK');
    } else if (/^[a-zA-Z]$/.test(ev.key)) {
      ev.preventDefault();
      this._handleKey(ev.key.toUpperCase());
    }
  }

  _handleKey(key) {
    if (this._userTerminal || this._userSubmitting) return;
    if (key === 'ENTER') {
      this._submitUserGuess();
      return;
    }
    if (key === 'BACK') {
      if (this._userCurrentLetters.length === 0) return;
      this._userCurrentLetters.pop();
      this._renderUserCurrentRow();
      return;
    }
    if (this._userCurrentLetters.length >= WORD_LENGTH) return;
    this._userCurrentLetters.push(key);
    this._renderUserCurrentRow();
  }

  _renderUserCurrentRow() {
    const rowIdx = this._userGuesses.length;
    const row = this._userRows[rowIdx];
    if (!row) return;
    for (let c = 0; c < WORD_LENGTH; c += 1) {
      const tile = row[c];
      const letter = this._userCurrentLetters[c] ?? '';
      tile.textContent = letter;
      tile.classList.toggle('has-letter', letter !== '');
      tile.classList.toggle('active-cell', c === this._userCurrentLetters.length);
    }
  }

  async _submitUserGuess() {
    if (this._userCurrentLetters.length !== WORD_LENGTH) {
      this._setUserStatus('Need 5 letters', 'error');
      return;
    }
    const guess = this._userCurrentLetters.join('').toLowerCase();
    this._userSubmitting = true;
    this._setUserStatus('');
    try {
      const resp = await fetch(`/matches/${this.matchId}/play-along/guess`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ guess }),
      });
      if (resp.status === 429) {
        this._setUserStatus('Out of attempts for this match', 'error');
        this._userTerminal = true;
        return;
      }
      if (!resp.ok) {
        const body = await resp.json().catch(() => ({}));
        this._setUserStatus(body.error || 'Invalid guess', 'error');
        return;
      }
      const { feedback, solved } = await resp.json();
      this._userGuesses.push({ word: guess, feedback, solved });
      this._userCurrentLetters = [];
      this._persistUserGuesses();
      this._paintUserRow(this._userGuesses.length - 1, guess, feedback);
      if (solved) {
        this._userTerminal = true;
        this._setUserStatus(`Solved in ${this._userGuesses.length}/${MAX_GUESSES}`, 'solved');
      } else if (this._userGuesses.length >= MAX_GUESSES) {
        this._userTerminal = true;
        this._setUserStatus('Out of guesses', 'error');
      } else {
        this._setUserStatus('');
      }
    } catch (err) {
      this._setUserStatus('Network error — try again', 'error');
    } finally {
      this._userSubmitting = false;
    }
  }

  _paintUserRow(rowIdx, word, feedback) {
    const row = this._userRows[rowIdx];
    if (!row) return;
    // Reuse the cascade timing from agent grids so user reveals feel
    // consistent. No serialization across players — single grid.
    const FLIP_MS = 600;
    const STAGGER_MS = 280;
    const upper = word.toUpperCase();
    for (let i = 0; i < WORD_LENGTH; i += 1) {
      const tile = row[i];
      if (!tile) continue;
      const flipStart = i * STAGGER_MS;
      const revealAt = flipStart + FLIP_MS / 2;
      const letter = upper[i] ?? '';
      const fb = feedback[i];
      tile.classList.remove('active-cell');
      tile.classList.remove('correct', 'present', 'absent');
      tile.textContent = '';
      setTimeout(() => tile.classList.add('flip-in'), flipStart);
      setTimeout(() => {
        tile.textContent = letter;
        tile.classList.add('has-letter');
        if (fb) tile.classList.add(fb);
      }, revealAt);
    }
  }

  _setUserStatus(msg, kind) {
    if (!this.userStatusEl) return;
    this.userStatusEl.classList.remove('error', 'solved');
    if (kind === 'error') this.userStatusEl.classList.add('error');
    if (kind === 'solved') this.userStatusEl.classList.add('solved');
    this.userStatusEl.textContent = msg || '';
  }

  _persistUserGuesses() {
    try {
      localStorage.setItem(
        this._userStorageKey(),
        JSON.stringify({ guesses: this._userGuesses }),
      );
    } catch (_e) {
      // localStorage may be unavailable (private mode); swallow.
    }
  }

  _loadUserGuessesFromStorage() {
    let saved = null;
    try {
      saved = localStorage.getItem(this._userStorageKey());
    } catch (_e) {
      return;
    }
    if (!saved) return;
    try {
      const parsed = JSON.parse(saved);
      const guesses = Array.isArray(parsed?.guesses) ? parsed.guesses : [];
      for (let i = 0; i < guesses.length; i += 1) {
        const g = guesses[i];
        if (!g?.word || !Array.isArray(g.feedback)) continue;
        this._userGuesses.push(g);
        // Paint synchronously; no cascade for restored rows.
        const row = this._userRows[i];
        if (!row) continue;
        const upper = g.word.toUpperCase();
        for (let c = 0; c < WORD_LENGTH; c += 1) {
          const tile = row[c];
          tile.textContent = upper[c] ?? '';
          tile.classList.add('has-letter');
          if (g.feedback[c]) tile.classList.add(g.feedback[c]);
        }
        if (g.solved) {
          this._userTerminal = true;
          this._setUserStatus(`Solved in ${i + 1}/${MAX_GUESSES}`, 'solved');
        }
      }
      if (!this._userTerminal && this._userGuesses.length >= MAX_GUESSES) {
        this._userTerminal = true;
        this._setUserStatus('Out of guesses', 'error');
      }
    } catch (_e) {
      // Corrupt storage — ignore and start fresh.
    }
  }

  _userStorageKey() {
    return `wordle.userPlay.${this.matchId}`;
  }

  _setConnectionStatus(status) {
    // The 'terminal' transition signals the match is over; defer the badge
    // swap until the queued tile cascades have drained so it doesn't precede
    // the final flip.
    if (status === 'terminal') {
      this._afterGlobalAnim(() => this._renderConnectionStatus('terminal'));
    } else {
      this._renderConnectionStatus(status);
    }
  }

  _renderConnectionStatus(status) {
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
