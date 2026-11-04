// Aggregation is based only on received observations; empty minutes are not filled in.
(function (root) {
  'use strict';
  function lineData(data) {
    const bySecond = new Map();
    for (const point of data) bySecond.set(Math.floor(point.t / 1000), point.v);
    return [...bySecond].map(([time, value]) => ({ time, value }));
  }
  function candleData(data) {
    const bars = [];
    for (const point of data) {
      const time = Math.floor(point.t / 60000) * 60;
      let bar = bars.at(-1);
      if (!bar || bar.time !== time) {
        bar = { time, open: point.v, high: point.v, low: point.v, close: point.v };
        bars.push(bar);
      } else {
        bar.high = Math.max(bar.high, point.v);
        bar.low = Math.min(bar.low, point.v);
        bar.close = point.v;
      }
    }
    return bars;
  }
  function combine(history, observed, kind) {
    const historical = history.map(b => ({time:b.time,open:Number(b.open),high:Number(b.high),low:Number(b.low),close:Number(b.close)}));
    const cutoff = historical.length ? (historical.at(-1).time + 60) * 1000 : -Infinity;
    const live = observed.filter(p => p.t >= cutoff);
    const later = kind === 'line' ? lineData(live) : candleData(live);
    const result = [];
    for (const bar of historical) {
      if (result.length && bar.time - result.at(-1).time > 60) result.push({time:result.at(-1).time + 60});
      result.push(kind === 'line' ? {time:bar.time,value:bar.close} : bar);
    }
    if (result.length && later.length && later[0].time - result.at(-1).time > 120) result.push({time:result.at(-1).time + 60});
    return result.concat(later);
  }
  const api = { lineData, candleData, combine };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.ChartData = api;
})(globalThis);
