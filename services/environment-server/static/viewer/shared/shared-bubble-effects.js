/**
 * Shared Bubble Effects
 *
 * Typing animation and hover-to-persist behavior for reasoning bubbles.
 * Used by both Coup and Vibe Check viewers.
 */

/**
 * Reveal text character-by-character inside an element.
 *
 * @param {HTMLElement} element - The element to type into (its textContent is replaced).
 * @param {string} fullText - The complete text to reveal.
 * @param {number} [charDelayMs=40] - Milliseconds between characters.
 * @returns {{ promise: Promise<void>, cancel: () => void }}
 *   - `promise` resolves when typing finishes naturally.
 *   - `cancel()` snaps the element to the full text immediately.
 */
export function typeText(element, fullText, charDelayMs = 40) {
  let index = 0;
  let cancelled = false;
  let intervalId = null;

  const promise = new Promise((resolve) => {
    if (!fullText || fullText.length === 0) {
      element.textContent = '';
      resolve();
      return;
    }

    element.textContent = '';

    intervalId = setInterval(() => {
      if (cancelled) {
        clearInterval(intervalId);
        resolve();
        return;
      }

      index++;
      element.textContent = fullText.slice(0, index);

      if (index >= fullText.length) {
        clearInterval(intervalId);
        intervalId = null;
        resolve();
      }
    }, charDelayMs);
  });

  const cancel = () => {
    if (cancelled) return;
    cancelled = true;
    if (intervalId) {
      clearInterval(intervalId);
      intervalId = null;
    }
    element.textContent = fullText;
  };

  return { promise, cancel };
}

/**
 * Wire hover-to-persist behavior on a reasoning bubble element.
 *
 * On mouseenter: clears the auto-hide timer so the bubble stays visible.
 * On mouseleave: restarts a shorter 2s timer before fading out.
 *
 * @param {HTMLElement} bubbleEl - The `.reasoning-bubble` DOM element.
 * @param {object} state - Mutable state object with `hideTimer` and `element` fields.
 * @param {function} startHideTimer - Called with a delay (ms) to schedule the fade-out.
 */
export function setupBubbleHover(bubbleEl, state, startHideTimer) {
  bubbleEl.addEventListener('mouseenter', () => {
    if (state.hideTimer) {
      clearTimeout(state.hideTimer);
      state.hideTimer = null;
    }
    // Ensure bubble stays visible while hovered
    bubbleEl.classList.remove('fade-out');
    bubbleEl.classList.add('visible');
  });

  bubbleEl.addEventListener('mouseleave', () => {
    // Restart a shorter 2s timer on mouse leave
    startHideTimer(2000);
  });
}
