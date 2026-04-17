/**
 * Coup State Management Module
 * Handles WebSocket connection, state tracking, and event queuing
 */

export class CoupStateManager {
  constructor(matchId, onEvent, onStateChange) {
    this.matchId = matchId;
    this.onEvent = onEvent;
    this.onStateChange = onStateChange;

    // Game state
    this.players = new Map();
    this.playerNames = new Map();
    this.pendingAction = null;
    this.currentPhase = 'awaiting_action';
    this.turnNumber = 0;
    this.activePlayer = null;
    this.deckCount = 0;
    this.winner = null;

    // Event queue for sequential processing
    this.eventQueue = [];
    this.isProcessingQueue = false;

    // Catchup mode: when true, state updates are applied but animations are skipped
    this.isCatchingUp = false;

    // Replay controller (set externally by viewer for completed matches)
    this.replayController = null;

    // WebSocket connection
    this.ws = null;
    this.reconnectAttempts = 0;
    this.maxReconnectAttempts = 5;
    this.reconnectDelay = 1000;
    this.connectionStatusCallback = null;
  }

  setConnectionStatusCallback(callback) {
    this.connectionStatusCallback = callback;
  }

  async loadInitialState() {
    const baseUrl = window.location.origin;
    const response = await fetch(`${baseUrl}/matches/${this.matchId}/state`);

    if (!response.ok) {
      throw new Error(`Failed to load state: ${response.status}`);
    }

    const state = await response.json();
    this.applyFullState(state);

    // Load player names (best-effort, non-blocking)
    await this.loadPlayerNames();

    return state;
  }

  async loadPlayerNames() {
    try {
      const baseUrl = window.location.origin;
      const response = await fetch(`${baseUrl}/matches/${this.matchId}/player_names`);
      if (response.ok) {
        const names = await response.json();
        for (const [playerId, name] of Object.entries(names)) {
          this.playerNames.set(parseInt(playerId), name);
        }
      }
    } catch (e) {
      console.warn('[CoupState] Failed to load player names:', e);
    }
  }

  /**
   * Reset state to initial blank values.
   * Used by ReplayController.seek() before replaying events from zero.
   */
  resetState() {
    this.players.clear();
    this.pendingAction = null;
    this.currentPhase = 'awaiting_action';
    this.turnNumber = 0;
    this.activePlayer = null;
    this.deckCount = 0;
    this.winner = null;
  }

  applyFullState(state) {
    this.turnNumber = state.turn_number;
    this.currentPhase = state.current_phase;
    this.activePlayer = state.active_player;
    this.pendingAction = state.pending_action;
    this.deckCount = state.deck_count;

    this.players.clear();
    for (const [playerId, playerState] of Object.entries(state.players)) {
      this.players.set(parseInt(playerId), {
        coins: playerState.coins,
        cards: playerState.cards,
        eliminated: playerState.eliminated,
      });
    }

    // Check for game over
    if (state.current_phase.game_over) {
      this.winner = state.current_phase.game_over.winner;
    }

    this.notifyStateChange();
  }

