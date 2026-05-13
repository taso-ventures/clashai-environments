/**
 * Vibe Check State Manager
 *
 * Manages game state via REST bootstrap + WebSocket live events.
 * Mirrors coup-state.js pattern: event queue, catchup suppression, reconnection.
 *
 * SpectatorEvent variants (snake_case serde):
 *   game_started, round_started, clue_given, agent_reasoning,
 *   guess_submitted, steal_guess_submitted, target_revealed,
 *   score_update, game_over
 */
export class VibeCheckStateManager {
  constructor(matchId, onEvent, onStateChange) {
    this.matchId = matchId;
    this.onEvent = onEvent;
    this.onStateChange = onStateChange;

    // Game state
    this.teams = new Map();          // teamId -> { team_id, score, player_ids }
    this.players = new Map();        // playerId -> { player_id, team, display_name }
    this.round = 0;
    this.phase = 'waiting';          // string phase name
    this.activeTeam = null;
    this.cluegiver = null;
    this.clue = null;
    this.spectrum = null;            // { left_endpoint, right_endpoint }
    this.zoneConfig = { bullseye_half_width: 0.04, near_half_width: 0.08, far_half_width: 0.12 };
    this.targetScore = 10;
    this.guessPosition = null;
    this.stealingTeam = null;
    this.stealDirection = null;
    this.targetPosition = null;
    this.winner = null;
    this.isGameOver = false;

    // Event queue
    this.eventQueue = [];
    this.isProcessingQueue = false;
    this.isCatchingUp = false;
    this.MAX_QUEUE_SIZE = 100;

    // Replay controller (set externally by viewer for completed matches)
    this.replayController = null;

    // WebSocket
    this.ws = null;
    this.reconnectAttempts = 0;
    this.maxReconnectAttempts = 5;
    this.reconnectDelay = 1000;
    this.connectionStatusCallback = null;
  }

  setConnectionStatusCallback(callback) {
    this.connectionStatusCallback = callback;
  }

  updateConnectionStatus(status) {
    if (this.connectionStatusCallback) {
      this.connectionStatusCallback(status);
    }
  }

  // ─── REST Bootstrap ───

  async loadInitialState() {
    try {
      const response = await fetch(`/matches/${this.matchId}/state`);
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      // Server wraps state in { "state": <env JSON> } per PROTOCOL.md.
      const body = await response.json();
      this.applyFullState(body.state);

      // Load player names (best-effort, non-blocking)
      await this.loadPlayerNames();
    } catch (err) {
      console.warn('[VibeState] Failed to load initial state:', err);
    }
  }

  async loadPlayerNames() {
    try {
      const response = await fetch(`/matches/${this.matchId}/player_names`);
      if (response.ok) {
        const names = await response.json();
        for (const [playerId, name] of Object.entries(names)) {
          const pid = parseInt(playerId);
          const existing = this.players.get(pid);
          if (existing) {
            existing.display_name = name;
          } else {
            this.players.set(pid, { player_id: pid, team: null, display_name: name });
          }
        }
      }
    } catch (e) {
      console.warn('[VibeState] Failed to load player names:', e);
    }
  }

  /**
   * Reset state to initial blank values.
   * Used by ReplayController.seek() before replaying events from zero.
   */
  resetState() {
    this.teams.clear();
    this.players.clear();
    this.round = 0;
    this.phase = 'waiting';
    this.activeTeam = null;
    this.cluegiver = null;
    this.clue = null;
    this.spectrum = null;
    this.guessPosition = null;
    this.stealingTeam = null;
    this.stealDirection = null;
    this.targetPosition = null;
    this.winner = null;
    this.isGameOver = false;
  }

  applyFullState(state) {
    this.round = state.round || 0;
    this.targetScore = state.target_score || 10;

    // Zone config
    if (state.zone_config) {
      this.zoneConfig = state.zone_config;
    }

    // Teams
    this.teams.clear();
    if (state.teams) {
      for (const team of state.teams) {
        this.teams.set(team.team_id, { ...team });
      }
    }

    // Players
    this.players.clear();
    if (state.players) {
      for (const player of state.players) {
        this.players.set(player.player_id, { ...player });
      }
    }

    // Spectrum (visible to all)
    this.spectrum = state.spectrum || null;

    // Target — filter out for spectator leak prevention
    // Spectators learn target ONLY via TargetRevealed event
    this.targetPosition = null;

    // Phase
    this.applyPhaseFromState(state.phase);

    // Round history
    this.roundHistory = state.round_history || [];

    this.isGameOver = state.is_game_over || false;

    this.notifyStateChange();
  }

