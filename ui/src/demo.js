// A stand-in for server.py, used by the hosted page when no local server is reachable.
// It answers the same routes with the same shape of status, from frames made by tools/make_demo.py.
import data from './demo-data.json';

const NO_PICTURE = ['clock', 'vj', 'score', 'cloud', 'custom', 'visualizer', 'clock-weather'];
const EVENTS = Object.fromEntries(data.notifications.map((n) => [n.id, n]));
const ALIASES = { waiting: 'claude-waiting', done: 'claude-finished', ok: 'job-passed', fail: 'job-failed', timeup: 'timer-finished' };
const state = { mode: 'manual', showing: null, faces: null, frameMs: 4000, timerEnd: 0, before: 'manual',
  notification: null, notifiedAt: 0, unit: 'c', inRotation: false };
const fetchedAt = Date.now() / 1000 - 240;

const total = (n) => n.ms.reduce((a, b) => a + b, 0);

export function demoNotifications() {
  return data.notifications;
}

function notify(id) {
  const event = EVENTS[ALIASES[id] ?? id];
  if (event) Object.assign(state, { notification: event, notifiedAt: Date.now() });
}

function weatherFace() {
  return data.weather.faces[state.unit];
}

function showUsage() {
  const faces = state.inRotation ? [...data.usageFaces, weatherFace()] : data.usageFaces;
  Object.assign(state, { mode: 'usage', showing: 'usage', faces, frameMs: data.usageMs, timerEnd: 0 });
}

function showWeather() {
  Object.assign(state, { mode: 'weather', showing: 'weather', faces: [weatherFace()], frameMs: 4000, timerEnd: 0 });
}

function weatherStatus() {
  const r = data.weather.readings[state.unit];
  return { auto: false, system_location: { checked: false }, latitude: data.weather.latitude, longitude: data.weather.longitude, place: data.weather.place,
    unit: state.unit, in_rotation: state.inRotation, error: null, face: weatherFace(),
    reading: { ...r, fetched_at: fetchedAt } };
}

function tick() {
  const now = Date.now();
  // same rule as server.py: a timeline that does not loop ends, and the screen goes back
  if (state.notification && !state.notification.loop && now - state.notifiedAt >= total(state.notification)) {
    state.notification = null;
  }
  if (state.mode === 'timer') {
    const left = Math.ceil((state.timerEnd - now) / 1000);
    if (left <= 0) {
      notify('timer-finished');
      if (state.before === 'usage') showUsage();
      else if (state.before === 'weather') showWeather();
      else Object.assign(state, { mode: 'manual', showing: null, faces: null, timerEnd: 0 });
    } else {
      // keep the bar in step with the time left by borrowing the face drawn for that many minutes
      state.faces = [data.timer[String(Math.min(99, Math.ceil(left / 60)))]];
    }
  }
}

function status() {
  tick();
  const n = state.notification;
  return {
    mode: state.mode, port: 'demo', port_setting: 'auto', accounts: { claude: 'file', codex: 'file' },
    usage: state.mode === 'usage' ? data.usage : null, playback: 'device', weather: weatherStatus(),
    pomodoro: data.pomodoro,
    faces: state.faces, frame_ms: state.frameMs, showing: state.showing, error: null,
    timer_left: state.mode === 'timer' ? Math.max(0, Math.ceil((state.timerEnd - Date.now()) / 1000)) : null,
    notification: n ? n.id : null,
    ...(n && { notification_label: n.label, notification_loop: n.loop, notification_frames: n.frames,
      notification_ms: n.ms, notification_elapsed_ms: Date.now() - state.notifiedAt }),
  };
}

function manual(showing, faces) {
  Object.assign(state, { mode: 'manual', showing, faces, frameMs: 4000, notification: null, timerEnd: 0 });
}

export function demoCall(path, body) {
  if (path === '/api/ports') return { ports: [], setting: 'auto', help: '' };
  if (path === '/api/notifications') return { events: data.notifications };
  if (['/api/usage', '/api/timer', '/api/weather'].includes(path)) state.notification = null;   // same rule as server.py
  if (path === '/api/usage') body.on ? showUsage() : manual(null, null);
  if (path === '/api/weather') body.on ? showWeather() : manual(null, null);
  if (path === '/api/weather/settings') {
    if (body.unit) state.unit = body.unit;
    if ('in_rotation' in body) state.inRotation = !!body.in_rotation;
    if (state.mode === 'weather') showWeather();
    if (state.mode === 'usage') showUsage();
  }
  if (path === '/api/timer') {
    if (body.minutes > 0) {
      if (state.mode !== 'timer') state.before = state.mode;
      Object.assign(state, { mode: 'timer', showing: 'timer', frameMs: 4000, timerEnd: Date.now() + body.minutes * 60000 });
    } else if (state.mode === 'timer') {
      if (state.before === 'usage') showUsage();
      else if (state.before === 'weather') showWeather();
      else manual(null, null);
    }
  }
  if (path === '/api/notify') {
    const name = body.event ?? body.state;
    if (name === 'dismiss' || name === 'clear') state.notification = null;
    else notify(name);
  }
  if (path === '/api/pixels') manual('pixels', [body.pixels]);
  if (path === '/api/command') {
    const [first] = body.args ?? [];
    if (body.cmd === 'light') manual('light', [Array(256).fill('#' + String(first).replace('#', ''))]);
    else if (body.cmd === 'off') manual('off', [Array(256).fill('#000000')]);
    else if (NO_PICTURE.includes(body.cmd)) manual(body.cmd, null);
  }
  return { ok: true, ...status() };
}
