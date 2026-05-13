/**
 * Red Button State Manager
 *
 * Parses UnifiedEvent envelopes from the spectator WebSocket
 * and dispatches to callback hooks for UI/renderer updates.
 *
 * Supports replay mode: when `replayController` is set, events
 * are buffered for paced playback instead of processed immediately.
 */

const DEFAULT_MAX_TURNS = 200;

export class RedButtonState {
  constructor() {
    this.round = 0;
    this.maxTurns = DEFAULT_MAX_TURNS;
    this.isTerminal = false;
    this.currentActor = null;
    this.playerNames = {};
    this.eventCount = 0;

    // Event queue for ordered processing
    this.eventQueue = [];
    this.isProcessingQueue = false;

    // Catchup mode (suppress animations during REST bootstrap / reconnect)
    this.isCatchingUp = false;
    this._pendingTerminal = null;

    // Replay controller (set externally by viewer when in replay mode)
    this.replayController = null;

    // Callback hooks — set by the viewer
    this.onGameStarted = null;
    this.onRoundAdvance = null;
    this.onMessageSpoken = null;
    this.onActionTaken = null;
    this.onReasoning = null;
    this.onButtonPressed = null;
    this.onGameOver = null;
    this.onTurnIndicator = null;
  }

  /**
   * Reset state to initial blank values.
   * Used by ReplayController.seek() before replaying events from zero.
   */
  resetState() {
    this.round = 0;
    this.maxTurns = DEFAULT_MAX_TURNS;
    this.isTerminal = false;
    this.currentActor = null;
    this.playerNames = {};
    this.eventCount = 0;
    this._pendingTerminal = null;
  }

  /**
   * Queue an event for ordered processing (supports replay buffering).
   * @param {object} event - Parsed JSON from WebSocket
   */
  queueEvent(event) {
    const MAX_QUEUE_SIZE = 100;
    if (this.eventQueue.length >= MAX_QUEUE_SIZE) {
      console.warn('[RBState] Event queue overflow, dropping oldest events');
      this.eventQueue.splice(0, this.eventQueue.length - MAX_QUEUE_SIZE + 1);
    }
    this.eventQueue.push(event);
    this.processQueue();
  }

  /**
   * Process queued events. In replay mode, buffers events for the
   * ReplayController instead of processing them directly.
   */
  async processQueue() {
    if (this.isProcessingQueue || this.eventQueue.length === 0) {
      return;
    }

    this.isProcessingQueue = true;

    while (this.eventQueue.length > 0) {
      const event = this.eventQueue.shift();

      // In replay mode, buffer events (skip catchup markers)
      if (this.replayController && !event.catchup_start && !event.catchup_end) {
        this.replayController.bufferEvent(event);
        continue;
      }

      await this.processEvent(event);
    }

    this.isProcessingQueue = false;

    // Start replay playback after all events are buffered
    if (this.replayController && this.replayController.totalEvents > 0 && !this.replayController.isPlaying) {
      this.replayController.startPlayback();
    }
  }

  /**
   * Parse a raw UnifiedEvent envelope and dispatch to callbacks.
   * @param {object} raw - Parsed JSON from WebSocket
   */
  processEvent(raw) {
    // Handle catchup markers from server
    if (raw.catchup_start) {
      this.isCatchingUp = true;
      return;
    }
    if (raw.catchup_end) {
      this.isCatchingUp = false;
      this._syncUiAfterCatchup();
      return;
    }

    this.eventCount++;

    const et = raw.event_type;
    const action = raw.action;
    if (!action) return;

    // Match start
    if (et === 'match_start') {
      const players = action.player_names || {};
      this.playerNames = players;
      if (!this.isCatchingUp && this.onGameStarted) this.onGameStarted(players);
      return;
    }

    // Terminal
    if (et === 'terminal' || raw.is_terminal) {
      this.isTerminal = true;
      if (this.isCatchingUp) {
        this._pendingTerminal = action;
      } else {
        this._handleTerminal(action);
      }
      return;
    }

    // System events (reasoning)
    if (et === 'system') {
      const evtName = action.event_name;
      if (evtName === 'agent_reasoning') {
        const role = action.role || this._guessRole(action.player_id);
        if (!this.isCatchingUp && this.onReasoning) {
          this.onReasoning(role, action.reasoning || '', this.round);
        }
      }
      return;
    }

    // Action events — may be array of SpectatorEvents
    if (et === 'action') {
      const events = Array.isArray(action) ? action : [action];
      for (const ev of events) {
        this._handleSpectatorEvent(ev);
      }
      return;
    }

    // Bare array fallback (older format)
    if (Array.isArray(action)) {
      for (const ev of action) {
        this._handleSpectatorEvent(ev);
      }
    }
  }

