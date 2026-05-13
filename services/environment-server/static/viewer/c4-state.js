/**
 * Connect Four State Manager
 *
 * Bootstraps from REST GET /matches/:id/state, then re-fetches on every
 * UnifiedEvent action broadcast over the spectator WebSocket. The
 * connect_four environment adapter returns events: [] from apply_action,
 * so the viewer derives deltas by diffing the freshly-fetched full state
 * against the previously-applied snapshot.
 *
 * Mirrors ttt-state.js — different state shape (6x7 grid, blue/orange
 * disc identifiers in the wire format) but identical bootstrap + WS
 * subscription pattern.
 */

const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 15000;

export const C4_ROWS = 6;
export const C4_COLS = 7;

function emptyBoard() {
  return Array.from({ length: C4_ROWS }, () =>
    Array.from({ length: C4_COLS }, () => 'empty'),
  );
}

export class ConnectFourState {
  constructor(matchId) {
    this.matchId = matchId;

    // Game state (mirrors crates/connect-four-protocol::ConnectFourFullState).
    this.board = emptyBoard();
    this.currentPlayer = null;
    this.turn = 0;
    this.phase = 'playing';
    this.winner = null;
    this.terminalReason = null;
    this.moveHistory = [];
    this.players = []; // [{ player_id, disc, display_name }]
    this.playerNames = {};

    // WebSocket bookkeeping
    this.ws = null;
    this.reconnectAttempt = 0;
    this.isCatchingUp = false;
    this._closing = false;
    this._inFlight = false;
    this._pending = false;

    // Callback hooks — set by the viewer
    this.onMoveMade = null;
    this.onTurnChanged = null;
    this.onGameOver = null;
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
    this._applyFullState(body.state);
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
    } catch (_e) {
      /* best-effort */
    }
  }

  _applyFullState(state) {
    const prevHistory = this.moveHistory;
    this.board = state.board ?? emptyBoard();
    this.currentPlayer = state.current_player ?? null;
    this.turn = state.turn ?? 0;
    this.phase = state.phase;
    this.winner = state.winner ?? null;
    this.terminalReason = state.terminal_reason ?? null;
    this.moveHistory = state.move_history ?? [];
    this.players = state.players ?? [];
    return this.moveHistory.slice(prevHistory.length);
  }

  displayName(playerId) {
    if (playerId === null || playerId === undefined) return null;
    const fromMap = this.playerNames?.[String(playerId)];
    if (fromMap) return fromMap;
    const fromState = this.players?.find((p) => p.player_id === playerId);
    if (fromState?.display_name) return fromState.display_name;
    return `Player ${playerId}`;
  }

  /**
   * Wire-format disc identifier for a player ('blue' or 'orange'). Falls
   * back to engine convention (player 0 = blue, player 1 = orange).
   */
  discFor(playerId) {
    const fromState = this.players?.find((p) => p.player_id === playerId);
    if (fromState?.disc) return fromState.disc;
    return playerId === 0 ? 'blue' : 'orange';
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
      this._notifyConnection(this.phase === 'game_over' ? 'terminal' : 'running');
    };

    this.ws.onmessage = async (ev) => {
      let frame;
      try {
        frame = JSON.parse(ev.data);
      } catch {
        return;
      }
      if (frame.catchup_start) {
        this.isCatchingUp = true;
        return;
      }
      if (frame.catchup_end) {
        this.isCatchingUp = false;
        await this._refetchAndDispatch();
        return;
      }
      const eventType = frame.event_type;
      if (eventType === 'action' || eventType === 'terminal') {
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
    } catch {
      return;
    }
    const newMoves = this._applyFullState(body.state);

    for (const move of newMoves) {
      const disc = move.disc ?? this.discFor(move.player_id);
      if (this.onMoveMade) {
        this.onMoveMade({
          row: move.row,
          col: move.column,
          disc,
          playerId: move.player_id,
          turn: move.turn,
        });
      }
    }

    if (this.onTurnChanged) {
      const disc = this.currentPlayer !== null ? this.discFor(this.currentPlayer) : null;
      this.onTurnChanged(this.currentPlayer, disc);
    }

    if (this.phase === 'game_over' && this.onGameOver) {
      const winnerDisc =
        this.winner !== null && this.winner !== undefined
          ? this.discFor(this.winner)
          : null;
      this.onGameOver(this.winner, winnerDisc, this.terminalReason);
      this._notifyConnection('terminal');
    }
  }

  _scheduleReconnect() {
    const delay = Math.min(
      RECONNECT_BASE_MS * Math.pow(2, this.reconnectAttempt),
      RECONNECT_MAX_MS,
    );
    this.reconnectAttempt += 1;
    setTimeout(() => {
      if (!this._closing) this.connect();
    }, delay);
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
