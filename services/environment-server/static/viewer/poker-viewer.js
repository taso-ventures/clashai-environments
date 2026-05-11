/**
 * Poker Spectator Viewer Orchestrator
 *
 * Wires renderer + state manager + DOM overlay. Same bootstrap pattern as
 * ttt/c4/wordle — REST initial snapshot, WS subscribe, reconcile the
 * holographic table on every action event.
 */

import { PokerRenderer } from './poker-render.js';
import { PokerState } from './poker-state.js';

const ROUND_LABELS = {
  pre_flop: 'PRE-FLOP',
  flop: 'FLOP',
  turn: 'TURN',
  river: 'RIVER',
  showdown: 'SHOWDOWN',
  preflop: 'PRE-FLOP',
};

// Poker protocol serializes Rank as "two".."ten", "jack", "queen", "king", "ace"
// and Suit as "hearts" | "diamonds" | "clubs" | "spades".
const RANK_GLYPH = {
  two: '2', three: '3', four: '4', five: '5', six: '6', seven: '7',
  eight: '8', nine: '9', ten: '10', jack: 'J', queen: 'Q', king: 'K', ace: 'A',
};
const SUIT_GLYPH = {
  hearts: '♥',
  diamonds: '♦',
  clubs: '♣',
  spades: '♠',
};
const RED_SUITS = new Set(['hearts', 'diamonds']);

const COMMUNITY_SLOTS = 5;
const HOLE_CARDS_PER_PLAYER = 2;

function buildCardFace(card, { faceDown = false, empty = false } = {}) {
  const el = document.createElement('div');
  el.className = 'card-face';
  const rankEl = document.createElement('span');
  rankEl.className = 'rank';
  const suitEl = document.createElement('span');
  suitEl.className = 'suit';
  el.appendChild(rankEl);
  el.appendChild(suitEl);

  if (empty) {
    el.classList.add('empty');
    return el;
  }
  if (faceDown || !card) {
    el.classList.add('face-down');
    return el;
  }
  const suit = String(card.suit ?? '').toLowerCase();
  const rank = String(card.rank ?? '').toLowerCase();
  rankEl.textContent = RANK_GLYPH[rank] ?? '?';
  suitEl.textContent = SUIT_GLYPH[suit] ?? '?';
  el.classList.add(RED_SUITS.has(suit) ? 'red' : 'black');
  return el;
}

class PokerViewer {
  constructor() {
    const url = new URL(window.location.href);
    this.matchId = url.searchParams.get('matchId') || url.searchParams.get('match_id');

    this.canvas = document.getElementById('three-canvas');
    this.handCounter = document.getElementById('hand-counter');
    this.roundIndicator = document.getElementById('round-indicator');
    this.connBadge = document.getElementById('conn-badge');
    this.potValue = document.getElementById('pot-value');
    this.playerRows = [
      document.querySelector('.player-row-0'),
      document.querySelector('.player-row-1'),
    ];
    this.stackNames = [
      document.getElementById('stack-name-0'),
      document.getElementById('stack-name-1'),
    ];
    this.stackValues = [
      document.getElementById('stack-value-0'),
      document.getElementById('stack-value-1'),
    ];
    this.dealerBadges = [
      document.getElementById('dealer-badge-0'),
      document.getElementById('dealer-badge-1'),
    ];
    this.holeRows = [
      document.getElementById('hole-cards-0'),
      document.getElementById('hole-cards-1'),
    ];
    this.communityRow = document.getElementById('community-row');
    this.gameOverEl = document.getElementById('game-over-overlay');
    this.winnerTitle = document.getElementById('winner-title');
    this.winnerName = document.getElementById('winner-name');
    this.winnerReason = document.getElementById('winner-reason');
    this.connStatus = document.getElementById('connection-status');

    this.renderer = null;
    this.state = null;
  }

