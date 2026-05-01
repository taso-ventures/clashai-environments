/**
 * Shared Replay Controller
 *
 * Client-side replay engine for completed matches. Buffers all events
 * received via WebSocket catchup, then plays them back with per-event-type
 * timing delays. Supports play/pause, seek, step, and speed control.
 *
 * No backend changes required — timing is entirely client-side using
 * a delay table keyed by event type.
 */

export class ReplayController {
  /**
   * @param {object} [options]
   * @param {function} [options.onEvent] - (eventType, eventData) => Promise — animated playback
   * @param {function} [options.onSilentEvent] - (eventType, eventData) => void — state-only, no anim
   * @param {function} [options.onReset] - () => void — reset state to blank before seek replay
   * @param {function} [options.onProgress] - (current, total) => void — scrubber update
   * @param {function} [options.onPlayStateChange] - (isPlaying) => void — play/pause UI
   */
  constructor(options = {}) {
    this.events = [];
    this.currentIndex = -1;
    this.isPlaying = false;
    this.playbackSpeed = 1.0;
    this.playbackTimer = null;

    // Per-event-type delays in ms at 1x speed
    this.eventDelays = {
      game_started: 500,
      turn_advanced: 800,
      round_started: 1000,
      action_declared: 1200,
      challenge_issued: 1500,
      block_declared: 1200,
      card_revealed: 800,
      influence_lost: 1000,
      player_eliminated: 1500,
      clue_given: 1500,
      guess_submitted: 1000,
      steal_guess_submitted: 1000,
      target_revealed: 2000,
      score_update: 800,
      agent_reasoning: 1500, // base; overridden dynamically in scheduleNext()
      game_over: 2000,
      default: 800,
    };

    // Callbacks
    this.onEvent = options.onEvent || null;
    this.onSilentEvent = options.onSilentEvent || null;
    this.onReset = options.onReset || null;
    this.onProgress = options.onProgress || null;
    this.onPlayStateChange = options.onPlayStateChange || null;
  }

  /**
   * Buffer an event for replay. Called during catchup phase.
   * @param {object} event - Raw event object from WebSocket
   */
  bufferEvent(event) {
    this.events.push(event);
  }

  /**
   * Get the event type string from a raw event object.
   * @param {object} event
   * @returns {string}
   */
  getEventType(event) {
    // Events are { "event_type": { ...data } } tagged enums
    for (const key of Object.keys(event)) {
      if (key !== 'catchup_start' && key !== 'catchup_end') {
        return key;
      }
    }
    return 'default';
  }

  /**
   * Start or resume playback from current position.
   */
  async startPlayback() {
    if (this.events.length === 0) return;
    if (this.isPlaying) return;

    this.isPlaying = true;
    this.onPlayStateChange?.(true);
    this.scheduleNext();
  }

  /**
   * Schedule the next event with appropriate delay.
   * @private
   */
  scheduleNext() {
    if (!this.isPlaying) return;

    if (this.currentIndex >= this.events.length - 1) {
      // Reached end of replay
      this.isPlaying = false;
      this.onPlayStateChange?.(false);
      return;
    }

    const nextEvent = this.events[this.currentIndex + 1];
    const eventType = this.getEventType(nextEvent);
    let baseDelay = this.eventDelays[eventType] ?? this.eventDelays.default;

    // Dynamic delay for agent_reasoning: scale with text length to match typing animation
    if (eventType === 'agent_reasoning') {
      const reasoning = nextEvent[eventType]?.reasoning || '';
      baseDelay = Math.max(1500, Math.min(reasoning.length * 40 + 500, 4000));
    }

    const delay = baseDelay / this.playbackSpeed;

    this.playbackTimer = setTimeout(async () => {
      if (!this.isPlaying) return;

      this.currentIndex++;
      const event = this.events[this.currentIndex];
      const type = this.getEventType(event);

      try {
        await this.onEvent?.(type, event);
      } catch (err) {
        console.warn('[ReplayController] Event handler error:', err);
      }

      this.onProgress?.(this.currentIndex, this.events.length);
      this.scheduleNext();
    }, delay);
  }

  /**
   * Pause playback.
   */
  pause() {
    clearTimeout(this.playbackTimer);
    this.playbackTimer = null;
    this.isPlaying = false;
    this.onPlayStateChange?.(false);
  }

  /**
   * Resume playback from current position.
   */
  resume() {
    if (this.isPlaying) return;
    this.isPlaying = true;
    this.onPlayStateChange?.(true);
    this.scheduleNext();
  }

  /**
   * Toggle play/pause.
   */
  togglePlayPause() {
    if (this.isPlaying) {
      this.pause();
    } else {
      this.resume();
    }
  }

  /**
   * Set playback speed multiplier.
   * @param {number} speed - 0.5, 1, 2, or 4
   */
  setSpeed(speed) {
    this.playbackSpeed = speed;
    // If currently playing, restart scheduling with new speed
    if (this.isPlaying) {
      clearTimeout(this.playbackTimer);
      this.scheduleNext();
    }
  }

  /**
   * Seek to a specific event index.
   * Applies events 0..targetIndex silently (state-only, no animations),
   * then pauses. User can click play to resume animated playback.
   *
   * @param {number} targetIndex
   */
  async seek(targetIndex) {
    this.pause();

    const clampedIndex = Math.max(-1, Math.min(targetIndex, this.events.length - 1));

    // Reset state to blank before replaying from zero — critical for backward seeks
    this.onReset?.();

    // Apply all events up to target silently
    for (let i = 0; i <= clampedIndex && i < this.events.length; i++) {
      const event = this.events[i];
      const type = this.getEventType(event);
      try {
        this.onSilentEvent?.(type, event);
      } catch (err) {
        console.warn('[ReplayController] Silent event error:', err);
      }
    }

    this.currentIndex = clampedIndex;
    this.onProgress?.(this.currentIndex, this.events.length);
  }

  /**
   * Step forward one event (animated).
   */
  async stepForward() {
    if (this.currentIndex >= this.events.length - 1) return;
    this.pause();

    this.currentIndex++;
    const event = this.events[this.currentIndex];
    const type = this.getEventType(event);

    try {
      await this.onEvent?.(type, event);
    } catch (err) {
      console.warn('[ReplayController] Step forward error:', err);
    }

    this.onProgress?.(this.currentIndex, this.events.length);
  }

  /**
   * Step backward one event (seek to previous index).
   */
  async stepBackward() {
    if (this.currentIndex <= 0) return;
    await this.seek(this.currentIndex - 1);
  }

  /**
   * Reset to beginning.
   */
  reset() {
    this.pause();
    this.currentIndex = -1;
    this.onProgress?.(-1, this.events.length);
  }

  /**
   * Get total event count.
   * @returns {number}
   */
  get totalEvents() {
    return this.events.length;
  }
}