  applyPhaseFromState(phase) {
    if (!phase) {
      this.phase = 'waiting';
      return;
    }

    // TurnPhase is a tagged enum: { "clue_phase": { active_team, cluegiver } }
    if (typeof phase === 'string') {
      this.phase = phase;
      return;
    }

    // Handle snake_case tagged enum variants
    if (phase.clue_phase) {
      this.phase = 'clue_phase';
      this.activeTeam = phase.clue_phase.active_team;
      this.cluegiver = phase.clue_phase.cluegiver;
    } else if (phase.guess_phase) {
      this.phase = 'guess_phase';
      this.activeTeam = phase.guess_phase.active_team;
      this.cluegiver = phase.guess_phase.cluegiver;
      this.clue = phase.guess_phase.clue;
    } else if (phase.steal_phase) {
      this.phase = 'steal_phase';
      this.activeTeam = phase.steal_phase.active_team;
      this.stealingTeam = phase.steal_phase.stealing_team;
      this.clue = phase.steal_phase.clue;
      this.guessPosition = phase.steal_phase.active_guess;
    } else if (phase.resolving) {
      this.phase = 'resolving';
      this.activeTeam = phase.resolving.active_team;
      this.stealingTeam = phase.resolving.stealing_team;
      this.clue = phase.resolving.clue;
      this.guessPosition = phase.resolving.active_guess;
      this.stealDirection = phase.resolving.steal_direction;
    } else if (phase.game_over !== undefined) {
      this.phase = 'game_over';
      this.winner = phase.game_over.winner;
      this.isGameOver = true;
    }
  }

  // ─── WebSocket Connection ───

