/** WSL 实验控制面：真实跨发行版 Agent、已接收任务恢复与备份对账。 */
import {spawn} from 'node:child_process';
import {mkdir,writeFile,readFile} from 'node:fs/promises';
import path from 'node:path';
import https from 'node:https';
import http from 'node:http';
import net from 'node:net';
import {randomBytes,randomUUID} from 'node:crypto';
import assert from 'node:assert/strict';

const root = process.cwd(), args = process.argv.slice(2);
const option = (name, fallback) => args.find(a => a.startsWith('--'+name+'='))?.slice(name.length+3) ?? fallback;
const runId = option('run-id', randomBytes(6).toString('hex'));
assert.match(runId,/^[a-zA-Z0-9-]+$/,'运行标识格式无效');
const distros = option('agent-distros','opsd_c001_lab,opsd_c052_lab,opsd_c061_lab').split(',');
assert.equal(distros.length,3,'需要三个 Agent 发行版');
for (const d of distros) assert.match(d,/^opsd_[a-z0-9_]+$/,'只允许实验发行版');
const hubDistro = process.env.OPSD_LAB_DISTRO || process.env.WSL_DISTRO_NAME;
assert.match(hubDistro ?? '',/^opsd_[a-z0-9_]+$/,'控制面只能运行在 opsd_ 发行版');
const run = path.join(root,'.data','lab-cp-'+runId);
await mkdir(run,{recursive:true});
async function freePort(){ return await new Promise((resolve,reject)=>{const s=net.createServer();s.on('error',reject);s.listen(0,'127.0.0.1',()=>{const port=s.address().port;s.close(()=>resolve(port));});}); }
const base=Number(option('base',await freePort())), agentPort=await freePort(), healthPort=await freePort();
const bin=path.join(root,'target','debug'), hubDir=path.join(run,'hub');
const password=randomBytes(24).toString('hex'), passwordFile=path.join(run,'password');
await writeFile(passwordFile,password,{mode:0o600});
const quote=s=>"'"+String(s).replaceAll("'","'\\''")+"'";
const logs=[], results=[], children=new Set(), units=new Map(), keepers=new Map();
let hub, ca, entrance='', cookie='', csrf='', registered=[], successfulTask, current='准备';
const output = text => (args.includes('--json') ? process.stderr : process.stdout).write(text+'\n');
function record(name,status,detail,command='',distro=hubDistro,resource=''){
  results.push({name,status,exitCode:status==='PASS'?0:1,detail:String(detail).slice(0,2000),command,distro,resource});
  output('['+status+'] '+name+'：'+String(detail).slice(0,200));
}
function execute(program,argv,{input,timeout=120000,allowFailure=false}={}){
  return new Promise((resolve,reject)=>{
    const child=spawn(program,argv,{cwd:root,stdio:['pipe','pipe','pipe'],windowsHide:true});
    children.add(child);let stdout='',stderr='',settled=false;
    const finish=(error,result)=>{if(settled)return;settled=true;clearTimeout(timer);children.delete(child);error?reject(error):resolve(result);};
    const timer=setTimeout(()=>{child.kill('SIGKILL');finish(new Error('命令超时：'+program));},timeout);
    child.stdout.on('data',b=>stdout+=b);child.stderr.on('data',b=>stderr+=b);
    child.on('error',e=>finish(e));child.on('close',(code,signal)=>{
      const result={code,signal,stdout,stderr};logs.push(program+' '+argv.slice(0,3).join(' ')+'\n'+stdout+stderr);
      if(code===0||allowFailure)finish(null,result);else finish(new Error('命令失败，退出码 '+code+' 信号 '+signal+'：'+program+'\n'+stderr.slice(-2000)+stdout.slice(-1000)));
    });
    child.stdin.end(input);
  });
}
async function remote(distro,script,options={}){
  const encoded=Buffer.from('set -euo pipefail\n'+script+'\n').toString('base64');
  return execute('/mnt/c/Windows/System32/wsl.exe',['-d',distro,'-u','root','--','bash','-c','printf %s '+encoded+' | base64 -d | bash -s'],options);
}
const command=(name,argv)=>execute(path.join(bin,name),argv);
async function agentCommand(node,argv){return remote(node.distro,[path.join(bin,'opsd-agent'),...argv].map(quote).join(' '));}
async function keepDistro(distro){
  if(keepers.has(distro))return;
  const child=spawn('/mnt/c/Windows/System32/wsl.exe',['-d',distro,'-u','root','--','bash','-c','printf "就绪\n"; exec sleep infinity'],{cwd:root,stdio:['ignore','pipe','pipe'],windowsHide:true});
  keepers.set(distro,child);children.add(child);
  await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(new Error('发行版驻留启动超时：'+distro)),30000);child.once('error',e=>{clearTimeout(timer);reject(e);});child.once('exit',code=>{clearTimeout(timer);reject(new Error('发行版驻留提前退出：'+distro+' '+code));});child.stdout.once('data',()=>{clearTimeout(timer);resolve();});});
}
async function startAgent(node){
  const unit='opsd-test-'+runId+'-agent-'+node.index;
  await remote(node.distro,['systemd-run','--quiet','--no-block','--collect','--unit='+unit,'--property=Type=exec','--property=Environment=RUST_LOG=info','--',path.join(bin,'opsd-agent'),'--data-dir',node.dir,'run'].map(quote).join(' '));
  units.set(unit,node);return unit;
}
async function stopAgent(node){
  const unit='opsd-test-'+runId+'-agent-'+node.index;
  const result=await remote(node.distro,'journalctl --no-pager -u '+quote(unit)+' -n 80; systemctl stop '+quote(unit),{allowFailure:true});
  logs.push(result.stdout+result.stderr);units.delete(unit);
}
async function stopHub(){
  if(!hub)return;const child=hub;hub=null;
  if(child.exitCode!==null||child.signalCode!==null)return;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('close',()=>{clearTimeout(timer);children.delete(child);resolve();});child.kill('SIGTERM');});
}
async function startHub(dir){
  hub=spawn(path.join(bin,'opsd-hub'),['--data-dir',dir,'serve','--listen','127.0.0.1:'+base,'--agent-listen','0.0.0.0:'+agentPort,'--health-listen','127.0.0.1:'+healthPort,'--origin','https://localhost:'+base],{cwd:root,stdio:['ignore','pipe','pipe']});
  children.add(hub);hub.on('error',e=>logs.push(String(e)));hub.stdout.on('data',b=>logs.push(String(b)));hub.stderr.on('data',b=>logs.push(String(b)));
  await wait(async()=>{const r=await httpRequest('/api/v1/health',healthPort);return r.status===200&&r.data.status==='ok';},'主控健康检查');
}
function httpRequest(route,port=base,method='GET',body){
  return new Promise((resolve,reject)=>{
    const data=body===undefined?undefined:JSON.stringify(body),health=port===healthPort;
    const req=(health?http:https).request({hostname:'127.0.0.1',port,path:route,method,ca,headers:health?{}:{Origin:'https://localhost:'+base,Cookie:cookie,'X-CSRF-Token':csrf,...(data?{'Content-Type':'application/json','Content-Length':Buffer.byteLength(data)}:{})}},res=>{
      let text='';res.on('data',b=>text+=b);res.on('end',()=>{let value;try{value=JSON.parse(text);}catch{value=text;}resolve({status:res.statusCode,data:value,headers:res.headers});});res.on('error',reject);
    });req.on('error',reject);req.setTimeout(5000,()=>req.destroy(new Error('HTTP 请求超时')));if(data)req.write(data);req.end();
  });
}
const request=(route,method='GET',body)=>httpRequest('/'+entrance+'/api/v1'+route,base,method,body);
async function wait(fn,label,timeout=60000){
  const deadline=Date.now()+timeout;let error='';while(Date.now()<deadline){try{if(await fn())return;}catch(e){error=e.message;}await new Promise(r=>setTimeout(r,300));}throw new Error(label+'超时：'+error+'\n'+logs.join('').slice(-1200));
}
async function login(){cookie='';csrf='';const r=await request('/auth/login','POST',{password});assert.equal(r.status,200,'登录应成功');cookie=r.headers['set-cookie'][0].split(';')[0];csrf=r.data.csrf;assert.ok(csrf);}
async function nodes(){const r=await request('/nodes');assert.equal(r.status,200);assert.ok(Array.isArray(r.data));for(const n of r.data)assert.equal(typeof n.node?.id,'string');return r.data;}
const find=(rows,id)=>rows.find(n=>n.node.id===id);
async function stage(name,fn,dependencies=[]){
  current=name;if(dependencies.some(dep=>!results.some(r=>r.name===dep&&r.status==='PASS'))){record(name,'NOT_RUN','前置场景没有通过');return;}
  try{await fn();}catch(e){record(name,'FAIL',e.message,'node tests/lab/control-plane.mjs');}
}
// 夹具只允许修改本次实验目录中的 SQLite；模拟已授权、已落盘但尚未回传的任务。
async function fixture(script,input={}){
  const src=Buffer.from(script).toString('base64');
  return execute('python3',['-c','import base64;exec(base64.b64decode('+JSON.stringify(src)+'))'],{input:JSON.stringify(input)});
}
const seedFixture=String.raw`
import json,sys,sqlite3,pathlib
x=json.load(sys.stdin)
root=pathlib.Path(x['run']).resolve()
assert root.name.startswith('lab-cp-') and root.parent.name=='.data'
for name in [x['agent_db'],x['hub_db']]:
    p=pathlib.Path(name).resolve()
    assert root in p.parents and p.name in ['node.db','control.db']
    con=sqlite3.connect(p)
    for t in x['tasks']:
        con.execute('INSERT INTO tasks(id,node_id,task_key,digest,value) VALUES(?,?,?,?,?)',(t['id'],t['node_id'],t['key'],t['digest'],json.dumps(t,separators=(',',':'))))
    con.commit();con.close()
print('已在两端写入限定实验任务')
`;
async function localTasks(node){return JSON.parse((await fixture('import sqlite3,json,sys\nx=json.load(sys.stdin)\nc=sqlite3.connect(x["db"])\nprint(json.dumps([json.loads(r[0]) for r in c.execute("SELECT value FROM tasks")]))',{db:path.join(node.dir,'node.db')})).stdout);}

