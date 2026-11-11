// Independent trusted SDK host. No private packets or credentials on stdout.
import { readFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
const [sdkDirectory, mode, directory, executable, expected = 'verified'] = process.argv.slice(2);
if (!sdkDirectory || !['http', 'stdio'].includes(mode) || !directory || !['verified', 'missing'].includes(expected) || (mode === 'stdio' && !executable)) {
  throw new Error('Usage: node scripts/verify_market_rules.mjs <SDK dist/esm> <http|stdio> <scope directory> <binary or -> [verified|missing]');
}
const load = file => import(pathToFileURL(join(resolve(sdkDirectory), file)).href);
const { Client } = await load('client/index.js');
const scope = JSON.parse(readFileSync(join(directory, 'scope.json'), 'utf8'));
const canonical = v => Array.isArray(v) ? v.map(canonical) : v && typeof v === 'object'
  ? Object.fromEntries(Object.keys(v).sort().map(k => [k, canonical(v[k])])) : v;
const hash = v => createHash('sha256').update(JSON.stringify(canonical(v))).digest('hex');
const ns = s => { const m = /^(.*T\d\d:\d\d:\d\d)(?:\.(\d{1,9}))?Z$/.exec(s); assert.ok(m); return BigInt(Date.parse(m[1] + 'Z')) * 1000000n + BigInt((m[2] || '').padEnd(9, '0')); };
const price = s => { const [a, b = ''] = s.split('.'); assert.ok(b.length <= 4); return BigInt(a) * 10000n + BigInt(b.padEnd(4, '0')); };
let transport;
if (mode === 'http') {
  const { StreamableHTTPClientTransport } = await load('client/streamableHttp.js');
  const cap = JSON.parse(readFileSync(join(directory, 'capability.json'), 'utf8'));
  const token = execFileSync('gcloud', ['auth', 'print-identity-token'], { encoding: 'utf8' }).trim();
  transport = new StreamableHTTPClientTransport(new URL('https://crypto-research-mcp-308289525742.us-central1.run.app/mcp'), {
    requestInit: { headers: { Authorization: `Bearer ${token}`, 'X-Research-Capability': cap.capability } },
  });
} else {
  const { StdioClientTransport } = await load('client/stdio.js');
  transport = new StdioClientTransport({ command: resolve(executable), maxBufferSize: 32768,
    env: { DASHBOARD_ROLE: 'research-mcp-stdio', MCP_SCOPE_FILE: resolve(directory, 'scope.json') } });
}
const client = new Client({ name: 'verified-market-rules-host', version: '1.0.0' });
let checks = 0;
try {
  await client.connect(transport);
  const tools = await client.listTools(); assert.ok(tools.tools.some(t => t.name === 'get_market_rules' && t.annotations.readOnlyHint)); checks++;
  const call = (args = {}) => client.callTool({ name: 'get_market_rules', arguments: args });
  const first = await call(); assert.equal(first.isError, false); checks++;
  const envelope = first.structuredContent; assert.equal(envelope.packet_hash, hash(scope.packet)); assert.equal(envelope.cutoff, scope.packet.cutoff); checks++;
  if (expected === 'missing') {
    assert.equal(envelope.data.status, 'unavailable'); assert.equal(envelope.data.rules, null); assert.equal(envelope.data.rules_hash, null); checks++;
  } else {
    const r = envelope.data.rules; const e = r.evidence; const p = scope.packet; const c = r.configuration;
    assert.equal(envelope.data.status, 'verified_asof_cutoff'); checks++;
    assert.equal(envelope.data.rules_hash, hash(r)); assert.equal(envelope.data.rules_hash, scope.rules_hash); checks++;
    assert.deepEqual(r, scope.market_rules); assert.equal(r.packet_hash, scope.packet_hash); checks++;
    assert.equal(r.source_id, p.source_id); assert.equal(r.source_created_at, p.source_created_at); checks++;
    assert.ok(ns(r.source_created_at) <= ns(p.cutoff)); checks++;
    assert.equal(e.ticker, p.ticker); assert.equal(e.market_type, 'binary'); assert.equal(Number(e.notional_usd), 1); checks++;
    for (const key of ['series', 'series_changes', 'event_changes']) {
      assert.ok(ns(e[key].request_started_at) >= ns(e.market_received_at));
      assert.ok(ns(e[key].request_started_at) <= ns(e[key].received_at));
      assert.ok(ns(e[key].received_at) <= ns(p.received_at));
      assert.ok(ns(e[key].received_at) <= ns(p.source_created_at));
      assert.ok(ns(e[key].received_at) <= ns(p.cutoff)); checks++;
    }
    assert.equal(e.market_request_started_at, p.metadata_request_started_at); assert.equal(e.market_received_at, p.metadata_received_at); checks++;
    for (const key of ['series_changes', 'event_changes']) { assert.ok(e[key].data.every(v => ns(v.at) <= ns(p.cutoff))); checks++; }
    const latest = e.event_changes.data.at(-1);
    assert.equal(c.effective_fee_type, latest?.fee_type ?? e.series.data.fee_type); checks++;
    assert.equal(Number(c.effective_fee_multiplier), Number(latest?.multiplier ?? e.series.data.multiplier)); checks++;
    const ticks = new Set(); let end = 0n;
    for (const band of e.price_ranges) {
      const start = price(band.start), stop = price(band.end), step = price(band.step);
      assert.equal(start, end); assert.ok(step > 0n && stop > start && (stop - start) % step === 0n);
      for (let v = start; v <= stop; v += step) if (v > 0n && v < 10000n) ticks.add(String(v));
      end = stop;
    }
    assert.equal(end, 10000n); assert.equal(ticks.size, c.supported_price_count); checks++;
    for (const side of ['yes_dollars', 'no_dollars']) {
      assert.ok(p.book[side].every(([v]) => ticks.has(String(price(v))) && ticks.has(String(10000n - price(v))))); checks++;
    }
    assert.ok(!/"(result|outcome|expiration_value|future_candidate|markets)":/.test(JSON.stringify(r))); checks++;
    assert.ok(JSON.stringify(envelope).length < 32768); checks++;
  }
  for (const args of [{ asset: 'BTC' }, { cutoff: '2099-01-01T00:00:00Z' }, { phase: 'execution_candidate' }, { run_id: 'other' }, { rules_hash: '0'.repeat(64) }, { event_ticker: 'other' }]) {
    const r = await call(args); assert.equal(r.isError, true); assert.equal(r.structuredContent.data.error_code, 'ASOF_DENIED'); checks++;
  }
  for (const name of ['get_outcome', 'get_run_review', 'advance_clock', 'collect_opening', 'place_order']) {
    const r = await client.callTool({ name, arguments: {} }); assert.equal(r.isError, true); checks++;
  }
  assert.deepEqual((await call()).structuredContent, envelope); checks++;
  console.log(JSON.stringify({ asset: scope.packet.asset, transport: mode, expected, checks, status: 'passed', future_outcomes_and_rule_overrides: 'denied' }));
} finally { await client.close(); }
