<script>
  // 16x16 LED matrix. pixels is 256 colors; onpaint(index) fires as the user paints or drags.
  let { pixels, onpaint = null, label = '16 by 16 screen' } = $props();
  let painting = false;

  // How brightly a pixel is lit, 0 to 1. Dim pixels bleed less light, like a real panel.
  function level(color) {
    const n = parseInt(color.slice(1), 16);
    return Math.max(n >> 16, (n >> 8) & 255, n & 255) / 255;
  }
  let last = -1;

  function paint(event) {
    const index = document.elementFromPoint(event.clientX, event.clientY)?.dataset?.i;
    if (index === undefined || Number(index) === last) return;
    last = Number(index);
    onpaint(last);
  }

  function start(event) {
    if (!onpaint) return;
    painting = true;
    last = -1;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    paint(event);
  }
</script>

<div
  class="leds"
  class:editable={!!onpaint}
  role="img"
  aria-label={label}
  onpointerdown={start}
  onpointermove={(e) => painting && paint(e)}
  onpointerup={() => (painting = false)}
  onpointercancel={() => (painting = false)}
>
  {#each pixels as color, i (i)}
    <span data-i={i} class:lit={color !== '#000000'} style:--c={color} style:--l={level(color)}></span>
  {/each}
</div>

<style>
  /* A lit LCD panel: square pixels, a bright core, light bleeding onto the dark glass around it.
     Sizes are in cqw (percent of the grid's width), so the look holds from previews to the big screen. */
  .leds {
    position: relative;
    display: grid;
    grid-template-columns: repeat(16, 1fr);
    gap: clamp(1px, 0.55cqw, 3px);
    container-type: inline-size;
    width: 100%;
    aspect-ratio: 1;
    touch-action: none;
    user-select: none;
    isolation: isolate;
  }
  span {
    border-radius: 0.6cqw;
    background: #10131b;
    box-shadow: inset 0 0 0 0.25cqw #ffffff05;
  }
  span.lit {
    position: relative;
    z-index: 1;           /* the glow spills over unlit neighbors */
    --hot: calc(var(--l) * var(--l));   /* squared, so only bright pixels really bloom */
    background:
      radial-gradient(ellipse at 50% 40%, color-mix(in srgb, #fff calc(var(--hot) * 45%), var(--c)) 0 22%, transparent 70%),
      var(--c);
    box-shadow:
      0 0 1.2cqw color-mix(in srgb, var(--c) calc(var(--hot) * 90%), transparent),
      0 0 3.6cqw color-mix(in srgb, var(--c) calc(var(--hot) * 45%), transparent);
  }
  /* glass on top: faint scanlines and a soft reflection */
  .leds::after {
    content: '';
    position: absolute;
    inset: 0;
    z-index: 2;
    pointer-events: none;
    background:
      linear-gradient(160deg, #ffffff12 0%, transparent 38%),
      repeating-linear-gradient(0deg, #00000026 0 0.35cqw, transparent 0.35cqw 0.9cqw);
    mix-blend-mode: screen;
  }
  .editable { cursor: crosshair; }
  .editable span:hover { outline: 1px solid #ffffff66; outline-offset: -1px; }
</style>
