<script>
  import '@fontsource/chakra-petch/500.css';
  import '@fontsource/chakra-petch/600.css';
  import '@fontsource/bai-jamjuree/400.css';
  import '@fontsource/bai-jamjuree/500.css';
  import '@fontsource/bai-jamjuree/600.css';
  import PixelGrid from './PixelGrid.svelte';
  import ColorPicker from './ColorPicker.svelte';
  import Select from './Select.svelte';
  import { demoCall, demoNotifications } from './demo.js';
  import { frameAt } from './timeline.js';

  // Inside the desktop app the page comes from the app itself (tauri://localhost on macOS,
  // http://tauri.localhost on Windows) and calls the app's local server directly. Opened in a
  // browser at localhost:8765 it is served by that same server.
  const IN_APP = location.protocol === 'tauri:' || location.hostname === 'tauri.localhost';
  const HOSTED = !IN_APP && !['localhost', '127.0.0.1', '[::1]'].includes(location.hostname);
  const API = HOSTED || IN_APP ? 'http://127.0.0.1:8765' : '';
  let demo = $state(false);

  async function request(path, options) {
    if (demo) {
      const body = typeof options?.body === 'string' ? JSON.parse(options.body) : {};
      return demoCall(path, body);
    }
    return (await fetch(API + path, options)).json();
  }

  const BLANK = Array(256).fill('#000000');
  const TABS = [
    ['usage', 'AI usage'], ['weather', 'Weather'], ['system', 'Computer'], ['timer', 'Timer'], ['notify', 'Notifications'], ['clock', 'Clock'], ['light', 'Light'],
    ['draw', 'Draw'], ['score', 'Score'],
  ];
  const CLOCKS = ['Full screen', 'Rainbow', 'Boxed', 'Square analog', 'Inverted', 'Round analog'];
  const WEATHER = [[1, 'Clear'], [3, 'Cloudy'], [5, 'Storm'], [6, 'Rain'], [8, 'Snow'], [9, 'Fog']];
  const SWATCHES = ['#ffffff', '#ffb800', '#f0443a', '#ff5fa2', '#9b6bff', '#2f6bff', '#22c3e6', '#2fb574'];
  const PLAYBACK = [
    ['device', 'On the device', 'Frames are sent once and the device loops them.'],
    ['host', 'From this computer', 'One frame is sent per step. Use this if notifications do not blink.'],
  ];
  const SHOWING = {
    system: 'this computer',
    usage: 'AI usage', timer: 'the timer', clock: 'the clock', light: 'a solid color', off: 'off',
    vj: 'an effect', score: 'the score', pixels: 'your drawing', image: 'your image',
    cloud: 'the Cloud channel', custom: 'the Custom channel', visualizer: 'the visualizer', 'clock-weather': 'the clock',
    weather: 'the weather',
  };
  const NO_PREVIEW = ['clock', 'vj', 'score', 'image', 'cloud', 'custom', 'visualizer', 'clock-weather'];
  const CONDITION = {
    sunny: 'clear', night: 'clear', 'partly-cloudy': 'partly cloudy', cloudy: 'cloudy', rain: 'rain',
    storm: 'a thunderstorm', fog: 'fog', snow: 'snow',
  };

  let status = $state({ mode: 'manual', port: '', usage: null, faces: null, notification: null,
    timer_left: null, showing: null, frame_ms: 4000, error: null, accounts: {}, playback: 'device' });
  let polledAt = $state(Date.now());   // when status arrived, to keep previews in step with the device
  let now = $state(Date.now());
  let catalog = $state([]);            // every notification, with frames for the previews
  let catalogMissing = $state(false);  // the server is older than this page
  const still = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  function setStatus(next) {
    status = next;
    polledAt = Date.now();
  }
  let online = $state(true);
  let toast = $state('');
  let busy = $state(false);
  let tab = $state('usage');
  let timerKind = $state('pomodoro');   // which timer the Timer tab shows

  let brightness = $state(60);
  let clockStyle = $state(0);
  let clockColor = $state('#ffffff');
  let temp = $state(30);
  let weather = $state(1);
  let lightColor = $state('#ffb800');
  let red = $state(0);
  let blue = $state(0);
  let minutes = $state(25);
  let canvas = $state([...BLANK]);
  let brush = $state('#ffb800');
  let raw = $state('');
  let claudeToken = $state('');
  let codexToken = $state('');
  let codexAccount = $state('');
  let city = $state('');
  let lat = $state('');
  let lon = $state('');
  const weatherSettings = (body, done) => post('/api/weather/settings', body, done);
  async function findCity() {
    await weatherSettings({ city }, 'Location saved');
    if (!status.error) city = '';
  }
  // The browser's own location (GPS or Wi-Fi), more precise than the automatic one. Asks permission first.
  // Why the browser gave no location, by GeolocationPositionError code, with what to do about it.
  const LOCATION_PROBLEMS = {
    1: 'The browser was not allowed to share your location. Allow location for this page in the address bar, then try again.',
    2: 'The browser could not find a location. On Windows, turn on Location services and "Let desktop apps access your location" in Settings, Privacy & security, Location. On macOS, allow your browser in System Settings, Privacy & Security, Location Services.',
    3: 'Finding the location took too long. Try again, or type a city instead.',
  };
  let locationProblem = $state('');
  function useDeviceLocation() {
    locationProblem = '';
    if (!navigator.geolocation) {
      locationProblem = 'This browser cannot share its location. Type a city instead.';
      return;
    }
    busy = true;
    navigator.geolocation.getCurrentPosition(
      (pos) => {
        busy = false;
        weatherSettings({ latitude: pos.coords.latitude.toFixed(4), longitude: pos.coords.longitude.toFixed(4),
          place: "This device's location" }, 'Location saved');
      },
      (err) => {
        busy = false;
        const sys = status.weather?.system_location;
        locationProblem = err.code === 2 && sys?.checked && !sys.off.length
          ? 'Windows location is on, but no location was found. Windows finds it from nearby Wi-Fi, so a computer on a cable only may have none. Type a city instead.'
          : (LOCATION_PROBLEMS[err.code] ?? LOCATION_PROBLEMS[2]);
      },
      { timeout: 20000, maximumAge: 3600000 },
    );
  }
  function ago(fetchedAt) {
    const seconds = Math.max(0, Math.round(now / 1000 - fetchedAt));
    return seconds < 60 ? 'just now' : `${left(seconds)} ago`;
  }
  const SOURCES = {
    manual: 'Using what you entered', file: 'Found on this computer', keychain: 'Read from the macOS Keychain',
  };
  async function saveToken(body, done) {
    await post('/api/tokens', body, done);
    claudeToken = codexToken = codexAccount = '';
  }
  // Open by itself only when no account is found, decided once, so a status refresh never closes it under you
  let accountsOpen = $state(false);
  let accountsDecided = false;
  $effect(() => {
    if (accountsDecided || !status.accounts || !('claude' in status.accounts)) return;   // wait for the server's answer
    accountsDecided = true;
    accountsOpen = !status.accounts.claude && !status.accounts.codex;
  });
  let hooks = $state(null);       // whether Claude Code hooks are installed, from GET /api/hooks
  async function loadHooks() {
    try {
      hooks = demo ? null : await request('/api/hooks');
    } catch {
      hooks = null;
    }
  }
  async function setHooks(install) {
    await post('/api/hooks', { install }, install ? 'Claude Code connected' : 'Claude Code disconnected');
    await loadHooks();
  }
  let linkOpen = $state(false);
  let ports = $state({ ports: [], setting: 'auto', help: '' });
  let choice = $state('auto');
  let other = $state('');

  async function openLink() {
    linkOpen = !linkOpen;
    if (!linkOpen) return;
    try {
      ports = await request('/api/ports');
      const known = ports.setting === 'auto' || ports.ports.some((p) => p.id === ports.setting);
      choice = known ? ports.setting : 'other';
      other = known ? '' : ports.setting;
    } catch {
      online = false;
    }
  }
  async function connect() {
    await post('/api/port', { port: choice === 'other' ? other.trim() : choice }, 'Connected');
    if (!status.error) linkOpen = false;
  }

  async function loadCatalog() {
    try {
      const events = demo ? demoNotifications() : (await request('/api/notifications')).events;
      catalog = Array.isArray(events) ? events : [];
      catalogMissing = !Array.isArray(events);
    } catch {
      catalog = [];
    }
  }

  // What the display shows now: the notification's timeline if there is one, else the base faces.
  let screen = $derived.by(() => {
    if (status.notification_frames?.length) {
      const elapsed = status.notification_elapsed_ms + (now - polledAt);
      const i = still ? 0 : frameAt(status.notification_ms, status.notification_loop, elapsed);
      return status.notification_frames[i];
    }
    const faces = status.faces;
    if (!faces?.length) return BLANK;
    return faces[still ? 0 : Math.floor(now / (status.frame_ms || 4000)) % faces.length];
  });
  // Card previews replay every timeline from page load, looping so each behavior keeps showing.
  const pageStart = Date.now();
  const previewFrame = (n) => n.frames[still ? 0 : frameAt(n.ms, true, now - pageStart)];
  const dismiss = () => post('/api/notify', { event: 'dismiss' }, 'Notification dismissed');

  const PHASE = { focus: 'Focus', short: 'Short break', long: 'Long break' };
  const clockText = (seconds) => `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
  let pomo = $derived(status.pomodoro);
  let pomoRunning = $derived(pomo?.status === 'running');   // a boolean, so the poll restarts only when it flips
  let pomoForm = $state(null);         // the settings form, filled from the server once
  $effect(() => {
    if (pomo?.settings && !pomoForm) pomoForm = { ...pomo.settings };
  });
  const pomodoro = (action, done) => post('/api/pomodoro', { action }, done);

  let headline = $derived(
    !online ? "Can't reach Sideglance"
      : status.notification ? status.notification_label
      : pomo?.active ? `${PHASE[pomo.phase]}, ${clockText(pomo.seconds_left)}`
      : status.showing ? (status.showing === 'off' ? 'Screen is off' : `Showing ${SHOWING[status.showing] ?? status.showing}`)
      : 'Nothing sent to the screen yet');
  let noPreview = $derived(online && !status.notification && !pomo?.active && NO_PREVIEW.includes(status.showing));

  let toastTimer;
  function say(text) {
    toast = text;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ''), 2600);
  }

  async function call(path, options, done) {
    busy = true;
    try {
      if (demo && path === '/api/image') return say('Uploads need a real device');
      if (demo && path.startsWith('/api/pomodoro')) return say('The Pomodoro timer needs the Sideglance app');
      if (demo && path === '/api/weather/settings' && !('unit' in JSON.parse(options.body)) && !('in_rotation' in JSON.parse(options.body))) {
        return say('Looking up a place needs the Sideglance app');
      }
      const data = await request(path, options);
      online = true;
      if (data.mode) setStatus(data);
      if (!data.error && done) say(done);
    } catch {
      online = false;
    } finally {
      busy = false;
    }
  }
  const post = (path, body, done) => call(path, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }, done);
  const command = (done, cmd, ...args) => post('/api/command', { cmd, args }, done);
  const hex = (color) => color.replace('#', '');

  // Updates: the app checks by itself every few hours; these are for the page's own buttons.
  let update = $derived(status.update);
  let checkedNow = $state(false);
  async function checkUpdate() {
    await post('/api/update', { action: 'check' });
    checkedNow = true;
  }
  const installUpdate = () => post('/api/update', { action: 'install' });

  async function refresh() {
    if (demo) {
      setStatus(demoCall('/api/status', {}));
      // keep looking for a real server, and leave the demo as soon as one answers
      if (Date.now() - lastProbe > 5000) {
        lastProbe = Date.now();
        fetch(API + '/api/status').then((r) => r.json()).then((s) => { demo = false; setStatus(s); loadCatalog(); }).catch(() => {});
      }
      return;
    }
    try {
      setStatus(await request('/api/status'));
      if (!catalog.length) loadCatalog();
      if (hooks === null) loadHooks();
      online = true;
    } catch {
      if (HOSTED) {
        demo = true;
        online = true;
        lastProbe = Date.now();
        setStatus(demoCall('/api/status', {}));
        loadCatalog();
      } else {
        online = false;
      }
    }
  }
  let lastProbe = 0;

  function upload(file) {
    call('/api/image', { method: 'POST', body: file }, 'Image shown');
  }

  function setScore(side, delta) {
    if (side === 'red') red = Math.max(0, Math.min(999, red + delta));
    else blue = Math.max(0, Math.min(999, blue + delta));
    command('', 'score', red, blue);
  }

  function left(seconds) {
    if (seconds == null) return 'an unknown time';
    if (seconds >= 86400) return plural(Math.floor(seconds / 86400), 'day');
    if (seconds >= 3600) return plural(Math.floor(seconds / 3600), 'hour');
    return plural(Math.max(1, Math.ceil(seconds / 60)), 'minute');
  }
  const plural = (n, word) => `${n} ${word}${n === 1 ? '' : 's'}`;
  const level = (pct) => (pct >= 90 ? 'high' : pct >= 70 ? 'mid' : 'low');

  $effect(() => {
    // every second while a Pomodoro counts down, so the preview's seconds ring keeps moving
    const every = pomoRunning ? 1000 : 2000;
    refresh();
    const poll = setInterval(refresh, every);
    return () => clearInterval(poll);
  });
  $effect(() => {
    if (still) return;
    const clock = setInterval(() => (now = Date.now()), 100);
    return () => clearInterval(clock);
  });
  // On narrow screens the tab row scrolls sideways: keep the selected tab in view.
  $effect(() => {
    tab;
    queueMicrotask(() => document.querySelector('.tabs button.on')?.scrollIntoView({ inline: 'center', block: 'nearest' }));
  });
  // Follow the device to its tab only when the mode changes, so the user can still browse other tabs.
  let lastMode = '';
  $effect(() => {
    if (status.mode === lastMode) return;
    lastMode = status.mode;
    if (['usage', 'timer', 'weather', 'system'].includes(status.mode)) tab = status.mode;
    if (status.mode === 'timer') timerKind = 'countdown';
  });
  let lastPomodoro = false;
  let pomoActive = $derived(!!pomo?.active);
  $effect(() => {
    if (pomoActive && !lastPomodoro) {
      tab = 'timer';
      timerKind = 'pomodoro';
    }
    lastPomodoro = pomoActive;
  });
</script>

<div class="page">
  {#if IN_APP && online && status.app !== 'sideglance-app'}
    <p class="banner">
      Another program is using port 8765, probably the Python version of Sideglance, so this app cannot
      start its own server. Stop it (in the Python folder, run <code>python autostart.py uninstall</code>),
      then quit Sideglance from the tray or menu bar and open it again.
    </p>
  {/if}
  {#if update?.available}
    <div class="banner update" role="status">
      {#if update.busy === 'installing'}
        <p>Installing Sideglance {update.available.version}. The app restarts by itself when it is done.</p>
      {:else}
        <p>Sideglance {update.available.version} is ready. You have {status.version}.
          <a href={update.available.url} target="_blank" rel="noreferrer">What changed</a></p>
        <button class="go" disabled={busy || !!update.busy} onclick={installUpdate}>Update and restart</button>
      {/if}
      {#if update.error && update.busy !== 'installing'}
        <p class="bad">{update.error}. You can <a href={update.available.url} target="_blank" rel="noreferrer">download it from GitHub</a> and install it over this one.</p>
      {/if}
    </div>
  {/if}
  {#if demo}
    <p class="banner">
      This is a demo. Nothing is sent to a real device. Install the Sideglance app to control your own display.
    </p>
  {/if}
  <aside class="device">
    <p class="brand">Sideglance</p>
    <div class="shell">
      <div class="glass" class:dim={noPreview}>
        <PixelGrid pixels={noPreview ? BLANK : screen} label="What the display shows now" />
        {#if noPreview}<p class="glass-note">Preview not available.<br />Look at the device.</p>{/if}
      </div>
      <label class="knob">
        <span>Brightness {brightness}%</span>
        <input type="range" min="0" max="100" bind:value={brightness} style:--fill="{brightness}%"
          onchange={() => command('', 'brightness', brightness)} />
      </label>
    </div>

    <h1>{headline}</h1>
    {#if !online}
      <p class="help">Open the Sideglance app. This page reconnects by itself.</p>
    {:else if status.error}
      <p class="help bad">{status.error}</p>
    {:else if status.notification}
      <button class="quiet" onclick={dismiss}>Dismiss notification</button>
    {:else if demo}
      <p class="help">Demo display</p>
    {:else if status.port}
      <p class="help">Connected on {status.port}{status.port_setting === 'auto' ? ', found automatically' : ''}</p>
    {:else}
      <p class="help">Looking for the display</p>
    {/if}
    {#if online && !demo && HOSTED}
      <p class="help">Open <a href="http://localhost:8765">localhost:8765</a> to change the connection or tokens.</p>
    {/if}
    {#if online && !HOSTED}
      <button class="quiet" aria-expanded={linkOpen} onclick={openLink}>Change connection</button>
      {#if linkOpen}
        <div class="link">
          <label><input type="radio" bind:group={choice} value="auto" /> Find it automatically</label>
          {#each ports.ports as p}
            <label><input type="radio" bind:group={choice} value={p.id} /> {p.label}</label>
          {/each}
          <label><input type="radio" bind:group={choice} value="other" /> Another port
            <input class="grow" placeholder="COM6 or bt:11:22:33:44:55:66" bind:value={other} onfocus={() => (choice = 'other')} aria-label="Port name" />
          </label>
          {#if !ports.ports.length}<p class="help">No Bluetooth ports found. {ports.help}</p>{/if}
          <button class="go" disabled={busy || (choice === 'other' && !other.trim())} onclick={connect}>Connect</button>
        </div>
      {/if}
    {/if}
    <button class="quiet" disabled={busy || !online} onclick={() => command('Screen turned off', 'off')}>Turn screen off</button>
    {#if update?.supported}
      <div class="version">
        <p class="help">Sideglance {status.version}</p>
        <button class="quiet" disabled={busy || !!update.busy} onclick={checkUpdate}>
          {update.busy === 'checking' ? 'Checking…' : 'Check for updates'}
        </button>
        {#if checkedNow && !update.busy}
          <p class="help" class:bad={!!update.error && !update.available}>
            {update.available ? `Version ${update.available.version} is ready.` : update.error ? update.error + '.' : 'This is the newest version.'}
          </p>
        {/if}
      </div>
    {/if}
  </aside>

  <main>
    <div class="tabs" role="tablist" aria-label="Choose what the screen shows">
      {#each TABS as [id, name]}
        <button role="tab" aria-selected={tab === id} class:on={tab === id} onclick={() => (tab = id)}>{name}</button>
      {/each}
    </div>

    <section class="panel" role="tabpanel">
      {#if tab === 'usage'}
        <h2>AI usage</h2>
        {#if status.usage?.providers?.length}
          {#each status.usage.providers as p}
            <div class="provider">
              <h3><i style:background="rgb({p.color.join(',')})"></i>{p.name}</h3>
              {#each [['D', '5-hour window', p.pct_d, p.rem_d], ['W', '7-day window', p.pct_w, p.rem_w]] as [key, name, pct, rem]}
                <div class="meter {level(pct)}">
                  <div class="meter-head"><span><b>{key}</b> {name}</span><span>{Math.round(pct)}%</span></div>
                  <div class="track"><i style:width="{Math.min(100, pct)}%"></i></div>
                  <p>Resets in {left(rem)}</p>
                </div>
              {/each}
            </div>
          {/each}
          {#each status.usage.notes ?? [] as note}
            <p class="help bad">Skipped {note}</p>
          {/each}
        {:else}
          <p class="lead">Shows Claude and Codex usage, for whichever you are signed in to on this computer. The screen alternates between the percentage used and the time left until reset, and updates every 5 minutes.</p>
          <p class="lead">On the screen, orange D and W labels mean Claude and green ones mean Codex.</p>
        {/if}
        {#if status.mode === 'usage' && !status.usage && status.error}
          <p class="help bad">{status.error}</p>
        {/if}
        {#if status.mode === 'usage' && !status.notification}
          <button disabled={busy} onclick={() => post('/api/usage', { on: false }, 'Usage stopped')}>Stop showing usage</button>
        {:else}
          <button class="go" disabled={busy} onclick={() => post('/api/usage', { on: true }, 'Usage shown')}>Show usage</button>
        {/if}

        {#if !HOSTED}
        <details class="accounts" bind:open={accountsOpen}>
          <summary>Accounts and tokens</summary>
          <div class="stack">
          <p class="lead">Tokens are found automatically when Claude Code or the Codex CLI is signed in on this computer. If one is not found, enter a token, or the path to its credentials file. A path keeps working after the token refreshes. A pasted token stops working when it expires.</p>
          <div class="group">
            <h3>Claude</h3>
            <p class="help">{SOURCES[status.accounts?.claude] ?? 'Not found'}</p>
            <p class="help">The token is <code>claudeAiOauth.accessToken</code> in <code>~/.claude/.credentials.json</code>.</p>
            <div class="row">
              <input class="grow" type="password" autocomplete="off" placeholder="Token or file path" bind:value={claudeToken} aria-label="Claude token or credentials file path" />
              <button disabled={busy || !claudeToken.trim()} onclick={() => saveToken({ claude_token: claudeToken }, 'Claude token saved')}>Save Claude token</button>
              {#if status.accounts?.claude === 'manual'}
                <button disabled={busy} onclick={() => saveToken({ claude_token: '' }, 'Claude token removed')}>Remove</button>
              {/if}
            </div>
          </div>
          <div class="group">
            <h3>Codex</h3>
            <p class="help">{SOURCES[status.accounts?.codex] ?? 'Not found'}</p>
            <p class="help">The token is <code>tokens.access_token</code> in <code>~/.codex/auth.json</code>. The account ID is <code>tokens.account_id</code> in the same file.</p>
            <div class="row">
              <input class="grow" type="password" autocomplete="off" placeholder="Token or file path" bind:value={codexToken} aria-label="Codex token or auth file path" />
              <input class="grow" autocomplete="off" placeholder="Account ID" bind:value={codexAccount} aria-label="Codex account ID" />
              <button disabled={busy || !codexToken.trim()} onclick={() => saveToken({ codex_token: codexToken, codex_account_id: codexAccount }, 'Codex token saved')}>Save Codex token</button>
              {#if status.accounts?.codex === 'manual'}
                <button disabled={busy} onclick={() => saveToken({ codex_token: '', codex_account_id: '' }, 'Codex token removed')}>Remove</button>
              {/if}
            </div>
          </div>
          </div>
        </details>
        {/if}

      {:else if tab === 'system'}
        {@const sys = status.system}
        <h2>Computer</h2>
        <div class="weather-now">
          <div class="mini"><PixelGrid pixels={sys?.face ?? BLANK} label="Preview of the Computer face" /></div>
          <div class="card-text">
            {#if sys?.cpu != null}
              <p class="reading">CPU {Math.round(sys.cpu)}%, memory {Math.round(sys.ram)}%</p>
            {:else}
              <p class="help">Shows how busy this computer is: CPU on top (C), memory below (R). It updates every few seconds while it shows.</p>
            {/if}
            {#if sys?.battery}
              <p class="help">Battery {Math.round(sys.battery.percent)}%{sys.battery.charging ? ', charging' : ''}.</p>
            {/if}
          </div>
        </div>
        {#if status.mode === 'system' && !status.notification}
          <button disabled={busy} onclick={() => post('/api/system', { on: false }, 'Computer stopped')}>Stop showing the computer</button>
        {:else}
          <button class="go" disabled={busy || demo} onclick={() => post('/api/system', { on: true }, 'Computer shown')}>Show the computer</button>
        {/if}
        <p class="help">
          {sys?.battery
            ? 'When the battery drops below 15% and it is not charging, the Battery low notification shows, at most every 30 minutes.'
            : 'On a laptop, the Battery low notification shows when its battery drops below 15% and it is not charging.'}
          The Ditoo Pro does not report its own battery.
        </p>

      {:else if tab === 'weather'}
        {@const w = status.weather}
        <h2>Weather</h2>
        {#if w}
          <div class="weather-now">
            <div class="mini"><PixelGrid pixels={w.face} label="Preview of the weather face" /></div>
            <div class="card-text">
              {#if w.reading.fetched_at}
                <p class="reading">{Math.round(w.reading.temperature)}°{w.unit.toUpperCase()} and {CONDITION[w.reading.condition]}</p>
                <p class="help">{w.place}. Updated {ago(w.reading.fetched_at)}.</p>
              {:else if w.latitude == null && !w.auto}
                <p class="help">Set a location below. Until there is a reading, the face shows a cloud and --.</p>
              {:else if w.latitude == null}
                <p class="help">No reading yet. Your location is found when you show the weather.</p>
              {:else}
                <p class="help">No reading yet for {w.place}. It arrives when you show the weather.</p>
              {/if}
              {#if w.error && (status.mode === 'weather' || w.in_rotation)}<p class="help bad">{w.error}</p>{/if}
            </div>
          </div>
          {#if status.mode === 'weather' && !status.notification}
            <button disabled={busy} onclick={() => post('/api/weather', { on: false }, 'Weather stopped')}>Stop showing the weather</button>
          {:else}
            <button class="go" disabled={busy} onclick={() => post('/api/weather', { on: true }, 'Weather shown')}>Show the weather</button>
          {/if}

          <div class="group">
            <h3>Location</h3>
            <p class="help">
              {#if w.auto}
                {w.place ? `Found automatically: ${w.place}.` : 'Found automatically from your internet connection.'}
                This is approximate. Set a city if it is wrong.
              {:else}
                Location in use: {w.place}.
              {/if}
            </p>
            <div class="row">
              {#if !w.auto}
                <button disabled={busy} onclick={() => weatherSettings({ auto: true }, 'Location found automatically')}>Find it automatically</button>
              {/if}
              <button disabled={busy} onclick={useDeviceLocation}>Use this device's location</button>
            </div>
            {#if w.system_location?.checked && w.system_location.off.length}
              <p class="help bad">
                Windows is not sharing a location, so this device's location will not work. Turn on
                {w.system_location.off.map((name) => `"${name}"`).join(' and ')} in
                <a href="ms-settings:privacy-location">Settings, Privacy & security, Location</a>.
              </p>
            {:else if locationProblem}<p class="help bad">{locationProblem}</p>{/if}
            <div class="row">
              <input class="grow" placeholder="City, for example Chiang Mai" bind:value={city} aria-label="City"
                onkeydown={(e) => e.key === 'Enter' && city.trim() && findCity()} />
              <button disabled={busy || city.trim().length < 2} onclick={findCity}>Find city</button>
            </div>
            <details class="coords">
              <summary>Use latitude and longitude instead</summary>
              <div class="row">
                <label class="inline">Latitude <input type="number" step="0.01" min="-90" max="90" bind:value={lat} /></label>
                <label class="inline">Longitude <input type="number" step="0.01" min="-180" max="180" bind:value={lon} /></label>
                <button disabled={busy || lat === '' || lon === ''}
                  onclick={() => weatherSettings({ latitude: lat, longitude: lon }, 'Location saved')}>Save coordinates</button>
              </div>
            </details>
          </div>
          <div class="group">
            <h3>Unit</h3>
            <div class="chips">
              {#each [['c', 'Celsius'], ['f', 'Fahrenheit']] as [unit, name]}
                <button class:on={w.unit === unit} disabled={busy} onclick={() => weatherSettings({ unit }, `${name} saved`)}>{name}</button>
              {/each}
            </div>
          </div>
          <div class="group">
            <h3>AI usage rotation</h3>
            <p class="help">{w.in_rotation
              ? 'The weather face plays after the usage faces while AI usage is showing.'
              : 'Add the weather face after the usage faces, so one screen shows both.'}</p>
            <button disabled={busy} onclick={() => weatherSettings({ in_rotation: !w.in_rotation },
              w.in_rotation ? 'Removed from the rotation' : 'Added to the rotation')}>
              {w.in_rotation ? 'Remove from the rotation' : 'Add to the rotation'}</button>
          </div>
          <p class="help">Weather from <a href="https://open-meteo.com/">Open-Meteo</a>, updated every 15 minutes.</p>
        {:else}
          <p class="help">The app is older than this page. Update Sideglance.</p>
        {/if}

      {:else if tab === 'timer'}
        <h2>Timer</h2>
        <div class="chips" role="group" aria-label="Kind of timer">
          <button class:on={timerKind === 'pomodoro'} onclick={() => (timerKind = 'pomodoro')}>Pomodoro</button>
          <button class:on={timerKind === 'countdown'} onclick={() => (timerKind = 'countdown')}>Countdown</button>
        </div>
        {#if timerKind === 'pomodoro'}
          {#if pomo}
            <div class="weather-now">
              <div class="mini"><PixelGrid pixels={pomo.face} label="Preview of the Pomodoro face" /></div>
              <div class="card-text">
                <p class="reading">{PHASE[pomo.phase]}, {clockText(pomo.seconds_left)}</p>
                <p class="help">
                  {pomo.sessions_completed} of {pomo.sessions_per_set} focus sessions done.
                  {pomo.status === 'paused' ? 'Paused.' : pomo.status === 'waiting' ? 'Ready to start.' : pomo.status === 'idle' ? 'Not started.' : ''}
                </p>
              </div>
            </div>
            <div class="row">
              {#if pomo.status === 'running'}
                <button disabled={busy} onclick={() => pomodoro('pause', 'Paused')}>Pause</button>
              {:else if pomo.status === 'paused'}
                <button class="go" disabled={busy} onclick={() => pomodoro('resume', 'Resumed')}>Resume</button>
              {:else}
                <button class="go" disabled={busy} onclick={() => pomodoro('start', `${PHASE[pomo.phase]} started`)}>Start {PHASE[pomo.phase].toLowerCase()}</button>
              {/if}
              <button disabled={busy} onclick={() => pomodoro('skip', 'Skipped to the next phase')}>Skip to next phase</button>
              {#if pomo.active}
                <button disabled={busy} onclick={() => pomodoro('reset', 'Pomodoro reset')}>Reset</button>
              {:else}
                <button disabled={busy} onclick={() => pomodoro('show', 'Pomodoro shown')}>Show on display</button>
              {/if}
            </div>
            <p class="help">
              {pomo.active
                ? 'The Pomodoro face covers the other faces until you reset it. When a phase ends, the hourglass blinks until you start the next phase or dismiss it.'
                : 'Focus for 25 minutes, take a short break, and after four focus sessions take a long one. The display shows the minutes left, a ring for the seconds, and a dot per session.'}
            </p>

            {#if pomoForm}
              <div class="group">
                <h3>Lengths</h3>
                <div class="row">
                  <label class="inline">Focus <input type="number" min="1" max="99" bind:value={pomoForm.focus} /> min</label>
                  <label class="inline">Short break <input type="number" min="1" max="99" bind:value={pomoForm.short} /> min</label>
                  <label class="inline">Long break <input type="number" min="1" max="99" bind:value={pomoForm.long} /> min</label>
                  <label class="inline">Sessions per set <input type="number" min="1" max="4" bind:value={pomoForm.sessions} /></label>
                </div>
                <h3>Next phase</h3>
                <div class="chips">
                  <button class:on={!pomoForm.auto_start} onclick={() => (pomoForm.auto_start = false)}>Wait for me</button>
                  <button class:on={pomoForm.auto_start} onclick={() => (pomoForm.auto_start = true)}>Start on its own</button>
                </div>
                <button disabled={busy} onclick={() => post('/api/pomodoro/settings', pomoForm, 'Pomodoro settings saved')}>Save settings</button>
              </div>
            {/if}
          {:else}
            <p class="help">The app is older than this page. Update Sideglance.</p>
          {/if}

        {:else}
          {#if status.mode === 'timer'}
            <p class="big">{Math.ceil((status.timer_left ?? 0) / 60)}<small> min left</small></p>
            <div class="row">
              {#if status.notification}
                <button class="go" disabled={busy} onclick={() => post('/api/notify', { event: 'dismiss' }, 'Timer shown')}>Show timer</button>
              {/if}
              <button disabled={busy} onclick={() => post('/api/timer', { minutes: 0 }, 'Timer stopped')}>Stop timer</button>
            </div>
          {:else}
            <p class="lead">The screen counts down the minutes. When time is up, an hourglass blinks until you dismiss it, then the screen goes back to what it showed before.</p>
            <div class="chips">
              {#each [15, 25, 50] as m}
                <button class:on={minutes === m} onclick={() => (minutes = m)}>{m} min</button>
              {/each}
              <label class="inline">Custom <input type="number" min="1" max="99" bind:value={minutes} /></label>
            </div>
            <button class="go" disabled={busy} onclick={() => post('/api/timer', { minutes }, 'Timer started')}>Start {minutes}-minute timer</button>
          {/if}

        {/if}

      {:else if tab === 'notify'}
        <h2>Notifications</h2>
        <p class="lead">A notification shows over whatever is on the display, then the display goes back to it.</p>
        <div class="group">
          <h3>Claude Code</h3>
          {#if hooks?.installed}
            <p class="help">Connected. Claude Code shows the first two notifications on the display. Restart Claude Code if it was open when you connected it.</p>
            <button disabled={busy} onclick={() => setHooks(false)}>Disconnect Claude Code</button>
          {:else}
            <p class="help">Show when Claude Code is waiting for you and when it finishes. This adds hooks to <code>{hooks?.path ?? '~/.claude/settings.json'}</code> and keeps a copy of the old file next to it.</p>
            <button class="go" disabled={busy || demo} onclick={() => setHooks(true)}>Connect Claude Code</button>
          {/if}
          {#if hooks?.error}<p class="help bad">{hooks.error}</p>{/if}
        </div>
        <div class="cards">
          {#each catalog as n (n.id)}
            <article class="card" class:active={status.notification === n.id} aria-label={n.label}>
              <div class="mini"><PixelGrid pixels={previewFrame(n)} label="Preview of {n.label}" /></div>
              <div class="card-text">
                <h3>{n.label}</h3>
                <p class="help">{n.behavior}</p>
                <p class="help"><code>{n.id}</code></p>
                {#if status.notification === n.id}
                  <button disabled={busy} onclick={dismiss}>Dismiss</button>
                {:else}
                  <button class="go small" disabled={busy} onclick={() => post('/api/notify', { event: n.id }, 'Notification shown')}>Show on display</button>
                {/if}
              </div>
            </article>
          {:else}
            <p class="help">
              {catalogMissing
                ? 'The app is older than this page. Update Sideglance.'
                : 'Loading notifications'}
            </p>
          {/each}
        </div>

      {:else if tab === 'clock'}
        <h2>Clock</h2>
        <div class="chips">
          {#each CLOCKS as name, i}
            <button class:on={clockStyle === i} onclick={() => (clockStyle = i)}>{name}</button>
          {/each}
        </div>
        <ColorPicker bind:value={clockColor} swatches={SWATCHES} label="Clock color" />
        <button class="go" disabled={busy} onclick={() => command('Clock shown', 'clock', clockStyle, hex(clockColor))}>Show clock</button>
        <hr />
        <div class="group">
          <h3>Device clock and weather</h3>
          <div class="row">
            <button disabled={busy} onclick={() => command('Time synced', 'time')}>Sync time from this computer</button>
          </div>
        <div class="row">
          <label class="inline">Temperature <input type="number" min="-50" max="60" bind:value={temp} /> °C</label>
          <span class="inline">Weather
            <Select bind:value={weather} options={WEATHER} label="Weather" />
          </span>
          <button disabled={busy} onclick={() => command('Weather sent', 'clock-weather', temp, weather)}>Send weather</button>
        </div>
        </div>

      {:else if tab === 'light'}
        <h2>Light</h2>
        <p class="lead">Fills the screen with one color, at the brightness set under the screen.</p>
        <ColorPicker bind:value={lightColor} swatches={SWATCHES} label="Light color"
          onpick={(color) => command('', 'light', hex(color), brightness)} />
        <div class="row">
          <button disabled={busy} onclick={() => command('', 'light', 'ffffff', brightness, 1)}>Cycle through colors</button>
        </div>

      {:else if tab === 'draw'}
        <h2>Draw</h2>
        <div class="draw">
          <div class="canvas"><PixelGrid pixels={canvas} onpaint={(i) => (canvas[i] = brush)} label="16 by 16 drawing area" /></div>
          <div class="tools">
            <ColorPicker bind:value={brush} swatches={SWATCHES} label="Brush color" />
            <div class="row">
              <button class:on={brush === '#000000'} onclick={() => (brush = '#000000')}>Eraser</button>
              <button onclick={() => (canvas = [...BLANK])}>Clear drawing</button>
            </div>
            <button class="go" disabled={busy} onclick={() => post('/api/pixels', { pixels: canvas }, 'Drawing shown')}>Show drawing</button>
          </div>
        </div>

      {:else if tab === 'score'}
        <h2>Score</h2>
        <div class="score">
          {#each [['red', 'Red', red], ['blue', 'Blue', blue]] as [side, name, value]}
            <div class="side {side}">
              <span>{name}</span>
              <div class="stepper">
                <button aria-label="Subtract a point from {name}" disabled={busy} onclick={() => setScore(side, -1)}>−</button>
                <output>{value}</output>
                <button aria-label="Add a point to {name}" disabled={busy} onclick={() => setScore(side, 1)}>+</button>
              </div>
            </div>
          {/each}
        </div>
        <button onclick={() => { red = 0; blue = 0; command('Score reset', 'score', 0, 0); }}>Reset score</button>
      {/if}
    </section>

    <details>
      <summary>Testing and advanced commands</summary>
      <div class="stack">
        {#if !demo}
          <div class="group">
            <h3>Notification playback</h3>
            {#each PLAYBACK as [mode, name, help]}
              <label class="choice">
                <input type="radio" name="playback" value={mode} checked={status.playback === mode}
                  onchange={() => post('/api/playback', { mode }, 'Playback changed')} />
                <span><b>{name}</b><br /><span class="help">{help}</span></span>
              </label>
            {/each}
          </div>
        {/if}
        <div class="group">
          <h3>Built-in channels</h3>
          <div class="chips">
            <button disabled={busy} onclick={() => command('', 'cloud')}>Cloud</button>
          </div>
        </div>
        <div class="group">
          <h3>Send a raw hex command</h3>
          <div class="row">
            <input class="grow" placeholder="For example 4505" bind:value={raw} aria-label="Raw hex command" />
            <button disabled={busy || !raw.trim()} onclick={() => command('Command sent', 'raw', raw.replace(/\s+/g, ''))}>Send command</button>
          </div>
        </div>
      </div>
    </details>
  </main>

  {#if toast}<p class="toast" role="status">{toast}</p>{/if}
</div>

<style>
  :global(:root) {
    --paper: #e4e9f2;
    --panel: #f7f9fc;
    --ink: #161c2c;
    --soft: #5a6478;
    --line: #c9d1df;
    --blue: #2457e6;
    --shell: #232a3d;
    --low: #161c2c;
    --mid: #c98a00;
    --high: #d63a31;
  }
  :global(body) {
    margin: 0;
    background: var(--paper);
    color: var(--ink);
    font: 400 16px/1.55 'Bai Jamjuree', system-ui, sans-serif;
  }
  /* Line breaks: balance short text so no word is left alone, and avoid orphans in paragraphs. */
  h1, h2, h3, summary, .help, .glass-note, .file, .meter p { text-wrap: balance; }
  .lead { text-wrap: pretty; }
  button, .inline, .meter-head span, .custom { white-space: nowrap; }
  .page {
    max-width: 1080px;
    margin: 0 auto;
    padding: 40px 24px 64px;
    display: grid;
    grid-template-columns: minmax(260px, 340px) 1fr;
    gap: 48px;
    align-items: start;
  }
  .device { position: sticky; top: 32px; display: flex; flex-direction: column; gap: 12px; align-items: flex-start; }
  .shell {
    width: 100%;
    box-sizing: border-box;
    background: var(--shell);
    border-radius: 28px 28px 18px 18px;
    padding: 18px 18px 14px;
    box-shadow: inset 0 -6px 0 #171c2b, 0 18px 30px -18px #232a3d99;
  }
  .glass { position: relative; background: #05060a; border-radius: 14px; padding: 14px; box-shadow: inset 0 0 24px #000; }
  .glass.dim { opacity: 0.85; }
  .glass-note {
    position: absolute; inset: 0; margin: 0; display: grid; place-items: center;
    color: #d5dbea; font-size: 14px; text-align: center; padding: 0 24px;
    background: #07080cd9; border-radius: 14px;
  }
  .knob { display: flex; flex-direction: column; gap: 4px; margin-top: 14px; color: #c5ccdc; font-size: 14px; }
  .knob input[type='range'],
  .knob input[type='range']:hover,
  .knob input[type='range']:focus,
  .knob input[type='range']:focus-visible {
    -webkit-appearance: none; appearance: none; width: 100%; height: 22px; margin: 0; padding: 0;
    border: 0; border-radius: 0; background: none; box-shadow: none; outline: none; cursor: pointer;
  }
  .knob input[type='range']::-webkit-slider-runnable-track {
    height: 6px; border-radius: 3px;
    background: linear-gradient(90deg, #ffb800 var(--fill), #3a4256 var(--fill));
  }
  .knob input[type='range']::-moz-range-track { height: 6px; border-radius: 3px; background: #3a4256; }
  .knob input[type='range']::-moz-range-progress { height: 6px; border-radius: 3px; background: #ffb800; }
  .knob input[type='range']::-webkit-slider-thumb {
    -webkit-appearance: none; width: 18px; height: 18px; margin-top: -6px; border-radius: 50%;
    background: #ffb800; border: 3px solid var(--shell); transition: box-shadow 0.15s;
  }
  .knob input[type='range']::-moz-range-thumb {
    width: 12px; height: 12px; border-radius: 50%; background: #ffb800; border: 3px solid var(--shell);
  }
  /* keyboard focus shows as a halo on the thumb, not a box around the whole bar */
  .knob input[type='range']:focus-visible::-webkit-slider-thumb { box-shadow: 0 0 0 4px #ffb80066; }
  .knob input[type='range']:focus-visible::-moz-range-thumb { box-shadow: 0 0 0 4px #ffb80066; }
  @media (prefers-reduced-motion: reduce) { .knob input[type='range']::-webkit-slider-thumb { transition: none; } }
  .brand { margin: 0 0 4px; font: 600 18px 'Chakra Petch', sans-serif; letter-spacing: 0.02em; }
  .banner {
    grid-column: 1 / -1; margin: 0; padding: 12px 16px; border-radius: 12px;
    background: #fff4d1; border: 1px solid #e6c766; color: #4a3a05; text-wrap: pretty;
  }
  .banner.update { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 16px; background: #e4efff; border-color: #9dbcf2; color: #10294f; }
  .banner.update p { margin: 0; flex: 1 1 280px; }
  .banner.update .bad { flex-basis: 100%; color: var(--high); }
  .banner.update a { color: inherit; }
  .version { display: flex; flex-direction: column; align-items: flex-start; gap: 4px; padding-top: 12px; border-top: 1px solid var(--line); width: 100%; }
  .version .help { font-size: 14px; }
  .banner code { background: #f3e2a4; white-space: normal; overflow-wrap: anywhere; }
  .help a { color: var(--blue); }
  h1 { font: 600 26px/1.25 'Chakra Petch', sans-serif; margin: 12px 0 0; }
  .help { margin: 0; color: var(--soft); font-size: 15px; }
  .help.bad { color: var(--high); }
  .link { width: 100%; display: flex; flex-direction: column; gap: 8px; align-items: flex-start; padding: 12px 0; border-top: 1px solid var(--line); border-bottom: 1px solid var(--line); }
  .link label { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; font-size: 15px; }
  .link input[type='radio'] { accent-color: var(--blue); width: 18px; height: 18px; margin: 0; padding: 0; }
  code { font-family: ui-monospace, Consolas, monospace; font-size: 0.9em; background: #d6dcea; padding: 1px 5px; border-radius: 4px; white-space: nowrap; }
  main { display: flex; flex-direction: column; gap: 28px; min-width: 0; }
  /* one row that scrolls sideways when the tabs do not fit, rather than a second row */
  .tabs { display: flex; flex-wrap: nowrap; overflow-x: auto; scrollbar-width: none; gap: 2px; border-bottom: 2px solid var(--ink); }
  .tabs::-webkit-scrollbar { display: none; }
  .tabs button {
    flex: none; white-space: nowrap;
    border: 0; background: none; box-shadow: none; border-radius: 8px 8px 0 0; padding: 9px 10px;
    font: 500 15px 'Chakra Petch', sans-serif; color: var(--soft);
  }
  .tabs button:hover:not(:disabled) { color: var(--ink); background: #d6dcea; box-shadow: none; }
  .tabs button.on:hover:not(:disabled) { background: var(--ink); color: #fff; box-shadow: none; }
  .tabs button.on { background: var(--ink); color: #fff; }
  .panel { display: flex; flex-direction: column; gap: 20px; align-items: flex-start; min-height: 300px; }
  h2 { font: 600 30px/1.2 'Chakra Petch', sans-serif; margin: 6px 0 0; }
  h3 { font: 600 17px 'Chakra Petch', sans-serif; margin: 0; }
  .stack { display: flex; flex-direction: column; gap: 28px; margin-top: 20px; }
  .group { display: flex; flex-direction: column; gap: 12px; align-items: flex-start; width: 100%; }
  .lead { margin: 0; color: var(--soft); max-width: 58ch; }
  .big { font: 600 72px/1 'Chakra Petch', sans-serif; margin: 0; }
  .big small { font-size: 22px; font-weight: 500; }
  hr { width: 100%; border: 0; border-top: 1px solid var(--line); margin: 8px 0; }
  .row { display: flex; flex-wrap: wrap; gap: 12px; align-items: center; width: 100%; }
  .chips { display: flex; flex-wrap: wrap; gap: 10px; align-items: center; }
  .inline { display: inline-flex; align-items: center; gap: 8px; }

  /* Outlines are soft rings (box-shadow) rather than 1px borders: at 125% or 150% display scaling a 1px
     border cannot land on whole screen pixels and its thickness wobbles around rounded corners. */
  :global(:root) { --ring: inset 0 0 0 1px #c3cbda; --ring-hover: inset 0 0 0 1px #9aa4b8; --lift: 0 1px 2px #161c2c14; }
  button, input {
    font: 500 15px 'Bai Jamjuree', sans-serif; color: var(--ink);
    background: var(--panel); border: 0; border-radius: 10px; padding: 9px 15px;
    box-shadow: var(--ring), var(--lift);
    transition: background-color 120ms ease, box-shadow 120ms ease, color 120ms ease;
  }
  button, input { box-sizing: border-box; max-width: 100%; }
  button { cursor: pointer; }
  button:hover:not(:disabled) { background: #fff; box-shadow: var(--ring-hover), 0 2px 6px #161c2c1f; }
  button:active:not(:disabled) { box-shadow: var(--ring-hover); }
  button:disabled { opacity: 0.45; cursor: default; }
  button.on, button.on:hover:not(:disabled) { background: var(--ink); color: #fff; box-shadow: 0 1px 2px #161c2c33; }
  button.go { background: var(--blue); color: #fff; font-weight: 600; padding: 12px 23px; font-size: 16px; box-shadow: 0 1px 2px #2457e64d; }
  button.go:hover:not(:disabled) { background: #1b46c2; box-shadow: 0 3px 10px -2px #2457e680; }
  button.quiet, button.quiet:hover:not(:disabled), button.quiet:active:not(:disabled) {
    background: none; box-shadow: none; padding: 4px 0; color: var(--blue);
    text-decoration: underline; text-decoration-thickness: 1px; text-underline-offset: 3px;
  }
  button.quiet:hover:not(:disabled) { color: #173a9c; text-decoration-thickness: 2px; }
  input:hover:not(:disabled) { box-shadow: var(--ring-hover); }
  input:focus { background: #fff; }
  @media (prefers-reduced-motion: reduce) { button, input { transition: none; } }
  input[type='number'] { width: 72px; }
  .grow { flex: 1; min-width: 160px; }
  :is(button, input:not([type='range']), summary):focus-visible { outline: 3px solid #ffb800; outline-offset: 2px; }

  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 14px; width: 100%; }
  .card {
    display: flex; gap: 14px; align-items: flex-start; padding: 14px; border-radius: 14px;
    background: var(--panel); box-shadow: var(--ring), var(--lift);
  }
  .card.active { box-shadow: inset 0 0 0 2px var(--ink), var(--lift); }
  .mini { width: 104px; flex: none; background: #05060a; padding: 7px; border-radius: 10px; box-sizing: border-box; }
  .card-text { display: flex; flex-direction: column; gap: 6px; align-items: flex-start; min-width: 0; }
  .card-text h3 { font-size: 16px; }
  .card-text .help { font-size: 14px; }
  button.go.small { padding: 7px 14px; font-size: 15px; }
  .choice { display: flex; gap: 10px; align-items: flex-start; cursor: pointer; }
  .choice input { accent-color: var(--blue); width: 18px; height: 18px; margin: 3px 0 0; padding: 0; flex: none; }
  .weather-now { display: flex; gap: 16px; align-items: center; }
  .reading { margin: 0; font: 600 22px/1.2 'Chakra Petch', sans-serif; }
  .coords summary { font-size: 15px; }
  .coords .row { margin-top: 10px; }
  .coords input[type='number'] { width: 96px; }
  .provider { width: 100%; max-width: 460px; display: flex; flex-direction: column; gap: 14px; }
  .provider h3 { margin: 0; display: flex; align-items: center; gap: 8px; font-size: 20px; }
  .provider h3 i { width: 14px; height: 14px; border-radius: 50%; }
  .meter { width: 100%; max-width: 460px; --tone: var(--low); }
  .meter.mid { --tone: var(--mid); }
  .meter.high { --tone: var(--high); }
  .meter-head { display: flex; justify-content: space-between; font: 600 17px 'Chakra Petch', sans-serif; color: var(--tone); }
  .meter-head b { display: inline-block; width: 1.4em; }
  .track { height: 12px; background: #cfd6e4; border-radius: 6px; overflow: hidden; margin: 6px 0 4px; }
  .track i { display: block; height: 100%; background: var(--tone); border-radius: 6px; }
  .meter p { margin: 0; color: var(--soft); font-size: 15px; }


  .draw { display: flex; flex-wrap: wrap; gap: 28px; width: 100%; }
  .canvas { align-self: flex-start; width: min(100%, 340px); background: #05060a; padding: 12px; border-radius: 14px; box-sizing: border-box; }
  .tools { display: flex; flex-direction: column; gap: 18px; align-items: flex-start; flex: 1; min-width: 220px; }

  .score { display: flex; flex-wrap: wrap; gap: 32px; }
  .side span { font: 600 18px 'Chakra Petch', sans-serif; }
  .side.red { color: var(--high); }
  .side.blue { color: var(--blue); }
  .stepper { display: flex; align-items: center; gap: 12px; margin-top: 6px; }
  .stepper button { width: 46px; height: 46px; padding: 0; font-size: 24px; border-radius: 50%; }
  .stepper output { font: 600 56px/1 'Chakra Petch', sans-serif; min-width: 1.6em; text-align: center; }

  details { border-top: 1px solid var(--line); padding-top: 18px; width: 100%; }
  .accounts { width: 100%; max-width: 620px; }
  summary { cursor: pointer; font-weight: 500; color: var(--soft); width: fit-content; border-radius: 6px; }

  .toast {
    position: fixed; left: 50%; bottom: 28px; transform: translateX(-50%); margin: 0;
    background: var(--ink); color: #fff; padding: 10px 20px; border-radius: 999px; font-weight: 500;
  }

  @media (max-width: 760px) {
    .page { grid-template-columns: minmax(0, 1fr); gap: 24px; padding: 20px 16px 56px; }
    .device { position: static; align-items: center; text-align: center; }
    .shell { max-width: 280px; }
    .link { text-align: left; }
    h1 { font-size: 22px; }
    h2 { font-size: 26px; }
    .big { font-size: 56px; }
    .tabs { flex-wrap: nowrap; overflow-x: auto; scrollbar-width: none; margin: 0 -16px; padding: 0 16px; }
    .tabs::-webkit-scrollbar { display: none; }
    .tabs button { flex: none; white-space: nowrap; padding: 9px 12px; }
    .panel { min-height: 0; }
    .panel > .go, .tools > .go { width: 100%; }
    .canvas { width: 100%; }
    .score { gap: 20px; }
    .toast { width: max-content; max-width: calc(100vw - 32px); }
  }
</style>
