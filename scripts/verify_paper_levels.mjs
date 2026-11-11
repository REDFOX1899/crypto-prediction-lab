// Trusted SDK host: compare pure price ceilings to an independent integer oracle.
// Captures/tokens stay private; stdout contains aggregate proof only.
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
const [sdkDirectory, mode, directory, executable] = process.argv.slice(2);
if (!sdkDirectory || !['http', 'stdio'].includes(mode) || !directory || (mode === 'stdio' && !executable)) {
  throw new Error('Usage: node scripts/verify_paper_levels.mjs <SDK dist/esm> <http|stdio> <scope directory> [dashboard executable]');
}
const load = file => import(pathToFileURL(join(resolve(sdkDirectory), file)).href);
const { Client } = await load('client/index.js');
const scope = JSON.parse(readFileSync(join(directory, 'scope.json'), 'utf8'));
const canonical = value => Array.isArray(value) ? value.map(canonical) : value && typeof value === 'object'
  ? Object.fromEntries(Object.keys(value).sort().map(key => [key, canonical(value[key])])) : value;
const hash = value => createHash('sha256').update(JSON.stringify(canonical(value))).digest('hex');
const fixed = (text, scale) => {
  const [whole, fraction = ''] = text.split('.');
  return BigInt(whole) * 10n ** BigInt(scale) + BigInt(fraction.padEnd(scale, '0'));
};
const ceiling = probability => {
  const micro = fixed(probability, 6);
  let answer = null;
  for (let c = 1n; c <= 99n; c++) {
    const raw = 7n * c * (100n - c);
    const fee = ((raw + 9999n) / 10000n) * 10000n;
    if (micro - c * 10000n - fee - 5000n >= 10000n) answer = `0.${String(c).padStart(2, '0')}`;
  }
  return answer;
};
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
const client = new Client({ name: 'paper-level-isolation-verifier', version: '1.0.0' });
let checks = 0;
try {
  await client.connect(transport);
  const tools = await client.listTools();
  assert.ok(tools.tools.some(t => t.name === 'preview_paper_levels' && t.annotations.readOnlyHint)); checks++;
  const call = args => client.callTool({ name: 'preview_paper_levels', arguments: args });
  const args = { packet_hash: scope.packet_hash, quantity: '1', forecast: { kind: 'hypothetical', p_yes: '0.60', lower: '0.56', upper: '0.64' } };
  const first = await call(args);
  assert.equal(first.isError, false); checks++;
  const envelope = first.structuredContent;
  assert.equal(envelope.packet_hash, hash(scope.packet)); assert.equal(envelope.cutoff, scope.packet.cutoff); checks++;
  const preview = envelope.data;
  assert.equal(preview.policy_hash, hash(preview.policy)); checks++;
  assert.equal(preview.scenario_hash, hash({ packet_hash: scope.packet_hash, policy_hash: preview.policy_hash, scenario: preview.forecast_assumption })); checks++;
  assert.equal(preview.paper_order_eligible, false); assert.equal(preview.status, 'conditional_paper_preview'); checks++;
  assert.equal(preview.levels.YES.buy_up_to_usd, ceiling('0.56')); checks++;
  assert.equal(preview.levels.NO.buy_up_to_usd, ceiling('0.36')); checks++;
  for (const side of ['YES', 'NO']) {
    assert.equal(preview.levels[side].eligible_buy_price_usd, null); checks++;
    assert.ok(preview.levels[side].depth_under_cap.ladder.every(level => Number(level.ask_usd) <= Number(preview.levels[side].buy_up_to_usd))); checks++;
  }
  assert.deepEqual(preview.eligibility_reason_codes, ['UNVERIFIED_ASOF_FEE_RULES', 'UNVERIFIED_ASOF_TICK_GRID', 'UNCALIBRATED_PROBABILITY_BOUNDS']); checks++;
  assert.ok(Date.parse(preview.hypothetical_intent_expires_at) > Date.parse(envelope.cutoff)); checks++;
  assert.ok(Date.parse(preview.hypothetical_intent_expires_at) <= (scope.packet.open_ts + 40) * 1000); checks++;
  assert.ok(JSON.stringify(envelope).length < 32768); checks++;
  const benchmark = await call({ ...args, forecast: { kind: 'midpoint_benchmark' } });
  assert.equal(benchmark.isError, false); assert.equal(benchmark.structuredContent.data.forecast_assumption.p_yes, scope.packet.yes_midpoint_usd); checks++;
  const midpoint = fixed(scope.packet.yes_midpoint_usd, 6);
  const no = String(1000000n - midpoint).padStart(7, '0');
  const noProbability = no.slice(0, -6) + '.' + no.slice(-6);
  assert.equal(benchmark.structuredContent.data.levels.YES.buy_up_to_usd, ceiling(scope.packet.yes_midpoint_usd)); checks++;
  assert.equal(benchmark.structuredContent.data.levels.NO.buy_up_to_usd, ceiling(noProbability)); checks++;
  const repeat = await call(args); assert.deepEqual(repeat.structuredContent, envelope); checks++;
  for (const bad of [
    { ...args, packet_hash: '0'.repeat(64) }, { ...args, asset: 'BTC' },
    { ...args, cutoff: '2099-01-01T00:00:00Z' }, { ...args, phase: 'execution_candidate' },
    { ...args, run_id: 'other' }, { ...args, fee_multiplier: '0' },
    { ...args, forecast: { ...args.forecast, calibrated: true } },
    { ...args, forecast: { kind: 'midpoint_benchmark', p_yes: '1' } },
  ]) {
    const result = await call(bad); assert.equal(result.isError, true);
    assert.equal(result.structuredContent.data.error_code, 'ASOF_DENIED'); checks++;
  }
  for (const bad of [
    { ...args, quantity: '0' }, { ...args, quantity: '101' }, { ...args, quantity: '1.5' },
    { ...args, forecast: { ...args.forecast, lower: '0.90' } },
    { ...args, forecast: { ...args.forecast, lower: '-1' } },
  ]) {
    const result = await call(bad); assert.equal(result.isError, true); checks++;
  }
  for (const name of ['get_outcome', 'get_run_review', 'place_order']) {
    const result = await client.callTool({ name, arguments: {} }); assert.equal(result.isError, true); checks++;
  }
  const policy = await client.readResource({ uri: 'research://paper-policy/v1' });
  assert.equal(JSON.parse(policy.contents[0].text).data.version, 'paper_buy_levels_v1'); checks++;
  assert.deepEqual((await call(args)).structuredContent, envelope); checks++;
  console.log(JSON.stringify({ asset: scope.packet.asset, transport: mode, checks, status: 'passed',
    integer_oracle: 'matched', future_outcome_and_policy_overrides: 'denied', paper_order_eligible: false }));
} finally { await client.close(); }
