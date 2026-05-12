/**
 * Tic-Tac-Toe State Manager
 *
 * Bootstraps from REST GET /matches/:id/state, then re-fetches on every
 * UnifiedEvent action broadcast over the spectator WebSocket. The
 * tic_tac_toe environment adapter returns events: [] from apply_action
 * (the action stream's "what changed" payload is intentionally minimal),
 * so the viewer derives deltas by diffing the freshly-fetched full state
 * against the previously-applied snapshot.
 */

const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 15000;

export class TicTacToeState {
  constructor(matchId) {
    this.matchId = matchId;

    // Game state (mirrors crates/tic-tac-toe-protocol::TicTacToeFullState).
    this.board = [
      ['empty', 'empty', 'empty'],
      ['empty', 'empty', 'empty'],
      ['empty', 'empty', 'empty'],
    ];
    this.currentPlayer = null; // 0 | 1 | null
    this.turn = 0;
    this.phase = 'playing';
    this.winner = null;
    this.terminalReason = null;
    this.moveHistory = [];
    this.players = []; // [{ player_id, mark, display_name }]
    this.playerNames = {}; // { '0': 'Alice', '1': 'Bob' } from /player_names

    // WebSocket bookkeeping
    this.ws = null;
    this.reconnectAttempt = 0;
    this.isCatchingUp = false;
    this._closing = false;
    this._inFlight = false;
    this._pending = false;

    // Callback hooks — set by the viewer
    this.onInitialState = null;
    this.onMoveMade = null; // (move, board, currentPlayer, turn) => void
    this.onGameOver = null; // (winner, winnerMark, terminalReason) => void
    this.onTurnChanged = null; // (currentPlayer, mark) => void
    this.onConnectionChange = null; // ('connecting' | 'running' | 'reconnecting' | 'terminal') => void
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
        // Endpoint returns either { player_names: { '0': 'Alice', ... } }
        // or the bare map; handle both.
        this.playerNames = names.player_names ?? names ?? {};
      }
    } catch (_e) {
      // Best-effort; falls back to display_name from state.players.
    }
  }

  /**
   * Apply a freshly-fetched full state. Returns the list of *new* moves
   * since the previous snapshot so the viewer can animate just those.
   */
  _applyFullState(state) {
    const prevHistory = this.moveHistory;
    this.board = state.board;
    this.currentPlayer = state.current_player;
    this.turn = state.turn ?? state.turn_number ?? 0;
    this.phase = state.phase;
    this.winner = state.winner ?? null;
    this.terminalReason = state.terminal_reason ?? null;
    this.moveHistory = state.move_history ?? [];
    this.players = state.players ?? [];

    const newMoves = this.moveHistory.slice(prevHistory.length);
    return newMoves;
  }

  /** Map a player_id to a display name, falling back through known sources. */
  displayName(playerId) {
    if (playerId === null || playerId === undefined) return null;
    const fromMap = this.playerNames?.[String(playerId)];
    if (fromMap) return fromMap;
    const fromState = this.players?.find((p) => p.player_id === playerId);
    if (fromState?.display_name) return fromState.display_name;
    return `Player ${playerId}`;
  }

  /** Map a player_id to its mark ('x' | 'o'), falling back to engine convention. */
  markFor(playerId) {
    const fromState = this.players?.find((p) => p.player_id === playerId);
    if (fromState?.mark) return fromState.mark;
    return playerId === 0 ? 'x' : 'o';
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
      // Catchup markers — bare {catchup_start: true} / {catchup_end: true}
      // frames per PROTOCOL.md; not UnifiedEvent envelopes.
      if (frame.catchup_start) {
        this.isCatchingUp = true;
        return;
      }
      if (frame.catchup_end) {
        this.isCatchingUp = false;
        // After catchup, force a state re-fetch so the viewer is in sync
        // with whatever happened during the replayed window.
        await this._refetchAndDispatch();
        return;
      }
      // UnifiedEvent envelope. We don't read .action (always [] for ttt);
      // any action-class event triggers a re-fetch.
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

    this.ws.onerror = () => {
      // Let onclose handle the reconnect.
    };
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
      const mark = move.mark ?? this.markFor(move.player_id);
      if (this.onMoveMade) {
        this.onMoveMade({
          row: move.row,
          col: move.col,
          mark,
          playerId: move.player_id,
          turn: move.turn,
        });
      }
    }

    if (this.onTurnChanged) {
      const mark = this.currentPlayer !== null ? this.markFor(this.currentPlayer) : null;
      this.onTurnChanged(this.currentPlayer, mark);
    }

    if (this.phase === 'game_over' && this.onGameOver) {
      const winnerMark =
        this.winner !== null && this.winner !== undefined
          ? this.markFor(this.winner)
          : null;
      this.onGameOver(this.winner, winnerMark, this.terminalReason);
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
