<script>
  // A listbox select that looks the same on every platform.
  // Keyboard: Enter, Space or Arrow Down opens; arrows, Home and End move; Enter picks; Escape or Tab closes;
  // typing a letter jumps to the next option starting with it.
  let { value = $bindable(), options, label, onchange = () => {} } = $props();   // options: [[value, text], ...]
  let open = $state(false);
  let active = $state(0);
  let root;
  let list;
  const id = `select-${Math.random().toString(36).slice(2, 8)}`;
  let current = $derived(options.find(([v]) => v === value) ?? options[0]);

  function show() {
    open = true;
    active = Math.max(0, options.findIndex(([v]) => v === value));
    queueMicrotask(() => list?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' }));
  }

  function pick(index) {
    const [next] = options[index];
    open = false;
    root.querySelector('button').focus();
    if (next !== value) {
      value = next;
      onchange(next);
    }
  }

  function key(event) {
    const last = options.length - 1;
    if (!open) {
      if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(event.key)) {
        event.preventDefault();
        show();
      }
      return;
    }
    const moves = { ArrowDown: active + 1, ArrowUp: active - 1, Home: 0, End: last, PageDown: last, PageUp: 0 };
    if (event.key in moves) {
      event.preventDefault();
      active = Math.min(last, Math.max(0, moves[event.key]));
      list?.children[active]?.scrollIntoView({ block: 'nearest' });
    } else if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      pick(active);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      open = false;
    } else if (event.key === 'Tab') {
      open = false;
    } else if (event.key.length === 1) {
      const letter = event.key.toLowerCase();
      const order = options.map((_, i) => (active + 1 + i) % options.length);
      const hit = order.find((i) => options[i][1].toLowerCase().startsWith(letter));
      if (hit !== undefined) active = hit;
    }
  }

  $effect(() => {
    if (!open) return;
    const away = (event) => !root.contains(event.target) && (open = false);
    document.addEventListener('pointerdown', away);
    return () => document.removeEventListener('pointerdown', away);
  });
</script>

<div class="select" bind:this={root}>
  <button
    type="button" class="trigger" class:open aria-haspopup="listbox" aria-expanded={open} aria-controls={id}
    aria-label="{label}: {current[1]}" aria-activedescendant={open ? `${id}-${active}` : undefined}
    onclick={() => (open ? (open = false) : show())} onkeydown={key}
  >
    <span>{current[1]}</span>
    <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
      <path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" />
    </svg>
  </button>
  {#if open}
    <ul role="listbox" {id} aria-label={label} bind:this={list}>
      {#each options as [optionValue, text], i}
        <li
          id="{id}-{i}" role="option" aria-selected={optionValue === value} class:active={i === active}
          onpointermove={() => (active = i)} onpointerdown={(e) => e.preventDefault()} onclick={() => pick(i)}
        >
          {text}
          {#if optionValue === value}
            <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
              <path d="M3.5 8.5l3 3 6-7" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .select { position: relative; display: inline-block; }
  .trigger {
    display: inline-flex; align-items: center; justify-content: space-between; gap: 10px;
    min-width: 128px; padding: 9px 11px 9px 15px; text-align: left; cursor: pointer; box-sizing: border-box;
    font: 500 15px 'Bai Jamjuree', sans-serif; color: var(--ink); background: var(--panel);
    border: 0; border-radius: 10px; box-shadow: var(--ring), var(--lift);
    transition: background-color 120ms ease, box-shadow 120ms ease;
  }
  .trigger:hover, .trigger.open { background: #fff; box-shadow: var(--ring-hover), 0 2px 6px #161c2c1f; }
  .trigger:focus-visible { outline: 3px solid #ffb800; outline-offset: 2px; }
  .trigger svg { flex: none; color: var(--soft); transition: transform 120ms ease; }
  .trigger.open svg { transform: rotate(180deg); }
  ul {
    position: absolute; z-index: 20; top: calc(100% + 6px); left: 0; min-width: 100%;
    margin: 0; padding: 6px; list-style: none; box-sizing: border-box; max-height: 260px; overflow: auto;
    background: #fff; border-radius: 12px;
    box-shadow: 0 0 0 1px var(--line), 0 12px 28px -8px #161c2c40, 0 2px 6px #161c2c14;
  }
  li {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    padding: 8px 10px; border-radius: 8px; cursor: pointer; white-space: nowrap;
    font: 500 15px 'Bai Jamjuree', sans-serif; color: var(--ink);
  }
  li.active { background: #e8edf7; }
  li[aria-selected='true'] { color: var(--blue); }
  @media (prefers-reduced-motion: reduce) { .trigger, .trigger svg { transition: none; } }
</style>
