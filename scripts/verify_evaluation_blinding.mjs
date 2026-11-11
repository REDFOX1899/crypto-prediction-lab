// Trusted SDK host: evaluator reports/outcomes must remain outside blinded MCP.
import {readFileSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {execFileSync} from 'node:child_process';
import assert from 'node:assert/strict';
const [sdkDirectory,directory]=process.argv.slice(2);
if(!sdkDirectory||!directory)throw Error('Usage: node scripts/verify_evaluation_blinding.mjs <SDK dist/esm> <scope directory>');
const load=f=>import(pathToFileURL(join(resolve(sdkDirectory),f)).href);
const {Client}=await load('client/index.js');
const {StreamableHTTPClientTransport}=await load('client/streamableHttp.js');
const scope=JSON.parse(readFileSync(join(directory,'scope.json'),'utf8'));
const capability=JSON.parse(readFileSync(join(directory,'capability.json'),'utf8'));
const token=execFileSync('gcloud',['auth','print-identity-token'],{encoding:'utf8'}).trim();
const client=new Client({name:'prospective-report-blinding-verifier',version:'1.0.0'});
const transport=new StreamableHTTPClientTransport(new URL('https://crypto-research-mcp-308289525742.us-central1.run.app/mcp'),{requestInit:{headers:{Authorization:`Bearer ${token}`,'X-Research-Capability':capability.capability}}});
let checks=0;
try{
 await client.connect(transport);
 const call=(name,args={})=>client.callTool({name,arguments:args});
 const first=(await call('get_cycle_packet')).structuredContent;assert.deepEqual(first.data,scope.packet);assert.equal(first.cutoff,scope.packet.cutoff);checks++;
 const forecast=(await call('get_forecast')).structuredContent;assert.equal(forecast.data.paper_order_eligible,false);checks++;
 for(const name of ['get_evaluation','evaluate_day','get_daily_report','get_summary','get_prior_performance','get_outcome','get_future_snapshot','place_order']){
  const result=await call(name,{day:'2026-11-11'});assert.equal(result.isError,true);assert.equal(result.structuredContent.data.error_code,'ASOF_DENIED');checks++;
 }
 for(const uri of ['research://prospective','research://evaluation/2026-11-11','research://official-results','gs://crypto-prediction-lab-research-308289525742/prospective/']){
  await assert.rejects(client.readResource({uri}));checks++;
 }
 assert.deepEqual((await call('get_cycle_packet')).structuredContent,first);assert.deepEqual((await call('get_forecast')).structuredContent,forecast);checks++;
 console.log(JSON.stringify({asset:scope.packet.asset,status:'passed',checks,evaluation_reports_outcomes_and_future_reads:'denied',original_packet_and_forecast:'unchanged'}));
}finally{await client.close();}