  connect() {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${protocol}//${window.location.host}/matches/${this.matchId}/spectator/ws`;

    console.log('[VibeState] Connecting to', wsUrl);
    this.ws = new WebSocket(wsUrl);

    this.ws.onopen = () => {
      console.log('[VibeState] WebSocket connected');
      this.reconnectAttempts = 0;
      this.updateConnectionStatus('connected');
    };

    this.ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        this.queueEvent(data);
      } catch (err) {
        console.warn('[VibeState] Failed to parse WS message:', err);
      }
    };

    this.ws.onclose = () => {
      console.log('[VibeState] WebSocket disconnected');
      this.handleDisconnect();
    };

    this.ws.onerror = (err) => {
      console.error('[VibeState] WebSocket error:', err);
    };
  }

  handleDisconnect() {
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      this.updateConnectionStatus('failed');
      return;
    }

    this.reconnectAttempts++;
    const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1);
    this.updateConnectionStatus('reconnecting');

    console.log(`[VibeState] Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})`);

    setTimeout(async () => {
      try {
        await this.loadInitialState();
        this.connect();
      } catch (err) {
        console.error('[VibeState] Reconnection failed:', err);
        this.handleDisconnect();
      }
    }, delay);
  }

  // ─── Event Queue ───

  queueEvent(event) {
    if (this.eventQueue.length >= this.MAX_QUEUE_SIZE) {
      this.eventQueue.shift();
    }
    this.eventQueue.push(event);
    this.processQueue();
  }

  async processQueue() {
    if (this.isProcessingQueue) return;
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
    // Catchup markers
    if (event.catchup_start) {
      this.isCatchingUp = true;
      console.log('[VibeState] Catchup started');
      return;
    }
    if (event.catchup_end) {
      this.isCatchingUp = false;
      console.log('[VibeState] Catchup ended');
      this.notifyStateChange();
      return;
    }

    // Determine event type from snake_case tagged enum
    const eventType = this.getEventType(event);
    if (!eventType) {
      console.warn('[VibeState] Unknown event:', event);
      return;
    }

    // Apply state mutation
    this.applyEvent(eventType, event);

    // During catchup, skip animations (onEvent callback)
    if (!this.isCatchingUp) {
      try {
        await this.onEvent(eventType, event);
      } catch (err) {
        console.error('[VibeState] Event handler error:', err);
      }
      this.notifyStateChange();
    }
  }

  getEventType(event) {
    // SpectatorEvent is serde(rename_all = "snake_case") tagged enum
    // JSON comes as: { "game_started": { teams: [...], ... } }
    const knownTypes = [
      'game_started', 'round_started', 'clue_given', 'agent_reasoning',
      'guess_submitted', 'steal_guess_submitted', 'target_revealed',
      'score_update', 'game_over',
    ];
    for (const type of knownTypes) {
      if (event[type] !== undefined) return type;
    }
    return null;
  }

  // ─── State Mutations ───

  applyEvent(type, event) {
    const data = event[type];

    switch (type) {
      case 'game_started':
        this.teams.clear();
        for (const team of data.teams) {
          this.teams.set(team.team_id, { ...team });
        }
        this.players.clear();
        for (const player of data.players) {
          this.players.set(player.player_id, { ...player });
        }
        this.targetScore = data.target_score;
        this.round = 0;
        this.phase = 'game_started';
        this.isGameOver = false;
        this.winner = null;
        break;

      case 'round_started':
        this.round = data.round;
        this.activeTeam = data.active_team;
        this.cluegiver = data.cluegiver;
        this.spectrum = data.spectrum;
        this.clue = null;
        this.guessPosition = null;
        this.stealingTeam = null;
        this.stealDirection = null;
        this.targetPosition = null;
        this.phase = 'clue_phase';
        break;

      case 'clue_given':
        this.clue = data.clue;
        this.phase = 'guess_phase';
        break;

      case 'agent_reasoning':
        // No state mutation — just forwarded to UI
        break;

      case 'guess_submitted':
        this.guessPosition = data.position;
        this.phase = 'steal_phase';
        break;

      case 'steal_guess_submitted':
        this.stealDirection = data.direction;
        this.phase = 'resolving';
        break;

      case 'target_revealed':
        this.targetPosition = data.target_position;
        break;

      case 'score_update':
        for (const [teamId, score] of data.scores) {
          const team = this.teams.get(teamId);
          if (team) team.score = score;
        }
        break;

      case 'game_over':
        this.winner = data.winner;
        this.isGameOver = true;
        this.phase = 'game_over';
        // Update final scores
        if (data.final_scores) {
          for (const [teamId, score] of data.final_scores) {
            const team = this.teams.get(teamId);
            if (team) team.score = score;
          }
        }
        break;
    }
  }

  // ─── State Snapshot ───

  getState() {
    return {
      round: this.round,
      phase: this.phase,
      activeTeam: this.activeTeam,
      cluegiver: this.cluegiver,
      clue: this.clue,
      spectrum: this.spectrum,
      zoneConfig: this.zoneConfig,
      targetScore: this.targetScore,
      guessPosition: this.guessPosition,
      stealingTeam: this.stealingTeam,
      stealDirection: this.stealDirection,
      targetPosition: this.targetPosition,
      winner: this.winner,
      isGameOver: this.isGameOver,
      teams: Object.fromEntries(this.teams),
      players: Object.fromEntries(this.players),
    };
  }

  notifyStateChange() {
    try {
      this.onStateChange(this.getState());
    } catch (err) {
      console.error('[VibeState] State change handler error:', err);
    }
  }

  // ─── Helpers ───

  getTeamScore(teamId) {
    const team = this.teams.get(teamId);
    return team ? team.score : 0;
  }

  getPlayerName(playerId) {
    const player = this.players.get(playerId);
    if (!player) return `Agent ${playerId}`;
    return player.display_name || `Agent ${playerId}`;
  }

  getPlayerTeam(playerId) {
    const player = this.players.get(playerId);
    return player ? player.team : null;
  }

  /**
   * Apply an event's state mutations without triggering onEvent callback.
   * Used by ReplayController.seek() for silent state reconstruction.
   */
  applySilentEvent(eventType, event) {
    this.applyEvent(eventType, event);
    this.notifyStateChange();
  }

  dispose() {
    if (this.ws) {
      this.ws.onclose = null;
      this.ws.close();
      this.ws = null;
    }
    this.eventQueue = [];
  }
}
