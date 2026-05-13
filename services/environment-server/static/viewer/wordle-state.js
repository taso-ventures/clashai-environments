/**
 * Wordle State Manager
 *
 * Bootstraps from REST GET /matches/:id/state, then re-fetches on every
 * UnifiedEvent action broadcast over the spectator WebSocket. Diff-driven
 * pattern: emits onGuessAdded when a player's guesses array grows and
 * onChatMessage when chat_messages grows.
 */

const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 15000;
export const WORD_LENGTH = 5;
export const MAX_GUESSES = 6;

export class WordleState {
  constructor(matchId) {
    this.matchId = matchId;

    // Mirrors crates/wordle-protocol::WordleFullState.
    this.turn = 0;
    this.phase = 'lobby';
    this.players = []; // [{ player_id, display_name, target_word, guesses[], solved, eliminated, solved_turn }]
    this.chatMessages = []; // [{ player_id, player_name, text, turn, timestamp_ms, phase }]
    this.isTerminal = false;
    this.terminalReason = null;
    this.solveOrder = [];
    this.playerNames = {};

    // Per-player guess count for diff-driven dispatch
    this._lastGuessCount = new Map();
    this._lastChatCount = 0;

    this.ws = null;
    this.reconnectAttempt = 0;
    this.isCatchingUp = false;
    this._closing = false;
    this._inFlight = false;
    this._pending = false;

    // Callback hooks — set by the viewer
    this.onGuessAdded = null; // ({ playerId, guess, totalGuesses })
    this.onChatMessage = null; // (message)
    this.onPlayerProgress = null; // (player) — solved / eliminated state change
    this.onPhaseChange = null; // (phase)
    this.onGameOver = null; // (solveOrder, terminalReason, players)
    this.onConnectionChange = null;
  }

  // ─── REST Bootstrap ───

  async loadInitialState() {
    const baseUrl = window.location.origin;
    const response = await fetch(`${baseUrl}/matches/${this.matchId}/state`);
    if (!response.ok) {
      throw new Error(`Failed to load state: ${response.status}`);
    }
    // Server wraps state in { "state": <env JSON> } per PROTOCOL.md.
    const body = await response.json();
    this._applyFullState(body.state, { suppressEvents: true });
    await this._loadPlayerNames();
    return body.state;
  }

  async _loadPlayerNames() {
    try {
      const baseUrl = window.location.origin;
      const response = await fetch(`${baseUrl}/matches/${this.matchId}/player_names`);
      if (response.ok) {
        const names = await response.json();
        this.playerNames = names.player_names ?? names ?? {};
      }
    } catch (_e) { /* best-effort */ }
  }

  _applyFullState(state, { suppressEvents = false } = {}) {
    const prevPhase = this.phase;
    this.turn = state.turn ?? 0;
    this.phase = state.phase ?? 'lobby';
    this.players = state.players ?? [];
    this.chatMessages = state.chat_messages ?? [];
    this.isTerminal = state.is_terminal ?? false;
    this.terminalReason = state.terminal_reason ?? null;
    this.solveOrder = state.solve_order ?? [];

    if (suppressEvents) {
      // Seed the per-player guess counts so we don't re-emit on the first refetch.
      for (const p of this.players) {
        this._lastGuessCount.set(p.player_id, p.guesses?.length ?? 0);
      }
      this._lastChatCount = this.chatMessages.length;
      return;
    }

    // Diff guesses
    for (const p of this.players) {
      const prev = this._lastGuessCount.get(p.player_id) ?? 0;
      const cur = p.guesses?.length ?? 0;
      if (cur > prev && this.onGuessAdded) {
        for (let i = prev; i < cur; i += 1) {
          this.onGuessAdded({
            playerId: p.player_id,
            guess: p.guesses[i],
            totalGuesses: cur,
          });
        }
      }
      this._lastGuessCount.set(p.player_id, cur);

      if (this.onPlayerProgress) this.onPlayerProgress(p);
    }

    // Diff chat
    if (this.chatMessages.length > this._lastChatCount) {
      const newMessages = this.chatMessages.slice(this._lastChatCount);
      this._lastChatCount = this.chatMessages.length;
      if (this.onChatMessage) {
        for (const msg of newMessages) this.onChatMessage(msg);
      }
    }

    if (this.phase !== prevPhase && this.onPhaseChange) {
      this.onPhaseChange(this.phase);
    }
  }

  displayName(playerId) {
    if (playerId === null || playerId === undefined) return null;
    const fromMap = this.playerNames?.[String(playerId)];
    if (fromMap) return fromMap;
    const fromState = this.players?.find((p) => p.player_id === playerId);
    if (fromState?.display_name) return fromState.display_name;
    return `Player ${playerId}`;
  }

  // ─── WebSocket ───

  connect() {
    if (this._closing) return;
    this._notifyConnection('connecting');
    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const url = `${proto}//${window.location.host}/matches/${this.matchId}/spectator/ws`;
    this.ws = new WebSocket(url);

    this.ws.onopen = () => {
      this.reconnectAttempt = 0;
      this._notifyConnection(this.isTerminal ? 'terminal' : 'running');
    };

    this.ws.onmessage = async (ev) => {
      let frame;
      try { frame = JSON.parse(ev.data); } catch { return; }
      if (frame.catchup_start) { this.isCatchingUp = true; return; }
      if (frame.catchup_end) {
        this.isCatchingUp = false;
        await this._refetchAndDispatch();
        return;
      }
      const t = frame.event_type;
      if (t === 'action' || t === 'terminal') {
        await this._refetchAndDispatch();
      }
    };

    this.ws.onclose = () => {
      if (this._closing) return;
      this._notifyConnection('reconnecting');
      this._scheduleReconnect();
    };

    this.ws.onerror = () => { /* onclose handles reconnect */ };
  }

  async _refetchAndDispatch() {
    if (this._inFlight) { this._pending = true; return; }
    this._inFlight = true;
    try {
      do {
        this._pending = false;
        await this._doRefetchAndDispatch();
      } while (this._pending);
    } finally {
      this._inFlight = false;
    }
  }

  async _doRefetchAndDispatch() {
    let body;
    try {
      const baseUrl = window.location.origin;
      const response = await fetch(`${baseUrl}/matches/${this.matchId}/state`);
      if (!response.ok) return;
      body = await response.json();
    } catch { return; }
    this._applyFullState(body.state);

    if (this.isTerminal && this.onGameOver) {
      this.onGameOver(this.solveOrder, this.terminalReason, this.players);
      this._notifyConnection('terminal');
    }
  }

  _scheduleReconnect() {
    const delay = Math.min(
      RECONNECT_BASE_MS * Math.pow(2, this.reconnectAttempt),
      RECONNECT_MAX_MS,
    );
    this.reconnectAttempt += 1;
    setTimeout(() => { if (!this._closing) this.connect(); }, delay);
  }

  _notifyConnection(status) {
    if (this.onConnectionChange) this.onConnectionChange(status);
  }

  close() {
    this._closing = true;
    if (this.ws) {
      try { this.ws.close(); } catch (_) { /* noop */ }
      this.ws = null;
    }
  }
}