  async init() {
    if (!this.matchId) {
      this._showError('Missing matchId in URL.');
      return;
    }
    this.state = new PokerState(this.matchId);

    try {
      await this.state.loadInitialState();
    } catch (err) {
      console.error('[PokerViewer] Failed to load initial state:', err);
      this._showError('Failed to connect to match');
      return;
    }

    this.renderer = new PokerRenderer(this.canvas);

    this.state.onConnectionChange = (s) => this._setConnectionStatus(s);
    this.state.onHandSnapshot = (snapshot) => this._renderSnapshot(snapshot);
    this.state.onMatchOver = (winner, profits) => this._handleMatchOver(winner, profits);

    await this.renderer.charactersLoaded;

    // Set player labels from name map (or fallback) once characters loaded.
    if (this.stackNames[0]) this.stackNames[0].textContent = this.state.displayName(0);
    if (this.stackNames[1]) this.stackNames[1].textContent = this.state.displayName(1);

    // Render initial snapshot
    this._renderSnapshot({
      handNumber: this.state.handNumber,
      maxHands: this.state.maxHands,
      profits: this.state.profits,
      phase: this.state.phase,
      button: this.state.button,
      currentHand: this.state.currentHand,
    });

    if (this.state.isTerminal()) {
      let winner = null;
      if (this.state.profits[0] > this.state.profits[1]) winner = 0;
      else if (this.state.profits[1] > this.state.profits[0]) winner = 1;
      this._handleMatchOver(winner, this.state.profits);
    }

    const tick = () => {
      this.renderer.update();
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);

    this.state.connect();
  }

  _renderSnapshot(snapshot) {
    const { handNumber, maxHands, profits, currentHand, button } = snapshot;

    // Header — hand counter + round indicator
    if (this.handCounter) {
      this.handCounter.textContent = maxHands > 0
        ? `Hand ${handNumber}/${maxHands}`
        : `Hand ${handNumber}`;
    }
    if (this.roundIndicator) {
      if (currentHand && !currentHand.finished) {
        const roundKey = currentHand.round?.toLowerCase?.() ?? '';
        this.roundIndicator.textContent = ROUND_LABELS[roundKey] ?? roundKey.toUpperCase();
      } else {
        this.roundIndicator.textContent = '';
      }
    }

    // 2D Game Table panel — pot + per-player rows + readable card faces
    if (this.potValue) {
      this.potValue.textContent = currentHand?.pot ?? 0;
    }

    const stacks = currentHand?.stacks ?? [null, null];
    const folded = currentHand?.folded ?? [false, false];
    const holeCards = currentHand?.hole_cards ?? [[], []];
    const actionOn = currentHand?.action_on;

    for (let i = 0; i < 2; i += 1) {
      if (this.stackValues[i]) {
        this.stackValues[i].textContent = stacks[i] ?? '–';
      }
      if (this.dealerBadges[i]) {
        this.dealerBadges[i].classList.toggle('active', button === i);
      }
      if (this.playerRows[i]) {
        this.playerRows[i].classList.toggle(
          'acting',
          actionOn === i && !currentHand?.finished,
        );
      }
      if (this.holeRows[i]) {
        this.holeRows[i].innerHTML = '';
        const cards = holeCards[i] ?? [];
        for (let c = 0; c < HOLE_CARDS_PER_PLAYER; c += 1) {
          if (cards[c]) {
            this.holeRows[i].appendChild(
              buildCardFace(cards[c], { faceDown: folded[i] }),
            );
          } else {
            this.holeRows[i].appendChild(buildCardFace(null, { empty: true }));
          }
        }
      }
    }

    if (this.communityRow) {
      this.communityRow.innerHTML = '';
      const community = currentHand?.community ?? [];
      for (let i = 0; i < COMMUNITY_SLOTS; i += 1) {
        if (community[i]) {
          this.communityRow.appendChild(buildCardFace(community[i]));
        } else {
          this.communityRow.appendChild(buildCardFace(null, { empty: true }));
        }
      }
    }

    // 3D scene reconciliation (in lockstep with the 2D panel)
    this.renderer.syncHand({ currentHand, button });
  }

  _handleMatchOver(winner, profits) {
    if (!this.gameOverEl) return;
    this.gameOverEl.classList.remove('hidden', 'p1-wins', 'p2-wins');

    if (winner === 0) this.gameOverEl.classList.add('p1-wins');
    else if (winner === 1) this.gameOverEl.classList.add('p2-wins');

    if (this.winnerTitle) {
      this.winnerTitle.textContent = winner === null ? 'TIE' : 'VICTORY';
    }
    if (this.winnerName) {
      if (winner === null) {
        this.winnerName.textContent = 'Profits tied';
      } else {
        const margin = Math.abs(profits[0] - profits[1]);
        this.winnerName.textContent = `${this.state.displayName(winner)} (+${margin})`;
      }
    }
    if (this.winnerReason) {
      this.winnerReason.textContent = `Final ${profits[0]} : ${profits[1]}`;
    }
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

const viewer = new PokerViewer();
viewer.init().catch((err) => {
  console.error('[PokerViewer] init failed:', err);
});
