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
  const api = { lineData, candleData };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.ChartData = api;
})(globalThis);
