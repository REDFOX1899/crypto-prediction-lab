// Trusted protocol host. Requires an operator-prepared directory and an installed
// official @modelcontextprotocol/sdk. Prints aggregate checks, never packet data.
import { readFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';

const [sdkDirectory, mode, directory, executable] = process.argv.slice(2);
if (!sdkDirectory || !['http', 'stdio'].includes(mode) || !directory || (mode === 'stdio' && !executable)) {
  throw new Error('Usage: node scripts/verify_research_mcp.mjs <SDK dist/esm directory> <http|stdio> <scope directory> [dashboard executable]');
}
const load = file => import(pathToFileURL(join(resolve(sdkDirectory), file)).href);
const { Client } = await load('client/index.js');
const scope = JSON.parse(readFileSync(join(directory, 'scope.json'), 'utf8'));
const canonical = value => Array.isArray(value) ? value.map(canonical) : value && typeof value === 'object'
  ? Object.fromEntries(Object.keys(value).sort().map(key => [key, canonical(value[key])])) : value;
const hash = value => createHash('sha256').update(JSON.stringify(canonical(value))).digest('hex');
const ns = text => {
  const match = /^(.*T\d\d:\d\d:\d\d)(?:\.(\d{1,9}))?Z$/.exec(text);
  assert.ok(match, 'UTC timestamp required');
  return BigInt(Date.parse(match[1] + 'Z')) * 1000000n + BigInt((match[2] || '').padEnd(9, '0'));
};
let transport;
let headers;
const url = 'https://crypto-research-mcp-308289525742.us-central1.run.app/mcp';
if (mode === 'http') {
  const { StreamableHTTPClientTransport } = await load('client/streamableHttp.js');
  const capability = JSON.parse(readFileSync(join(directory, 'capability.json'), 'utf8'));
  // Operator development token, kept in memory; workload hosts use audience-bound
  // Cloud Run identity tokens and service-only invocation permission.
  const token = execFileSync('gcloud', ['auth', 'print-identity-token'], { encoding: 'utf8' }).trim();
  headers = { Authorization: `Bearer ${token}`, 'X-Research-Capability': capability.capability };
  transport = new StreamableHTTPClientTransport(new URL(url), { requestInit: { headers } });
} else {
  const { StdioClientTransport } = await load('client/stdio.js');
  transport = new StdioClientTransport({ command: resolve(executable),
    env: { DASHBOARD_ROLE: 'research-mcp-stdio', MCP_SCOPE_FILE: resolve(directory, 'scope.json') }, maxBufferSize: 32768 });
}
const client = new Client({ name: 'cutoff-isolation-verifier', version: '1.0.0' });
let checks = 0;
try {
  await client.connect(transport);
  const listed = await client.listTools();
  assert.deepEqual(listed.tools.map(t => t.name).sort(), ['get_book_depth', 'get_cycle_packet', 'preview_paper_levels']); checks++;
  assert.ok(listed.tools.every(t => t.annotations.readOnlyHint === true && t.inputSchema.additionalProperties === false)); checks++;
  const call = (name, args = {}) => client.callTool({ name, arguments: args });
  const first = await call('get_cycle_packet');
  assert.equal(first.isError, false);
  const envelope = first.structuredContent;
  assert.equal(envelope.run_id, scope.run_id); assert.equal(envelope.packet_hash, scope.packet_hash); checks++;
  assert.deepEqual(envelope.data, scope.packet); assert.equal(hash(envelope.data), scope.packet_hash); checks++;
  const packet = envelope.data;
  for (const field of ['source_created_at', 'received_at', 'metadata_received_at', 'book_received_at']) {
    assert.ok(ns(packet[field]) <= ns(packet.cutoff)); checks++;
  }
  assert.ok(ns(packet.cutoff) < (BigInt(packet.open_ts) + 40n) * 1000000000n); checks++;
  assert.ok(packet.source_id.endsWith(`s${packet.open_ts}_${packet.asset}_opening`)); checks++;
  for (const key of ['result', 'outcome', 'expiration_value', 'candidate', 'future_candidate', 'p_yes']) {
    assert.ok(!JSON.stringify(packet).includes(`"${key}"`)); checks++;
  }
  for (const side of ['YES', 'NO']) {
    const result = await call('get_book_depth', { packet_hash: scope.packet_hash, side, quantity: '1' });
    assert.equal(result.isError, false); assert.equal(result.structuredContent.packet_hash, scope.packet_hash); checks++;
  }
  const denials = [
    ['get_cycle_packet', { asset: packet.asset === 'BTC' ? 'ETH' : 'BTC' }],
    ['get_cycle_packet', { cutoff: '2099-01-01T00:00:00Z' }],
    ['get_cycle_packet', { open_time: packet.open_ts + 900 }],
    ['get_cycle_packet', { phase: 'execution_candidate' }],
    ['get_cycle_packet', { run_id: 'other' }],
    ['get_book_depth', { packet_hash: '0'.repeat(64), side: 'YES', quantity: '1' }],
    ['get_outcome', {}], ['get_run_review', {}], ['run_decisions', {}], ['place_order', {}], ['advance_clock', {}],
  ];
  for (const [name, args] of denials) {
    const result = await call(name, args); assert.equal(result.isError, true);
    assert.equal(result.structuredContent.data.error_code, 'ASOF_DENIED'); checks++;
  }
  for (const uri of ['research://outcomes', 'research://execution_candidate', 'file:///etc/passwd']) {
    await assert.rejects(client.readResource({ uri })); checks++;
  }
  const methodology = await client.readResource({ uri: 'research://methodology/v1' });
  assert.ok(JSON.stringify(methodology).length < 32768); checks++;
  assert.deepEqual((await call('get_cycle_packet')).structuredContent, envelope); checks++;
  if (mode === 'http') {
    const rpc = { jsonrpc: '2.0', id: 1, method: 'tools/list' };
    const request = (path, custom = {}) => fetch(path, { method: 'POST',
      headers: { ...headers, Accept: 'application/json, text/event-stream', 'Content-Type': 'application/json', ...custom }, body: JSON.stringify(rpc) });
    for (const path of ['/research/decision', '/research/mcp-scope', '/collect-opening', '/api/outcomes', '/run-decisions']) {
      assert.equal((await request(new URL(path, url))).status, 404); checks++;
    }
    assert.equal((await request(url, { 'X-Research-Capability': '' })).status, 403); checks++;
    assert.equal((await request(url, { 'X-Research-Capability': 'A'.repeat(43) })).status, 403); checks++;
    assert.equal((await request(url, { Origin: 'https://evil.example' })).status, 403); checks++;
    assert.equal((await request(url + '?cutoff=2099-01-01')).status, 403); checks++;
    assert.equal((await request(url, { 'MCP-Session-Id': 'other' })).status, 403); checks++;
    assert.equal((await fetch(url)).status, 403); checks++;
  }
  console.log(JSON.stringify({ asset: packet.asset, transport: mode, checks, status: 'passed', packet_hash_matches: true, future_and_outcome_access: 'denied' }));
} finally {
  await client.close();
}
