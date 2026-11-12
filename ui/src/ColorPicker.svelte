<script>
  // Preset swatches plus a "Custom" swatch that opens an in-page picker (hue, saturation, brightness, hex).
  // The picker is our own so it looks the same on Windows, macOS, Linux and phones.
  // value is bindable and updates while dragging. onpick(color) fires once a choice is made.
  let { value = $bindable(), swatches, onpick = () => {}, label = 'Color' } = $props();
  let open = $state(false);
  let h = $state(0);
  let s = $state(100);
  let v = $state(100);
  let hexText = $state('');
  let custom = $derived(!swatches.includes(value));

  function toHex(hue, sat, val) {
    const f = (n) => {
      const k = (n + hue / 60) % 6;
      const c = (val / 100) * (1 - (sat / 100) * Math.max(0, Math.min(k, 4 - k, 1)));
      return Math.round(c * 255).toString(16).padStart(2, '0');
    };
    return `#${f(5)}${f(3)}${f(1)}`;
  }

  function fromHex(hex) {
    const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
    const max = Math.max(r, g, b);
    const d = max - Math.min(r, g, b);
    let hue = 0;
    if (d) {
      if (max === r) hue = ((g - b) / d) % 6;
      else if (max === g) hue = (b - r) / d + 2;
      else hue = (r - g) / d + 4;
    }
    return [Math.round((hue * 60 + 360) % 360), Math.round(max ? (d / max) * 100 : 0), Math.round(max * 100)];
  }

  function pick(color) {
    value = color;
    onpick(color);
  }

  function toggle() {
    open = !open;
    if (open) {
      [h, s, v] = fromHex(value);
      hexText = value;
    }
  }

  function slide() {
    value = toHex(h, s, v);
    hexText = value;
  }

  function typeHex() {
    const text = hexText.trim().replace(/^#?/, '#').toLowerCase();
    if (!/^#[0-9a-f]{6}$/.test(text)) return;
    [h, s, v] = fromHex(text);
    pick(text);
  }
</script>

<div class="picker" role="group" aria-label={label}>
  <div class="swatches">
    {#each swatches as color}
      <button
        type="button" class="swatch" class:on={value === color} style:background={color}
        aria-label="{label} {color}" aria-pressed={value === color} onclick={() => { open = false; pick(color); }}
      ></button>
    {/each}
    <button type="button" class="custom" class:on={custom} aria-expanded={open} onclick={toggle}>
      <span class="swatch wheel" style:--picked={custom ? value : 'transparent'}></span>
      Custom
    </button>
  </div>

  {#if open}
    <div class="panel">
      <div class="now">
        <span class="chip" style:background={value}></span>
        <label>Hex
          <input class="hex" bind:value={hexText} maxlength="7" spellcheck="false" aria-label="{label} hex code"
            onchange={typeHex} />
        </label>
      </div>
      <label>Hue
        <input type="range" min="0" max="359" bind:value={h} oninput={slide} onchange={() => onpick(value)}
          style:--track="linear-gradient(90deg, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00)" />
      </label>
      <label>Saturation
        <input type="range" min="0" max="100" bind:value={s} oninput={slide} onchange={() => onpick(value)}
          style:--track="linear-gradient(90deg, {toHex(h, 0, v)}, {toHex(h, 100, v)})" />
      </label>
      <label>Brightness
        <input type="range" min="0" max="100" bind:value={v} oninput={slide} onchange={() => onpick(value)}
          style:--track="linear-gradient(90deg, #000, {toHex(h, s, 100)})" />
      </label>
      <button type="button" class="done" onclick={() => (open = false)}>Done</button>
    </div>
  {/if}
</div>

<style>
  .picker { display: flex; flex-direction: column; gap: 14px; width: 100%; max-width: 420px; }
  .swatches { display: flex; flex-wrap: wrap; gap: 10px; align-items: center; }
  .swatch {
    display: block; width: 40px; height: 40px; padding: 0; border-radius: 50%; cursor: pointer;
    border: 2px solid #fff; box-shadow: 0 0 0 1px var(--line); box-sizing: border-box; flex: none;
  }
  .swatch.on, .custom.on .wheel { box-shadow: 0 0 0 3px var(--ink); }
  .custom {
    display: inline-flex; align-items: center; gap: 8px; padding: 0 12px 0 0; border: 0; background: none;
    font: 500 15px 'Bai Jamjuree', sans-serif; color: var(--ink); cursor: pointer; white-space: nowrap; border-radius: 999px;
  }
  .wheel {
    background:
      radial-gradient(circle, var(--picked) 0 38%, transparent 40%),
      conic-gradient(#f0443a, #ffb800, #2fb574, #22c3e6, #2f6bff, #9b6bff, #ff5fa2, #f0443a);
  }
  :is(.swatch, .custom, .done, input):focus-visible { outline: 3px solid #ffb800; outline-offset: 3px; }

  .panel {
    display: flex; flex-direction: column; gap: 14px; padding: 16px; box-sizing: border-box;
    background: var(--panel); border: 0; box-shadow: var(--ring); border-radius: 14px;
  }
  .panel label { display: flex; flex-direction: column; gap: 6px; font-size: 14px; font-weight: 500; color: var(--soft); }
  .now { display: flex; align-items: flex-end; gap: 12px; }
  .chip { width: 56px; height: 56px; border-radius: 12px; box-shadow: inset 0 0 0 1px #161c2c26; flex: none; }
  .hex {
    width: 110px; padding: 9px 12px; border: 0; border-radius: 10px; background: #fff; box-shadow: var(--ring);
    font: 500 15px ui-monospace, Consolas, monospace; color: var(--ink);
  }
  input[type='range'] {
    appearance: none; -webkit-appearance: none; width: 100%; height: 16px; margin: 0; padding: 0;
    border: 1px solid var(--line); border-radius: 999px; background: var(--track); cursor: pointer;
  }
  input[type='range']::-webkit-slider-thumb {
    -webkit-appearance: none; width: 24px; height: 24px; border-radius: 50%;
    background: #fff; border: 3px solid var(--ink); box-shadow: 0 1px 3px #0005;
  }
  input[type='range']::-moz-range-thumb {
    width: 18px; height: 18px; border-radius: 50%; background: #fff; border: 3px solid var(--ink);
  }
  .done {
    align-self: flex-start; padding: 9px 19px; border-radius: 10px; border: 0; box-shadow: var(--ring), var(--lift);
    background: #fff; font: 500 15px 'Bai Jamjuree', sans-serif; color: var(--ink); cursor: pointer;
  }
  .done:hover { box-shadow: var(--ring-hover), 0 2px 6px #161c2c1f; }
</style>