  /**
   * Apply a raw event silently (state mutation only, no callbacks).
   * Used by ReplayController.seek() for fast state reconstruction.
   * @param {string} _eventType - Unused (event type from ReplayController)
   * @param {object} raw - Raw event object
   */
  applySilentEvent(_eventType, raw) {
    const et = raw.event_type;
    const action = raw.action;
    if (!action) return;

    if (et === 'match_start') {
      this.playerNames = action.player_names || {};
      return;
    }

    if (et === 'terminal' || raw.is_terminal) {
      this.isTerminal = true;
      return;
    }

    if (et === 'action') {
      const events = Array.isArray(action) ? action : [action];
      for (const ev of events) {
        this._applySilentSpectatorEvent(ev);
      }
      return;
    }

    if (Array.isArray(action)) {
      for (const ev of action) {
        this._applySilentSpectatorEvent(ev);
      }
    }
  }

  _applySilentSpectatorEvent(ev) {
    if (!ev || !ev.event_name) return;

    switch (ev.event_name) {
      case 'game_started':
        if (ev.config_summary?.max_turns) this.maxTurns = ev.config_summary.max_turns;
        break;
      case 'turn_advanced':
        this.round = ev.round || this.round + 1;
        this.currentActor = ev.actor || null;
        break;
      case 'button_pressed':
        break;
      case 'game_over':
        this.isTerminal = true;
        break;
    }
  }

  _handleSpectatorEvent(ev) {
    if (!ev || !ev.event_name) return;

    switch (ev.event_name) {
      case 'game_started':
        if (ev.config_summary?.max_turns) this.maxTurns = ev.config_summary.max_turns;
        if (!this.isCatchingUp && this.onRoundAdvance) this.onRoundAdvance(1);
        break;

      case 'turn_advanced': {
        this.round = ev.round || this.round + 1;
        const actor = ev.actor || null;
        this.currentActor = actor;
        if (!this.isCatchingUp) {
          if (this.onRoundAdvance) this.onRoundAdvance(this.round);
          if (this.onTurnIndicator && actor) this.onTurnIndicator(actor);
        }
        break;
      }

      case 'message_spoken': {
        const role = ev.speaker_role || 'persuader';
        if (this.onMessageSpoken) {
          this.onMessageSpoken(role, ev.message, ev.turn);
        }
        break;
      }

      case 'action_taken': {
        const role = ev.actor_role || 'resistor';
        if (this.onActionTaken) {
          this.onActionTaken(role, ev.action_type, ev.turn);
        }
        break;
      }

      case 'agent_reasoning': {
        const role = ev.player < 1 ? 'persuader' : 'resistor';
        if (!this.isCatchingUp && this.onReasoning) {
          this.onReasoning(role, ev.reasoning || '', ev.turn);
        }
        break;
      }

      case 'button_pressed':
        if (!this.isCatchingUp && this.onButtonPressed) this.onButtonPressed(ev.turn);
        break;

      case 'game_over':
        this.isTerminal = true;
        if (this.isCatchingUp) {
          this._pendingTerminal = ev;
        } else {
          this._handleTerminal(ev);
        }
        break;
    }
  }

  /**
   * After catchup ends, sync the UI with the accumulated internal state.
   * Conversation messages are rendered during catchup, but round counter,
   * turn indicator, and terminal overlay need an explicit refresh.
   */
  _syncUiAfterCatchup() {
    if (this.round > 0 && this.onRoundAdvance) {
      this.onRoundAdvance(this.round);
    }
    if (this.currentActor && this.onTurnIndicator) {
      this.onTurnIndicator(this.currentActor);
    }
    if (this.isTerminal && this._pendingTerminal) {
      this._handleTerminal(this._pendingTerminal);
      this._pendingTerminal = null;
    }
  }

  _handleTerminal(ev) {
    const winner = ev.winner_role || 'unknown';
    const reason = ev.terminal_reason || '';
    if (this.onGameOver) this.onGameOver(winner, reason);
  }

  _guessRole(playerId) {
    return playerId === 0 || playerId === '0' ? 'persuader' : 'resistor';
  }
}
