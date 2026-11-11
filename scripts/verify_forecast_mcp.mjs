// Independent SDK host and floating-point inference oracle; aggregate stdout only.
import { readFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
import assert from 'node:assert/strict';
const [sdkDirectory, mode, directory, executable, expected = 'forecast'] = process.argv.slice(2);
if (!sdkDirectory || !['http', 'stdio'].includes(mode) || !directory || !['forecast', 'missing'].includes(expected)) throw Error('Usage: node scripts/verify_forecast_mcp.mjs <SDK dist/esm> <http|stdio> <scope directory> <binary or -> [forecast|missing]');
const load = file => import(pathToFileURL(join(resolve(sdkDirectory), file)).href);
const { Client } = await load('client/index.js');
const scope = JSON.parse(readFileSync(join(directory, 'scope.json'), 'utf8'));
const ns = s => { const m = /^(.*T\d\d:\d\d:\d\d)(?:\.(\d{1,9}))?Z$/.exec(s); assert.ok(m); return BigInt(Date.parse(m[1] + 'Z')) * 1000000n + BigInt((m[2] || '').padEnd(9, '0')); };
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
const client = new Client({ name: 'forecast-arithmetic-verifier', version: '1.0.0' });
let checks = 0;
const close = (a,b) => { assert.ok(Number.isFinite(a) && Math.abs(a-b) < 1e-11); checks++; };
try {
  await client.connect(transport);
  const tools = await client.listTools(); assert.ok(tools.tools.some(t => t.name === 'get_forecast' && t.annotations.readOnlyHint)); checks++;
  const call = (args = {}) => client.callTool({ name: 'get_forecast', arguments: args });
  const first = await call(); assert.equal(first.isError, false); checks++;
  const envelope = first.structuredContent, data = envelope.data, p = scope.packet;
  assert.equal(envelope.packet_hash, scope.packet_hash); assert.equal(envelope.cutoff, p.cutoff); checks++;
  assert.equal(data.paper_order_eligible, false); checks++;
  if (expected === 'missing') {
    assert.equal(data.status, 'unavailable'); assert.equal(data.reason_code, 'NO_FORECAST_COMMITTED_AT_CUTOFF'); checks++;
  } else {
    const proof = scope.forecast_proof, m = proof.model;
    assert.equal(data.status, 'experimental_forecast'); assert.equal(data.forecast_hash, scope.forecast_hash); checks++;
    assert.equal(data.model_id, proof.object_name); assert.equal(data.model_generation, proof.generation); checks++;
    assert.ok(ns(proof.model_committed_at) <= ns(p.cutoff)); assert.ok(ns(m.calibrate_before) <= ns(proof.model_committed_at)); checks++;
    assert.ok(ns(m.training_max_label_available_at) <= ns(m.train_before)); assert.ok(ns(m.calibration_max_label_available_at) <= ns(m.calibrate_before)); checks++;
    assert.equal(m.embargo_seconds, 1800); assert.deepEqual(m.feature_names, ['midpoint_logit','spread_usd','time_remaining_fraction','native_bid_size_imbalance_top10','btc_intercept','eth_intercept']); checks++;
    const clip = x => Math.max(.000001, Math.min(.999999,x));
    const sigmoid = x => 1/(1+Math.exp(-Math.max(-40,Math.min(40,x))));
    const mid = clip(Number(p.yes_midpoint_usd));
    const yes = p.book.yes_dollars.reduce((a,l) => a + Number(l[1]),0), no = p.book.no_dollars.reduce((a,l) => a + Number(l[1]),0);
    const x = [1,Math.log(mid/(1-mid)),Number(p.spread_usd),(p.open_ts+900 - Number(ns(p.cutoff)/1000000n)/1000)/900,(yes-no)/(yes+no),Number(p.asset==='BTC'),Number(p.asset==='ETH')];
    const standardized = x.map((v,j) => j===0 ? 1 : Math.max(-10,Math.min(10,(v-m.mean[j])/m.scale[j])));
    const z = standardized.reduce((s,v,j) => s+v*m.coefficients[j],0);
    close(data.p_yes_raw,proof.predictions[0]); close(data.p_yes_calibrated,proof.predictions[1]);
    close(data.p_yes_raw,clip(sigmoid(z))); close(data.p_yes_calibrated,clip(sigmoid(m.calibration[0]+m.calibration[1]*z)));
    close(data.baselines.constant_half,.5); close(Number(data.baselines.market_midpoint),Number(p.yes_midpoint_usd));
    close(data.baselines.historical_yes_rate,m.historical_yes_rate[['BTC','ETH','SOL'].indexOf(p.asset)]);
    assert.equal(data.probability_interval,null); assert.equal(data.calibration_status,'fitted_diagnostic_only'); checks++;
    assert.ok(!/"(result|outcome|expiration_value|candidate|coefficients|training_manifest_hash)":/.test(JSON.stringify(envelope))); checks++;
  }
  const resource = await client.readResource({ uri:'research://forecast-policy/v1' }); assert.ok(JSON.stringify(resource).includes('automatic_retraining')); checks++;
  for (const args of [{asset:'ETH'},{cutoff:'2099-01-01T00:00:00Z'},{model_id:'other'},{phase:'execution_candidate'},{run_id:'other'},{outcome:'yes'}]) {
    const r = await call(args); assert.equal(r.isError,true); assert.equal(r.structuredContent.data.error_code,'ASOF_DENIED'); checks++;
  }
  for (const name of ['get_outcome','get_future_snapshot','get_run_review','train_model','place_order']) {
    const r = await client.callTool({ name, arguments:{} }); assert.equal(r.isError,true); checks++;
  }
  assert.deepEqual((await call()).structuredContent,envelope); checks++;
  console.log(JSON.stringify({ asset:p.asset,transport:mode,expected,checks,status:'passed',future_models_labels_and_overrides:'denied' }));
} finally { await client.close(); }