  connect() {
    const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${wsProtocol}//${window.location.host}/matches/${this.matchId}/spectator/ws`;

    this.ws = new WebSocket(wsUrl);

    this.ws.onopen = () => {
      console.log('[CoupState] WebSocket connected');
      this.reconnectAttempts = 0;
      this.updateConnectionStatus('connected');
    };

    this.ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        this.queueEvent(data);
      } catch (e) {
        console.error('[CoupState] Failed to parse event:', e);
      }
    };

    this.ws.onclose = (event) => {
      console.log('[CoupState] WebSocket closed:', event.code);
      this.handleDisconnect();
    };

    this.ws.onerror = (error) => {
      console.error('[CoupState] WebSocket error:', error);
    };
  }

  handleDisconnect() {
    if (this.reconnectAttempts < this.maxReconnectAttempts) {
      this.reconnectAttempts++;
      const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1);

      console.log(`[CoupState] Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})`);
      this.updateConnectionStatus('reconnecting');

      setTimeout(() => {
        this.loadInitialState()
          .then(() => this.connect())
          .catch((e) => {
            console.error('[CoupState] Reconnect failed:', e);
            this.handleDisconnect();
          });
      }, delay);
    } else {
      console.error('[CoupState] Max reconnection attempts reached');
      this.updateConnectionStatus('failed');
    }
  }

  updateConnectionStatus(status) {
    if (this.connectionStatusCallback) {
      this.connectionStatusCallback(status);
    }
  }

  queueEvent(event) {
    const MAX_QUEUE_SIZE = 100;
    if (this.eventQueue.length >= MAX_QUEUE_SIZE) {
      console.warn('[CoupState] Event queue overflow, dropping oldest events');
      this.eventQueue.splice(0, this.eventQueue.length - MAX_QUEUE_SIZE + 1);
    }
    this.eventQueue.push(event);
    this.processQueue();
  }

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

  async processEvent(event) {
    // Handle catchup markers from server (live game reconnect)
    if (event.catchup_start) {
      this.isCatchingUp = true;
      return;
    }
    if (event.catchup_end) {
      this.isCatchingUp = false;
      this.notifyStateChange();
      return;
    }

    // Update internal state based on event type
    const eventType = Object.keys(event)[0];
    const eventData = event[eventType];

    switch (eventType) {
      case 'game_started':
        this.handleGameStarted(eventData);
        break;

      case 'turn_advanced':
        this.handleTurnAdvanced(eventData);
        break;

      case 'agent_reasoning':
        this.handleAgentReasoning(eventData);
        break;

      case 'action_declared':
        this.handleActionDeclared(eventData);
        break;

      case 'challenge_issued':
        this.handleChallengeIssued(eventData);
        break;

      case 'block_declared':
        this.handleBlockDeclared(eventData);
        break;

      case 'card_revealed':
        this.handleCardRevealed(eventData);
        break;

      case 'influence_lost':
        this.handleInfluenceLost(eventData);
        break;

      case 'player_eliminated':
        this.handlePlayerEliminated(eventData);
        break;

      case 'game_over':
        this.handleGameOver(eventData);
        break;
    }

    // During catchup, update state silently (no animations)
    if (this.isCatchingUp) {
      return;
    }

    // Notify listeners (triggers animations in viewer)
    if (this.onEvent) {
      await this.onEvent(eventType, eventData);
    }

    this.notifyStateChange();
  }

  handleGameStarted(data) {
    this.players.clear();
    for (const player of data.players) {
      this.players.set(player.player_id, {
        coins: 2, // Initial coins
        cards: [{ revealed: false }, { revealed: false }], // Hidden cards
        eliminated: player.eliminated,
      });
    }
    this.turnNumber = 1;
    this.activePlayer = 0;
    this.currentPhase = 'awaiting_action';
  }

  handleTurnAdvanced(data) {
    this.turnNumber = data.turn;
    this.activePlayer = data.active_player;
    this.currentPhase = 'awaiting_action';
    this.pendingAction = null;
  }

  handleAgentReasoning(data) {
    // Store latest reasoning per player for UI display
    if (!this.agentReasoning) {
      this.agentReasoning = new Map();
    }
    this.agentReasoning.set(data.player, data.reasoning);
  }

  handleActionDeclared(data) {
    this.pendingAction = {
      actor: data.player,
      action: data.action,
    };

    // Extract target if present
    if (data.action.target !== undefined) {
      this.pendingAction.target = data.action.target;
    }

    // Derive phase from action type
    const actionType = data.action.action_type || Object.keys(data.action)[0];
    const challengeable = ['tax', 'assassinate', 'steal', 'exchange'];
    const blockable = ['foreign_aid'];
    if (challengeable.includes(actionType)) {
      this.currentPhase = 'challenge_window';
    } else if (blockable.includes(actionType)) {
      this.currentPhase = 'block_window';
    } else {
      this.currentPhase = 'action_resolving';
    }
  }

  handleChallengeIssued(data) {
    if (this.pendingAction) {
      this.pendingAction.challengedBy = data.challenger;
    }
    this.currentPhase = 'revealing_card';
  }

  handleBlockDeclared(data) {
    if (this.pendingAction) {
      this.pendingAction.blockedBy = data.blocker;
      this.pendingAction.blockClaimedRole = data.role;
    }
    this.currentPhase = 'block_challenge_window';
  }

  handleCardRevealed(data) {
    const player = this.players.get(data.player);
    if (player) {
      // Find first unrevealed card and reveal it
      const card = player.cards.find((c) => !c.revealed);
      if (card) {
        card.role = data.role;
        card.revealed = true;
      }
    }
  }

  handleInfluenceLost(data) {
    const player = this.players.get(data.player);
    if (player) {
      // Mark a card as lost (revealed)
      const card = player.cards.find((c) => !c.revealed);
      if (card) {
        card.role = data.role;
        card.revealed = true;
      }
    }
    this.currentPhase = 'action_resolving';
  }

  handlePlayerEliminated(data) {
    const player = this.players.get(data.player);
    if (player) {
      player.eliminated = true;
    }
    this.currentPhase = 'action_resolving';
  }

  handleGameOver(data) {
    this.winner = data.winner;
    this.currentPhase = { game_over: { winner: data.winner } };
  }

  notifyStateChange() {
    if (this.onStateChange) {
      this.onStateChange(this.getState());
    }
  }

  getState() {
    return {
      players: this.players,
      playerNames: this.playerNames,
      pendingAction: this.pendingAction,
      currentPhase: this.currentPhase,
      turnNumber: this.turnNumber,
      activePlayer: this.activePlayer,
      deckCount: this.deckCount,
      winner: this.winner,
      agentReasoning: this.agentReasoning || new Map(),
    };
  }

  getPlayerName(playerId) {
    return this.playerNames.get(playerId) || `Player ${playerId}`;
  }

  getPlayerState(playerId) {
    return this.players.get(playerId);
  }

  isPlayerActive(playerId) {
    return this.activePlayer === playerId;
  }

  isPlayerEliminated(playerId) {
    const player = this.players.get(playerId);
    return player ? player.eliminated : false;
  }

  getPlayerCount() {
    return this.players.size;
  }

  /**
   * Apply an event's state mutations without triggering onEvent callback.
   * Used by ReplayController.seek() for silent state reconstruction.
   */
  applySilentEvent(eventType, event) {
    const data = event[eventType];
    if (!data && eventType !== 'game_over') return;

    switch (eventType) {
      case 'game_started':
        this.handleGameStarted(data);
        break;
      case 'turn_advanced':
        this.handleTurnAdvanced(data);
        break;
      case 'agent_reasoning':
        this.handleAgentReasoning(data);
        break;
      case 'action_declared':
        this.handleActionDeclared(data);
        break;
      case 'challenge_issued':
        this.handleChallengeIssued(data);
        break;
      case 'block_declared':
        this.handleBlockDeclared(data);
        break;
      case 'card_revealed':
        this.handleCardRevealed(data);
        break;
      case 'influence_lost':
        this.handleInfluenceLost(data);
        break;
      case 'player_eliminated':
        this.handlePlayerEliminated(data);
        break;
      case 'game_over':
        this.handleGameOver(data);
        break;
    }

    this.notifyStateChange();
  }

  disconnect() {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }
}

// Helper to format action text for display
export function formatAction(action) {
  const actionType = action.action_type || Object.keys(action)[0];

  const actionNames = {
    income: 'Income',
    foreign_aid: 'Foreign Aid',
    coup: 'Coup',
    tax: 'Tax',
    assassinate: 'Assassinate',
    steal: 'Steal',
    exchange: 'Exchange',
    challenge: 'Challenge',
    block: 'Block',
    pass: 'Pass',
  };

  return actionNames[actionType] || actionType;
}

// Helper to get role display info
export function getRoleInfo(role) {
  const roles = {
    duke: { name: 'Duke', icon: '\u{1F451}', color: '#9b59b6' },
    assassin: { name: 'Assassin', icon: '\u{1F5E1}', color: '#2c3e50' },
    captain: { name: 'Captain', icon: '\u{1F6E1}', color: '#3498db' },
    ambassador: { name: 'Ambassador', icon: '\u{1F4DC}', color: '#27ae60' },
    contessa: { name: 'Contessa', icon: '\u{2B50}', color: '#e74c3c' },
    unknown: { name: 'Unknown', icon: '?', color: '#7f8c8d' },
  };

  return roles[role] || roles.unknown;
}
