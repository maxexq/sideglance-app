// Which frame of a timeline is showing `elapsed` ms after it started.
// durations: one duration per frame. A timeline that does not loop stays on its last frame.
export function frameAt(durations, loop, elapsed) {
  if (!durations?.length) return 0;
  const total = durations.reduce((sum, ms) => sum + ms, 0);
  if (total <= 0) return 0;
  let t = Math.max(0, elapsed);
  if (loop) t %= total;
  else if (t >= total) return durations.length - 1;
  for (let i = 0; i < durations.length; i++) {
    if (t < durations[i]) return i;
    t -= durations[i];
  }
  return durations.length - 1;
}
