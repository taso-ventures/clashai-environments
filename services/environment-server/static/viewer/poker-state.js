/**
 * Poker State Manager
 *
 * Bootstraps from REST GET /matches/:id/state (which returns a
 * MatchState envelope containing hand_number, profits, current_hand, etc.)
 * and re-fetches on every UnifiedEvent action/terminal frame. Diff-driven
 * dispatch: emits onHandChange when hand_number advances and
 * onHandSnapshot on every refetch (the renderer reconciles cards, chips,
 * stacks, button position against the snapshot).
 */

const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 15000;

export class PokerState {
  constructor(matchId) {
    this.matchId = matchId;

    // Mirrors crates/poker-protocol::MatchState.
    this.handNumber = 0;
    this.maxHands = 0;
    this.profits = [0, 0];
    this.phase = 'pre_match'; // pre_match | playing | completed
    this.button = 0;
    this.currentHand = null; // HandState | null
    this.handHistory = [];
    this.playerNames = {};

    this.ws = null;
    this.reconnectAttempt = 0;
    this.isCatchingUp = false;
    this._closing = false;
    this._inFlight = false;
    this._pending = false;

    this.onConnectionChange = null;
    this.onHandSnapshot = null; // (matchState) — fires every refetch
    this.onHandChange = null;   // (newHandNumber) — fires on new hand
    this.onMatchOver = null;    // (winner | null, profits)
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
    } catch (_e) { /* best-effort */ }
  }

  _applyFullState(state) {
    const prevHand = this.handNumber;
    this.handNumber = state.hand_number ?? 0;
    this.maxHands = state.max_hands ?? 0;
    this.profits = state.profits ?? [0, 0];
    this.phase = state.phase ?? 'pre_match';
    this.button = state.button ?? 0;
    this.currentHand = state.current_hand ?? null;
    this.handHistory = state.hand_history ?? [];
    return { prevHand, newHand: this.handNumber };
  }

  displayName(playerId) {
    if (playerId === null || playerId === undefined) return null;
    const fromMap = this.playerNames?.[String(playerId)];
    if (fromMap) return fromMap;
    return `Player ${playerId}`;
  }

  isTerminal() {
    return this.phase === 'completed';
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
      this._notifyConnection(this.isTerminal() ? 'terminal' : 'running');
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
    const { prevHand, newHand } = this._applyFullState(body.state);

    if (newHand !== prevHand && this.onHandChange) {
      this.onHandChange(newHand);
    }
    if (this.onHandSnapshot) {
      this.onHandSnapshot({
        handNumber: this.handNumber,
        maxHands: this.maxHands,
        profits: this.profits,
        phase: this.phase,
        button: this.button,
        currentHand: this.currentHand,
      });
    }

    if (this.isTerminal() && this.onMatchOver) {
      let winner = null;
      if (this.profits[0] > this.profits[1]) winner = 0;
      else if (this.profits[1] > this.profits[0]) winner = 1;
      this.onMatchOver(winner, this.profits);
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