try {
await stage('初始化与管理员认证',async()=>{
  const r=await command('opsd-hub',['--data-dir',hubDir,'init','--password-file',passwordFile]);
  entrance=r.stdout.match(/控制台安全入口：([A-Za-z0-9\-._~]{16})/)?.[1];assert.ok(entrance);
  ca=await readFile(path.join(hubDir,'pki','ca.pem'));await startHub(hubDir);
  assert.equal((await request('/nodes')).status,401);await login();
  record(current,'PASS','真实 CA、入口路径、管理员会话和 CSRF 验证成功','opsd-hub init/serve + POST /auth/login');
});
await stage('跨发行版注册与 mTLS 心跳',async()=>{
  for(let i=0;i<3;i++){
    const distro=distros[i],r=await request('/enrollment-tokens','POST',{name:distro,public_addresses:[],overlay_address:'127.0.0.1',ssh_port:22});assert.equal(r.status,200);
    const node={distro,index:i,nodeId:r.data.node_id,dir:path.join(run,'agent-'+i)},token=path.join(run,'token-'+i);
    registered.push(node);await keepDistro(distro);await writeFile(token,r.data.token,{mode:0o600});
    await agentCommand(node,['--data-dir',node.dir,'enroll','--hub','https://localhost:'+base,'--agent-url','wss://localhost:'+agentPort+'/agent','--ca',path.join(hubDir,'pki','ca.pem'),'--fingerprint',r.data.ca_fingerprint,'--token-file',token]);
    await startAgent(node);
  }
  await wait(async()=>{const rows=await nodes();return registered.every(n=>find(rows,n.nodeId)?.connected===true);},'三个独立发行版 Agent 在线');
  const before=await nodes();
  await wait(async()=>{const rows=await nodes();return registered.every(n=>find(rows,n.nodeId)?.node.last_seen>find(before,n.nodeId).node.last_seen);},'新的心跳时间');
  for(const n of registered)record(current+' '+n.distro,'PASS','Agent 由该发行版 systemd 执行，mTLS 在线且 last_seen 更新','systemd-run opsd-agent run + GET /nodes',n.distro,'node_id='+n.nodeId);
  record(current,'PASS','三个真实 Agent 跨发行版连接','opsd-agent enroll/run');
},['初始化与管理员认证']);
await stage('证书续期重连',async()=>{
  const n=registered[0],p=path.join(n.dir,'client.pem'),before=await readFile(p,'utf8');await stopAgent(n);
  await agentCommand(n,['--data-dir',n.dir,'renew-certificate']);assert.notEqual(await readFile(p,'utf8'),before);await startAgent(n);
  await wait(async()=>find(await nodes(),n.nodeId)?.connected===true,'续期后重连');
  record(current,'PASS','证书正文已变化，Agent 用新证书重连','opsd-agent renew-certificate + run',n.distro,'node_id='+n.nodeId);
},['跨发行版注册与 mTLS 心跳']);
await stage('任务幂等与事件序号',async()=>{
  const n=registered[0],key='lab-'+runId;
  const first=await request('/nodes/'+n.nodeId+'/actions','POST',{idempotency_key:key,action:{type:'inspect'}});assert.equal(first.status,202);assert.ok(first.data.task_id);
  const again=await request('/nodes/'+n.nodeId+'/actions','POST',{idempotency_key:key,action:{type:'inspect'}});assert.equal(again.status,202);assert.equal(again.data.task_id,first.data.task_id);
  assert.equal((await request('/nodes/'+n.nodeId+'/actions','POST',{idempotency_key:key,action:{type:'pull_image',reference:'different'}})).status,400);
  await wait(async()=>(await request('/tasks/'+first.data.task_id)).data.status==='succeeded','任务完成');
  const t=(await request('/tasks/'+first.data.task_id)).data;assert.ok(t.sequence>=3);
  const events=(await request('/tasks/'+t.id+'/events')).data;assert.ok(Array.isArray(events)&&events.length>0);assert.equal(new Set(events.map(e=>e.sequence)).size,events.length);
  const persisted=(await localTasks(n)).filter(x=>x.key===key);assert.equal(persisted.length,1);assert.equal(persisted[0].id,t.id);
  await new Promise(r=>setTimeout(r,6500));assert.equal((await request('/tasks/'+t.id)).data.sequence,t.sequence,'重复调度不应重新执行');
  successfulTask=t;record(current,'PASS','两端仅一条任务，冲突参数被拒绝，重复调度后事件序号不变','POST /nodes/{id}/actions + GET /tasks',n.distro,'task_id='+t.id);
},['证书续期重连']);
await stage('离线已接收任务恢复与不确定结果',async()=>{
  const n=registered[1];await stopAgent(n);await wait(async()=>find(await nodes(),n.nodeId)?.connected===false,'Agent 离线');
  const queued=await request('/nodes/'+n.nodeId+'/actions','POST',{idempotency_key:'offline-'+runId,action:{type:'peer_probe_targets',set:{version:1,targets:[]}}});assert.equal(queued.status,202,'离线操作应先持久化并返回任务编号');assert.ok(queued.data.task_id);
  const queuedTask=(await request('/tasks/'+queued.data.task_id)).data;assert.equal(queuedTask.status,'pending','离线任务应保持待执行状态');
  await stopHub();
  const now=Math.floor(Date.now()/1000),offlineAction={type:'peer_probe_targets',set:{version:1,targets:[]}},baseTask={...successfulTask,node_id:n.nodeId,action:offlineAction,result:null,error:null,created_at:now,updated_at:now};
  const accepted={...baseTask,id:randomUUID(),key:'accepted-'+runId,status:'accepted',sequence:1};
  const running={...baseTask,id:randomUUID(),key:'running-'+runId,status:'running',sequence:2};
  await fixture(seedFixture,{run,agent_db:path.join(n.dir,'node.db'),hub_db:path.join(hubDir,'control.db'),tasks:[accepted,running]});
  await startAgent(n);await wait(async()=>{const rows=await localTasks(n);return rows.find(t=>t.id===accepted.id)?.status==='succeeded'&&rows.find(t=>t.id===running.id)?.status==='uncertain';},'主控离线时本地任务恢复',120000);
  const local=await localTasks(n),a=local.find(t=>t.id===accepted.id),u=local.find(t=>t.id===running.id);assert.ok(a.sequence>1&&u.sequence>2);
  await startHub(hubDir);await login();
  await wait(async()=>{const at=(await request('/tasks/'+a.id)).data,ut=(await request('/tasks/'+u.id)).data;return at.status==='succeeded'&&at.sequence===a.sequence&&ut.status==='uncertain'&&ut.sequence===u.sequence&&(await request('/tasks/'+queued.data.task_id)).data.status==='succeeded';},'两端任务对账');
  record(current,'PASS','主控离线：accepted 实际执行；running 进入 uncertain；重连后两端状态与序号一致。夹具仅模拟落盘记录，不证明网络接收的故障时序。','停止 Hub/Agent → 写入实验夹具 → 重启 Agent → 恢复 Hub',n.distro,'task_id='+a.id+','+u.id);
},['任务幂等与事件序号']);
await stage('节点撤销拒绝重连',async()=>{
  if(!hub){await startHub(hubDir);await login();}
  const n=registered[2];assert.equal((await request('/nodes/'+n.nodeId,'DELETE')).status,200);await stopAgent(n);await startAgent(n);
  await wait(async()=>{const r=await remote(n.distro,'journalctl --no-pager -u '+quote('opsd-test-'+runId+'-agent-'+n.index)+' -n 40');return r.stdout.includes('400 Bad Request');},'被撤销证书的连接拒绝证据');
  const entry=find(await nodes(),n.nodeId);assert.ok(entry);assert.equal(entry.node.revoked,true);assert.equal(entry.connected,false);
  await stopAgent(n);record(current,'PASS','保留 revoked=true 记录，Agent 实际重连收到 400 且未上线','DELETE /nodes/{id} + Agent 重连日志',n.distro,'node_id='+n.nodeId);
},['跨发行版注册与 mTLS 心跳']);
await stage('备份恢复与旧任务不重复执行',async()=>{
  await stopHub();const backup=path.join(run,'backup'),restored=path.join(run,'restored');
  await command('opsd-hub',['--data-dir',hubDir,'backup','--destination',backup]);await command('opsd-hub',['--data-dir',restored,'restore','--source',backup]);
  assert.deepEqual(await readFile(path.join(restored,'pki','ca.pem')),ca);await startHub(restored);cookie='';csrf='';assert.equal((await request('/auth/me')).status,401);await login();
  const live=registered.slice(0,2);await wait(async()=>{const rows=await nodes();return live.every(n=>find(rows,n.nodeId)?.connected===true);},'恢复后存活 Agent 重连');
  await new Promise(r=>setTimeout(r,6500));const t=(await request('/tasks/'+successfulTask.id)).data;assert.equal(t.status,'succeeded');assert.equal(t.sequence,successfulTask.sequence);
  record(current,'PASS','CA、入口和管理员身份恢复，两台存活 Agent 重连，已完成任务未重新执行','opsd-hub backup/restore/serve + GET /tasks',hubDistro,'task_id='+t.id);
},['离线已接收任务恢复与不确定结果','节点撤销拒绝重连']);
} finally {
  for(const n of [...units.values()]){try{await stopAgent(n);}catch(e){record('清理 '+n.distro,'FAIL',e.message);}}
  await stopHub();for(const c of children)c.kill('SIGKILL');
  await writeFile(path.join(run,'control-plane.log'),logs.join('\n'));
}
const summary={runId,run,results,passed:results.filter(r=>r.status==='PASS').length,failed:results.filter(r=>r.status==='FAIL').length,notRun:results.filter(r=>r.status==='NOT_RUN').length};
await writeFile(path.join(run,'results.json'),JSON.stringify(summary,null,2));
console.log(JSON.stringify(summary,null,2));process.exitCode=summary.failed?1:summary.notRun?2:0;
