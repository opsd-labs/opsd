import {spawn} from 'node:child_process';
import {mkdir,writeFile,readFile,unlink} from 'node:fs/promises';
import path from 'node:path';
import https from 'node:https';
import http from 'node:http';
import {makeZip} from './lib/theme-zip.mjs';
import {randomBytes,createHash} from 'node:crypto';
import assert from 'node:assert/strict';
import {loadContract} from './lib/http-assert.mjs';
const root=process.cwd(),run=path.join(root,'.data','integration-'+Date.now());await mkdir(run,{recursive:true});
const bin=path.join(root,'target','debug');const exe=name=>path.join(bin,name+(process.platform==='win32'?'.exe':''));
const hubDir=path.join(run,'hub'),agentDir=path.join(run,'agent');const password=randomBytes(24).toString('hex');const passwordFile=path.join(run,'password');await writeFile(passwordFile,password,{mode:0o600});
async function command(name,args){return new Promise((resolve,reject)=>{const child=spawn(name==='docker'?'docker':exe(name),args,{cwd:root,windowsHide:true});let output='';child.stdout.on('data',b=>output+=b);child.stderr.on('data',b=>output+=b);child.on('error',reject);child.on('exit',code=>code===0?resolve(output):reject(new Error(output)));});}
const initOutput=await command('opsd-hub',['--data-dir',hubDir,'init','--password-file',passwordFile]);
// 安全入口由初始化生成并打印，控制台只响应带入口的地址。
const entrance=initOutput.match(/控制台安全入口：([A-Za-z0-9\-._~]{16})/)?.[1];
assert.ok(entrance,'初始化输出中应包含 16 位安全入口');
const ca=await readFile(path.join(hubDir,'pki','ca.pem'));let cookie='',csrf='';let base=19043,healthPort=19534;
/** 不带安全入口的根路径请求：用于验证门禁丢弃，以及 Agent 豁免与分享面。 */
function ungated(route,method='GET',body,options={}){return new Promise(resolve=>{const data=body===undefined?undefined:JSON.stringify(body);const req=https.request({hostname:'localhost',port:base,path:route,method,ca,headers:{Origin:`https://localhost:${base}`,...(data?{'Content-Type':'application/json','Content-Length':Buffer.byteLength(data)}:{}),...(options.headers||{})}},res=>{let text='';res.on('data',b=>text+=b);res.on('end',()=>{let value;try{value=JSON.parse(text)}catch{value=text}resolve({status:res.statusCode,data:value,headers:res.headers});});});req.on('error',e=>resolve({error:e.code||e.message}));req.setTimeout(5000,()=>req.destroy(new Error('请求超时')));if(data)req.write(data);req.end();});}
/** 明文回环健康检查，不经安全入口。 */
function health(){return new Promise(resolve=>{const req=http.request({hostname:'127.0.0.1',port:healthPort,path:'/api/v1/health'},res=>{let text='';res.on('data',b=>text+=b);res.on('end',()=>resolve({status:res.statusCode,text}));});req.on('error',e=>resolve({error:e.code||e.message}));req.setTimeout(5000,()=>req.destroy(new Error('请求超时')));req.end();});}
const contract=await loadContract(path.join(root,'docs','api','openapi.yaml'));
const {assertSchema,responseSchema,assertHttpResponse}=contract;
/** 原始请求：可改 Content-Type / 发非法 JSON；headers 带 content-type。 */
function raw(route,method,body,contentType){
 return new Promise(resolve=>{
  const data=body;
  const req=https.request({hostname:'localhost',port:base,path:`/${entrance}${route}`,method:method||'POST',ca,headers:{Origin:`https://localhost:${base}`,Cookie:cookie,'X-CSRF-Token':csrf,...(data!==undefined?{'Content-Type':contentType||'application/json','Content-Length':Buffer.byteLength(data)}:{})}},res=>{
   let text='';res.on('data',b=>text+=b);res.on('end',()=>{let value;try{value=JSON.parse(text)}catch{value=text}resolve({status:res.statusCode,data:value,text,headers:res.headers});});
  });
  req.on('error',e=>resolve({error:e.message}));
  req.setTimeout(5000,()=>req.destroy());
  if(data!==undefined)req.write(data);
  req.end();
 });
}
function request(route,method='GET',body,options={}){return new Promise((resolve,reject)=>{const data=body===undefined?undefined:JSON.stringify(body);const req=https.request({hostname:'localhost',port:base,path:`/${entrance}/api/v1`+route,method,ca,...options,headers:{Origin:`https://localhost:${base}`,Cookie:cookie,'X-CSRF-Token':csrf,...(data?{'Content-Type':'application/json','Content-Length':Buffer.byteLength(data)}:{}),...options.headers}},res=>{let text='';res.on('data',b=>text+=b);res.on('end',()=>{let value;try{value=JSON.parse(text)}catch{value=text}resolve({status:res.statusCode,data:value,headers:res.headers,cookie:res.headers['set-cookie']?.[0]});});});req.on('error',reject);req.setTimeout(5000,()=>req.destroy(new Error('请求超时')));if(data)req.write(data);req.end();});}
/** 上传主题包等原始字节；需要会话与 CSRF。 */
function upload(route,bytes){return new Promise((resolve,reject)=>{const req=https.request({hostname:'localhost',port:base,path:route,method:'POST',ca,headers:{Origin:`https://localhost:${base}`,Cookie:cookie,'X-CSRF-Token':csrf,'Content-Type':'application/zip','Content-Length':bytes.length}},res=>{let text='';res.on('data',b=>text+=b);res.on('end',()=>{let value;try{value=JSON.parse(text)}catch{value=text}resolve({status:res.statusCode,data:value,headers:res.headers});});});req.on('error',reject);req.setTimeout(10000,()=>req.destroy(new Error('请求超时')));req.write(bytes);req.end();});}

let hub,agent,etFixture;let portainerOwned=false;let fixtureOwned=false;const logs=[];
function launch(name,args){const c=spawn(exe(name),args,{cwd:root,windowsHide:true});c.stdout.on('data',b=>logs.push(b.toString()));c.stderr.on('data',b=>logs.push(b.toString()));return c;}
async function wait(fn,label){for(let i=0;i<80;i++){try{if(await fn())return}catch{}await new Promise(r=>setTimeout(r,200));}throw new Error(`${label} 超时\n${logs.join('').slice(-4000)}`);}
async function stop(c){if(!c||c.exitCode!==null)return;const done=new Promise(r=>c.on('exit',r));c.kill();await done;}
const serveArgs=dir=>['--data-dir',dir,'serve','--listen',`127.0.0.1:${base}`,'--agent-listen','127.0.0.1:19444','--health-listen',`127.0.0.1:${healthPort}`,'--origin',`https://localhost:${base}`];
async function ownStatus(withIdentity=true){
 const identity=withIdentity?{cert:await readFile(path.join(agentDir,'client.pem')),key:await readFile(path.join(agentDir,'client-key.pem'))}:{};
 return new Promise(resolve=>{const req=https.request({hostname:'localhost',port:19444,path:'/status',ca,...identity},res=>{let text='';res.on('data',b=>text+=b);res.on('end',()=>resolve({status:res.statusCode,data:JSON.parse(text)}));});req.on('error',e=>resolve({error:e.code||e.message}));req.setTimeout(5000,()=>req.destroy(new Error('请求超时')));req.end();});
}
// 直接执行包内节点程序，验证已有路径的实际持久恢复与禁止自动换盘。
async function siloNode(operation,body,directory){
 return new Promise((resolve,reject)=>{
  const child=spawn(exe('opsd-plugin-silo'),['node'],{cwd:root,env:{...process.env,OPSD_PLUGIN_NODE_DIR:directory},windowsHide:true});let stdout='',stderr='';
  child.stdout.on('data',b=>stdout+=b);child.stderr.on('data',b=>stderr+=b);child.on('error',reject);child.on('exit',code=>{if(code!==0)return reject(new Error(stderr));try{resolve(JSON.parse(stdout))}catch(error){reject(error)}});
  child.stdin.end(JSON.stringify({context:{plugin:'silo',version:'0.1.0',config:{},secrets:{},task_id:null},method:'EXECUTE',path:operation,body}));
 });
}
if(process.platform==='linux'){
 const data=path.join(run,'existing-storage'),state=path.join(run,'silo-node');await mkdir(data);await mkdir(state);await writeFile(path.join(data,'existing-file'),'已有内容');
 const selection={cluster:'integration',selections:[{mode:'existing',path:data}],confirmed:false};
 const prepared=await siloNode('storage.prepare',selection,state);assert.equal(prepared.status,200);assert.equal(await readFile(path.join(data,'existing-file'),'utf8'),'已有内容');
 await mkdir(path.join(data,'.minio.sys'));
 const restored=await siloNode('storage.prepare',selection,state);assert.equal(restored.status,200);assert.deepEqual(restored.body,prepared.body,'新进程应复用持久目录记录');
 const invalid=await siloNode('storage.prepare',{cluster:'invalid',selections:[{mode:'new',device:'/dev/opsd-nonexistent',expected_size:123,mount:path.join(run,'missing-disk')}],confirmed:true},state);assert.equal(invalid.status,400);assert.equal(await readFile(path.join(data,'existing-file'),'utf8'),'已有内容');
}
try{
 hub=launch('opsd-hub',serveArgs(hubDir));
 await wait(async()=> (await health()).status===200,'主控启动');
 // 门禁：不带入口、错误入口、以及正确入口的前缀截断都必须被丢弃，且不产生 HTTP 响应。
 // 只改首字符即可得到长度相同但必然不同的入口。
 const wrongEntrance=(entrance[0]==='A'?'B':'A')+entrance.slice(1);
 for(const route of ['/','/api/v1/auth/me','/api/v1/nodes',`/${entrance.slice(0,15)}/api/v1/auth/me`,`/${wrongEntrance}/api/v1/auth/me`]){
  const dropped=await ungated(route);
  assert.equal(dropped.status,undefined,`${route} 不应返回 HTTP 响应，实际 ${dropped.status}`);
  assert.ok(dropped.error,`${route} 应直接断开连接`);
 }
 assert.equal((await request('/nodes')).status,401);
 assert.equal((await request('/auth/login','POST',{password},{headers:{Origin:'https://evil.invalid'}})).status,403);
 const unauthorized=await request('/nodes');
 assertHttpResponse(unauthorized,{status:401,route:'/api/v1/nodes',method:'get',kind:'json',schema:responseSchema('/api/v1/nodes','get',401),contentType:'application/json'});
 const login=await request('/auth/login','POST',{password});
 assertHttpResponse(login,{status:200,route:'/api/v1/auth/login',method:'post',kind:'json',schema:responseSchema('/api/v1/auth/login','post',200),contentType:'application/json'});
 cookie=login.cookie.split(';')[0];csrf=login.data.csrf;
 const badPassword=await request('/auth/login','POST',{password:'wrong-password'});
 assertHttpResponse(badPassword,{status:401,route:'/api/v1/auth/login',method:'post',kind:'json',schema:responseSchema('/api/v1/auth/login','post',401),contentType:'application/json'});
 const badJson=await raw('/api/v1/auth/login','POST','{not json','application/json');
 assertHttpResponse(badJson,{status:400,route:'/api/v1/auth/login',method:'post',kind:'plain',schema:responseSchema('/api/v1/auth/login','post',400,'text/plain'),contentType:'text/plain'});
 const badCt=await raw('/api/v1/auth/login','POST',JSON.stringify({password}),'text/plain');
 assertHttpResponse(badCt,{status:415,route:'/api/v1/auth/login',method:'post',kind:'plain',schema:responseSchema('/api/v1/auth/login','post',415,'text/plain'),contentType:'text/plain'});
 const unknownField=await request('/auth/login','POST',{password,nope:1});
 assertHttpResponse(unknownField,{status:422,route:'/api/v1/auth/login',method:'post',kind:'plain',schema:responseSchema('/api/v1/auth/login','post',422,'text/plain'),contentType:'text/plain'});
 assert.equal((await request('/enrollment-tokens','POST',{name:'测试节点',public_addresses:[],ssh_port:22},{headers:{'X-CSRF-Token':'wrong'}})).status,403);
 const registration=await request('/enrollment-tokens','POST',{name:'测试节点',public_addresses:[],overlay_address:null,ssh_port:22});assert.equal(registration.status,200);
 assertSchema(registration.data,responseSchema('/api/v1/enrollment-tokens','post',200));
 assert.equal(registration.headers['cache-control'],'no-store');
 assert.equal(registration.data.installation,null,'未配置发行元数据的开发构建不生成安装信息');
 const pending=await request('/enrollment-tokens');assert.equal(pending.status,200);assert.equal(pending.data.some(e=>e.node_id===registration.data.node_id&&e.status==='pending'),true);assert.equal(JSON.stringify(pending.data).includes(registration.data.token),false);
 const tokenFile=path.join(run,'token');await writeFile(tokenFile,registration.data.token,{mode:0o600});
 await command('opsd-agent',['--data-dir',agentDir,'enroll','--hub',`https://localhost:${base}`,'--agent-url','wss://localhost:19444/agent','--ca',path.join(hubDir,'pki','ca.pem'),'--fingerprint',registration.data.ca_fingerprint,'--token-file',tokenFile]);
 const afterEnroll=await request('/enrollment-tokens');assert.equal(afterEnroll.data.some(e=>e.node_id===registration.data.node_id),false);
 // Agent 豁免路径挂在入口之外，仍然受一次性令牌保护：无效 CSR 必须被拒绝。
 const badEnroll=await ungated('/api/v1/enroll','POST',{token:registration.data.token,csr:'invalid'});
 assert.equal(badEnroll.status,401,`无效 CSR 应被拒绝，实际 ${badEnroll.status}`);
 const oldCertificate=await readFile(path.join(agentDir,'client.pem'),'utf8');
 await command('opsd-agent',['--data-dir',agentDir,'renew-certificate']);
 assert.notEqual(await readFile(path.join(agentDir,'client.pem'),'utf8'),oldCertificate);
 assert.ok((await ownStatus(false)).error,'没有客户端证书不能读取 Agent 在线状态');
 assert.equal((await ungated('/status')).status,undefined,'浏览器监听不得公开 Agent 状态');
 const disconnected=await ownStatus();assert.equal(disconnected.status,200);assert.equal(disconnected.data.connected,false);
 assertSchema(disconnected.data,responseSchema('/status','get',200));
 agent=launch('opsd-agent',['--data-dir',agentDir,'run']);
 await wait(async()=> (await request('/nodes')).data?.[0]?.connected,'Agent mTLS 注册连接');
 const node=registration.data.node_id;
 const connectedStatus=await ownStatus();assert.equal(connectedStatus.status,200);assert.equal(connectedStatus.data.node_id,node);assert.equal(connectedStatus.data.connected,true);
 const first=await request(`/nodes/${node}/actions`,'POST',{idempotency_key:'once',action:{type:'inspect'}});assert.equal(first.status,202);
 const duplicate=await request(`/nodes/${node}/actions`,'POST',{idempotency_key:'once',action:{type:'inspect'}});assert.equal(duplicate.data.task_id,first.data.task_id);
 assert.equal((await request(`/nodes/${node}/actions`,'POST',{idempotency_key:'once',action:{type:'pull_image',reference:'nginx:stable'}})).status,400);
 await wait(async()=> (await request(`/tasks/${first.data.task_id}`)).data.status==='succeeded','采集任务完成');
 const resultRes=await request(`/tasks/${first.data.task_id}`);
 assertHttpResponse(resultRes,{status:200,route:'/api/v1/tasks/{id}',method:'get',kind:'json',schema:responseSchema('/api/v1/tasks/{id}','get',200),contentType:'application/json'});
 const result=resultRes.data;
 assert.ok(result.sequence>=3);
 const peerRes=await request('/peer-addresses');
 assertHttpResponse(peerRes,{status:200,route:'/api/v1/peer-addresses',method:'get',kind:'json',schema:responseSchema('/api/v1/peer-addresses','get',200),contentType:'application/json'});
 assert.ok(Array.isArray(peerRes.data.addresses),'addresses 应为数组');
 const peerVersion=peerRes.data.version;
 const beforeDirectory=(await request('/network')).data.directory;
 const rename=()=>request(`/nodes/${node}`,'PUT',{name:'修改名称',install_mode:'host'});
 assert.equal((await rename()).status,200);
 assert.equal((await request('/network')).data.directory.revision,beforeDirectory.revision,'改名不改变网络目录');
 assert.equal((await request(`/nodes/${node}`,'PUT',{name:'错误手填地址',public_addresses:['8.8.8.8']})).status,400);
 const networkState=await request('/network');
 assertSchema(networkState.data,responseSchema('/api/v1/network','get',200));
 assert.equal(networkState.data.nodes[0].connections.paths.length,2);
 assert.equal((await request('/network','PUT',{node_id:'unknown-node'})).status,400);
 assert.equal((await request('/network','PUT',{node_id:node})).status,200);
 // 设置页可以读改入口，非法值必须被拒绝。
 assert.equal((await request('/settings/entrance')).data.value,entrance);
 assert.equal((await request('/settings/entrance','PUT',{value:'too-short'})).status,400);
 // 指标接口：总览恒可用；历史查询要校验区间与步长。
 const overview=await request('/metrics/overview');
 assert.equal(overview.status,200);
 assert.ok(Array.isArray(overview.data.nodes));
 assert.ok(overview.data.clock_skew_warn_seconds>0);
 const now=Math.floor(Date.now()/1000);
 const history=await request(`/nodes/${node}/metrics?from=${now-3600}&to=${now}&step=60`);
 assert.equal(history.status,200);
 assert.ok(Array.isArray(history.data.points));
 assert.ok(['raw','m5','h1'].includes(history.data.tier));
 // 区间非法或点数过多必须被拒绝，而不是返回空数组假装成功
 assert.equal((await request(`/nodes/${node}/metrics?from=${now}&to=${now-1}&step=60`)).status,400);
 assert.equal((await request(`/nodes/${node}/metrics?from=${now-86400*400}&to=${now}&step=15`)).status,400);
 const tiers=await request('/metrics/status');
 assert.equal(tiers.status,200);
 assert.equal(tiers.data.tiers.length,3);
 // 保留策略必须逐层放宽
 const retention=tiers.data.tiers.map(t=>t.retention_seconds);
 assert.ok(retention[0]<retention[1]&&retention[1]<retention[2]);
 // 会话 Cookie 的 Path 必须限定在安全入口之下，浏览器才不会把它发给分享面。
 assert.ok(
  login.cookie.includes(`Path=/${entrance}/`),
  `会话 Cookie 应限定在入口路径，实际：${login.cookie}`,
 );

 // ---- 独立只读输出 API ----
 assert.equal((await ungated('/share/retired/data/nodes')).status,undefined,'旧分享路径退出门禁');
 assert.equal((await ungated('/api/v1/public/nodes')).status,401);
 const keyCreated=await request('/export/keys','POST',{label:'面板抓取',nodes:[node]});
 assert.equal(keyCreated.status,200);
 const apiKey=keyCreated.data.key;
 const withKey={headers:{Authorization:`Bearer ${apiKey}`}};
 const listed=await request('/export/keys');
 assert.ok(!JSON.stringify(listed.data).includes(apiKey),'密钥列表不返回原文');
 const nodesPublic=await ungated('/api/v1/public/nodes','GET',undefined,withKey);
 assertSchema(nodesPublic.data,responseSchema('/api/v1/public/nodes','get',200));
 assert.equal(nodesPublic.data.nodes.length,1);
 const publicText=JSON.stringify(nodesPublic.data);
 for(const secret of ['127.0.0.1','ssh_port','inventory','public_addresses','overlay_address','connections','plugin_versions'])assert.ok(!publicText.includes(secret),`公开响应不应包含 ${secret}`);
 const emptyKey=await request('/export/keys','POST',{label:'空范围',nodes:[]});
 assert.equal((await ungated('/api/v1/public/nodes','GET',undefined,{headers:{Authorization:`Bearer ${emptyKey.data.key}`}})).data.nodes.length,0);
 const metrics=await ungated('/api/v1/public/metrics','GET',undefined,withKey);
 assert.equal(metrics.status,200);assert.ok(String(metrics.data).includes('# TYPE opsd_node_online gauge'));
 assert.ok(String(metrics.data).includes('opsd_node_online{'));
 assert.equal((await request(`/export/keys/${keyCreated.data.id}`,'DELETE')).status,200);
 assert.equal((await ungated('/api/v1/public/nodes','GET',undefined,withKey)).status,401);
 assert.equal((await request(`/nodes/${node}/actions`,'POST',{idempotency_key:'old-business',action:{type:'docker',container:'existing',operation:'restart'}})).status,400,'旧执行入口不再写入');

 // ---- 真实插件进程、ET 接口适配与双连接（ET 上游使用隔离夹具） ----
 const machineId='11111111-1111-4111-8111-111111111111',instanceId='22222222-2222-4222-8222-222222222222';
 const etPassword=randomBytes(18).toString('hex');
 let etConfig={instance_id:instanceId,virtual_ipv4:'127.0.0.1',network_length:8,dev_name:'lo',hostname:'原名称',mtu:1400,network_secret:'upstream-private-network'};
 let running=true,slowWrite=false,writeStarted=false,releaseWrite;
 const writeReleased=new Promise(resolve=>{releaseWrite=resolve});
 etFixture=http.createServer(async(req,res)=>{
  let body='';for await(const chunk of req)body+=chunk;const input=body?JSON.parse(body):{};
  res.setHeader('Content-Type','application/json');
  if(req.url==='/api/v1/auth/login'){
   if(input.username!=='fixture'||input.password!==createHash('md5').update(etPassword).digest('hex')){res.writeHead(401);res.end('{}');return;}
   res.setHeader('Set-Cookie','et_fixture=active; Path=/');res.end('{}');return;
  }
  if(!req.headers.cookie?.includes('et_fixture=active')){res.writeHead(401);res.end('{}');return;}
  const prefix=`/api/v1/machines/${machineId}`;
  if(req.url===prefix+'/validate-config'){res.end(JSON.stringify({toml_config:'fixture'}));return;}
  if(req.url===prefix+'/networks/config/'+instanceId){
   if(req.method==='PUT'){etConfig=input.config;writeStarted=true;if(slowWrite)await writeReleased;res.end('{}');return;}
   res.end(JSON.stringify(etConfig));return;
  }
  if(req.url===prefix+'/networks/info/'+instanceId){res.end(JSON.stringify({info:{map:{[instanceId]:{running,dev_name:'lo',my_node_info:{virtual_ipv4:{address:{addr:2130706433},network_length:8},stun_info:{public_ip:[],last_update_time:0}}}}}}));return;}
  if(req.url===prefix+'/networks/'+instanceId){running=!input.disabled;res.end('{}');return;}
  if(req.url===prefix+'/networks'){res.end(JSON.stringify({running_inst_ids:running?[instanceId]:[],disabled_inst_ids:running?[]:[instanceId]}));return;}
  if(req.url==='/api/v1/machines'){res.end(JSON.stringify({machines:[{info:{machine_id:machineId,user_token:etPassword,hostname:'隔离 ET 机器',easytier_version:'2.6.4'}}]}));return;}
  res.writeHead(404);res.end('{}');
 });
 await new Promise(resolve=>etFixture.listen(0,'127.0.0.1',resolve));
 const etUrl=`http://127.0.0.1:${etFixture.address().port}`;
 async function pluginArchive(version){
  const manifest=JSON.parse(await readFile(path.join(root,'plugins/et/plugin.json'),'utf8'));manifest.version=version;
  return makeZip({'plugin.json':JSON.stringify(manifest),'api.json':await readFile(path.join(root,'plugins/et/api.json')),['linux-'+(process.arch==='arm64'?'arm64':'amd64')+'/plugin']:await readFile(exe('opsd-plugin-et')),'web/index.html':'<!doctype html><html lang="zh-CN"><body>插件夹具页面</body></html>'});
 }
 const pluginZip=await pluginArchive('0.1.0');
 const installedPlugin=await upload(`/${entrance}/api/v1/plugins/package`,pluginZip);
 assertHttpResponse(installedPlugin,{status:200,route:'/api/v1/plugins/package',method:'post',kind:'json',schema:responseSchema('/api/v1/plugins/package','post',200),contentType:'application/json'});
 assert.equal((await upload(`/${entrance}/api/v1/plugins/package`,pluginZip)).status,400,'不得覆盖同一版本');
 assert.equal((await request('/plugins/et/selection','PUT',{version:'0.1.0',enabled:true})).status,200);
 assert.equal((await request('/plugins/et/configuration','PUT',{config:{url:etUrl,username:'fixture',password:etPassword}})).status,400,'普通配置不得存秘密');
 assert.equal((await request('/plugins/et/configuration','PUT',{config:{url:etUrl,username:'fixture'},secrets:{password:etPassword}})).status,200);
 const configuredPlugin=await request('/plugins/et/configuration');
 assert.deepEqual(configuredPlugin.data.secret_names,['password']);assert.ok(!JSON.stringify(configuredPlugin.data).includes(etPassword));
 const machineList=await request('/plugins/et/api/machines');assert.equal(machineList.status,200);assert.ok(!JSON.stringify(machineList.data).includes(etPassword));
 const binding=await request('/plugins/et/api/bind','POST',{machine_id:machineId,instance_id:instanceId,node_id:node,idempotency_key:'bind-fixture'});assert.equal(binding.status,202);
 await wait(async()=> (await request('/tasks/'+binding.data.task_id)).data.status==='succeeded','ET 实例经实际接口与节点网卡核对后关联');
 await wait(async()=> (await request('/network')).data.nodes[0].connections.paths.every(path=>path.connected),'同一节点的公网与 ET 控制连接并存');
 assert.equal((await request('/network')).data.directory.overlays[0].interface,'lo');
 slowWrite=true;
 const update=await request('/plugins/et/api/config/update','POST',{machine_id:machineId,instance_id:instanceId,patch:{hostname:'新名称'},idempotency_key:'update-fixture'});assert.equal(update.status,202);
 await wait(()=>writeStarted,'旧版本插件开始执行上游写入');
 assert.equal((await upload(`/${entrance}/api/v1/plugins/package`,await pluginArchive('0.1.1'))).status,200);
 assert.equal((await request('/plugins/et/selection','PUT',{version:'0.1.1',enabled:true})).status,200);
 assert.equal((await request('/plugins/et/0.1.0','DELETE')).status,400,'活动任务依赖的版本不能卸载');
 releaseWrite();
 await wait(async()=> (await request('/tasks/'+update.data.task_id)).data.status==='succeeded','旧版本任务在升级后完成读回核对');
 const fixedVersion=(await request('/tasks/'+update.data.task_id)).data;
 assert.equal(fixedVersion.action.version,'0.1.0');assert.equal(fixedVersion.plugin_versions.et,'0.1.0');
 assert.equal(etConfig.hostname,'新名称');assert.equal(etConfig.mtu,1400);assert.equal(etConfig.network_secret,'upstream-private-network');
 assert.equal((await request('/plugins/et/0.1.0','DELETE')).status,200);
 const knownDirectory=(await request('/network')).data.directory;
 if(process.env.OPSD_LAB==='1'){
  // CI 自有 Docker Engine 上运行真实 Portainer；现有业务容器不被接管或重建。
  const existingName='opsd-integration-existing';
  await command('docker',['run','-d','--name',existingName,'--label','io.opsd.integration=true','alpine:3.22','sh','-c','echo opsd-existing; sleep 1200']);fixtureOwned=true;
  const before=JSON.parse(await command('docker',['inspect',existingName]))[0];
  assert.equal((await upload('/'+entrance+'/api/v1/plugins/package',await readFile(path.join(root,'.release/plugins/docker-0.1.0.zip')))).status,200);
  assert.equal((await request('/plugins/docker/selection','PUT',{version:'0.1.0',enabled:true})).status,200);
  assert.equal((await request('/plugins/docker/configuration','PUT',{config:{},secrets:{admin_password:randomBytes(18).toString('hex'),agent_secret:randomBytes(18).toString('hex')}})).status,200);
  assert.equal((await request('/network','PUT',{node_id:node})).status,200);
  async function dockerJob(path,input){
   const accepted=await request('/plugins/docker/api'+path,'POST',{...input,idempotency_key:randomBytes(16).toString('hex')});assert.equal(accepted.status,202,JSON.stringify(accepted.data));
   for(let attempt=0;attempt<600;attempt++){
    const task=(await request('/tasks/'+accepted.data.task_id)).data;
    if(task.status==='succeeded')return task.result;
    if(['failed','uncertain','blocked','cancelled'].includes(task.status))throw new Error('真实 Portainer 任务未成功：'+task.error);
    await new Promise(resolve=>setTimeout(resolve,1000));
   }
   throw new Error('真实 Portainer 任务超时');
  }
  portainerOwned=true;
  await dockerJob('/install',{nodes:[node],allow_engine_install:false});
  const actual=JSON.parse(await command('docker',['inspect','opsd-portainer']))[0];
  assert.deepEqual(actual.HostConfig.PortBindings,{'9443/tcp':[{HostIp:'127.0.0.1',HostPort:'9443'}]});
  const inventory=await request('/plugins/docker/api/inventory','POST',{node_id:node});assert.equal(inventory.status,200);assert.ok(inventory.data.containers.some(container=>container.Id===before.Id));
  const logs=await request('/plugins/docker/api/logs','POST',{node_id:node,container:before.Id});assert.equal(logs.status,200);assert.ok(logs.data.text.includes('opsd-existing'));
  await dockerJob('/mutate',{node_id:node,resource:'container',operation:'stop',id:before.Id});
  await dockerJob('/mutate',{node_id:node,resource:'container',operation:'start',id:before.Id});
  const after=JSON.parse(await command('docker',['inspect',existingName]))[0];assert.equal(after.Id,before.Id);assert.deepEqual(after.Mounts,before.Mounts);assert.equal(after.State.Running,true);
  // 重复安装只复用自有服务；普通既有容器的身份和挂载仍然不变。
  await dockerJob('/install',{nodes:[node],allow_engine_install:false});
  const reused=JSON.parse(await command('docker',['inspect','opsd-portainer']))[0];assert.equal(reused.Id,actual.Id);
  console.log('通过：真实 Portainer Server 安装、ET 地址绑定路径、原生 API 盘点和日志、原有容器启停与身份保留、重复安装复用；地址使用隔离回环夹具，不代表现网 ET 或多主机 Agent 验收。');
 }

 await new Promise(resolve=>etFixture.close(resolve));etFixture=undefined;
 // 使用尚未缓存的接口验证上游不可用；已知地址不得随之清空。
 assert.equal((await request('/plugins/et/api/instances','POST',{machine_id:'33333333-3333-4333-8333-333333333333'})).status,400);
 assert.deepEqual((await request('/network')).data.directory,knownDirectory);
 assert.equal((await ownStatus()).data.connected,true);

 // ---- 主题 ----
 // 控制台主题：令牌级，提交 JSON 清单即可
 const badShort=await request('/themes/console','POST',{short:'default',name:'占用内置标识',surfaces:['console'],tokens:{light:{'--accent':'#123456'}}});
 assert.equal(badShort.status,400,'不应允许主题占用 default 标识');
 const rawConfig=await request('/themes/console','POST',{short:'rawbad',name:'原始面板',surfaces:['console'],tokens:{light:{'--accent':'#123456'}},configuration:{type:'raw',data:[]}});
 assert.equal(rawConfig.status,400,'首版不应接受 raw 配置');
 const injection=await request('/themes/console','POST',{short:'inject',name:'注入',surfaces:['console'],tokens:{light:{'--accent':'#fff; --x: red'}}});
 assert.equal(injection.status,400,'应拒绝会注入额外 CSS 声明的令牌值');
 const remoteUrl=await request('/themes/console','POST',{short:'remote',name:'外链',surfaces:['console'],tokens:{light:{'--accent':'url(https://evil.example/x)'}}});
 assert.equal(remoteUrl.status,400,'应拒绝可引发外部请求的令牌值');
 const consoleTheme=await request('/themes/console','POST',{
  short:'indigo-soft',name:{'zh-CN':'柔和靛蓝',en:'Soft Indigo'},version:'1.0.0',
  surfaces:['console'],
  tokens:{light:{'--accent':'#0F2540','--accent-bg':'#EEF0F2'},dark:{'--accent':'#9BB5D6'}},
  configuration:{type:'managed',data:[{key:'dense',name:{'zh-CN':'更紧凑'},type:'switch',default:true}]},
 });
 assert.equal(consoleTheme.status,200,'控制台主题应安装成功');
 assert.equal((await request('/themes/active','PUT',{surface:'console',short:'indigo-soft',settings:{}})).status,200);
 const active=await request('/themes/active');
 assert.equal(active.data.console.short,'indigo-soft');
 assert.equal(active.data.console.tokens.light['--accent'],'#0F2540','应返回令牌供界面应用');
 assert.equal(active.data.console.settings.dense,true,'未保存的配置项应补齐默认值');
 // 未声明控制台的主题不能被选为控制台主题
 const shareOnly=await request('/themes/console','POST',{short:'shareonly',name:'仅分享',surfaces:['share'],tokens:null});
 assert.equal(shareOnly.status,400,'仅声明分享页的主题不应能安装为控制台主题');

 // 完整控制台前端：安装、启用与样式覆盖分别处理，页面在登录前可用。
 const frontendManifest={short:'test-console',name:'测试控制台',version:'1.0.0',surfaces:['console'],console_frontend:{api_version:2}};
 const frontendFiles={
  'theme.json':JSON.stringify(frontendManifest),
  'index.html':'<!doctype html><html><head><title>独立控制台</title></head><body><div id="external-console"></div><script src="./assets/app.js"></script></body></html>',
  'assets/app.js':'document.title="独立控制台";',
 };
 const frontendZip=makeZip(frontendFiles);
 const frontendInstall=await upload(`/${entrance}/api/v1/themes/console/package`,frontendZip);
 assertHttpResponse(frontendInstall,{status:200,route:'/api/v1/themes/console/package',method:'post',kind:'json',schema:responseSchema('/api/v1/themes/console/package','post',200),contentType:'application/json'});
 assert.equal((await request('/themes/active')).data.console_frontend.short,'default','安装不应自动启用');
 assert.equal((await upload(`/${entrance}/api/v1/themes/console/package`,frontendZip)).status,409,'重复标识不得覆盖');
 assert.equal((await request('/themes/active','PUT',{surface:'console_frontend',short:'indigo-soft'})).status,400,'样式覆盖不得作为完整前端');
 assert.equal((await request('/themes/active','PUT',{surface:'console',short:'test-console'})).status,400,'完整前端不得作为样式覆盖');
 assert.equal((await request('/themes/console','POST',frontendManifest)).status,400,'完整前端必须通过包安装');
 assert.equal((await request('/themes/console','POST',{short:'test-console',name:'覆盖尝试',surfaces:['console'],tokens:{light:{'--accent':'#123456'}}})).status,400,'旧 JSON 安装不能覆盖完整前端');
 assert.equal((await request('/themes/active','PUT',{surface:'console_frontend',short:'test-console',settings:{unexpected:true}})).status,400,'完整前端不能接收托管配置');
 assert.equal((await request('/themes/active','PUT',{surface:'console_frontend',short:'test-console'})).status,200);
 const frontendActive=await request('/themes/active');
 assertHttpResponse(frontendActive,{status:200,route:'/api/v1/themes/active',method:'get',kind:'json',schema:responseSchema('/api/v1/themes/active','get',200),contentType:'application/json'});
 assert.equal(frontendActive.data.console.short,'indigo-soft','完整前端切换不能重置样式覆盖');
 const frontends=await request('/themes');
 assertHttpResponse(frontends,{status:200,route:'/api/v1/themes',method:'get',kind:'json',schema:responseSchema('/api/v1/themes','get',200),contentType:'application/json'});
 assert.equal(frontends.data.themes.find(t=>t.short==='test-console').console_mode,'frontend');
 const guestPage=await ungated(`/${entrance}/`);
 assert.equal(guestPage.status,200);
 assert.ok(guestPage.data.includes('id="external-console"'),'登录前页面应使用全局选择');
 assert.ok(guestPage.data.includes(`<base href="/${entrance}/frontend/test-console/">`),'相对资源应指向固定主题目录');
 assert.equal(guestPage.headers['cache-control'],'no-store');
 assert.equal((await ungated(`/${entrance}/api/v1/themes/active`)).status,401,'公开页面不应放行管理 API');
 const recoveryPage=await ungated(`/${entrance}/frontend/default/`);
 assert.equal(recoveryPage.status,200);
 assert.ok(recoveryPage.data.includes('id="app"'),'固定内置入口应可用于恢复管理');
 assert.equal((await request('/themes/active')).data.console_frontend.short,'test-console','恢复入口不应修改全局选择');
 assert.equal((await ungated(`/${entrance}/frontend/test-console/assets/app.js`)).status,200);
 assert.equal((await request('/themes/active','PUT',{surface:'console_frontend',short:'default'})).status,200);
 assert.equal((await ungated(`/${entrance}/frontend/test-console/assets/app.js`)).status,200,'切换后旧页面的资源仍应可读');
 assert.equal((await ungated(`/${entrance}/frontend/test-console/%2e%2e/%2e%2e/pki/ca-key.pem`)).status,404,'资源不得越出主题目录');
 assert.equal((await request('/themes/active','PUT',{surface:'console_frontend',short:'test-console'})).status,200);
 await stop(hub);
 hub=launch('opsd-hub',serveArgs(hubDir));
 await wait(async()=> (await health()).status===200,'主题选择持久化后的主控启动');
 assert.equal((await request('/themes/active')).data.console_frontend.short,'test-console','正常重启应保留主题选择和资源');
 await wait(async()=> (await request('/nodes')).data?.[0]?.connected,'主题重启场景后 Agent 重连');
 await unlink(path.join(hubDir,'themes','test-console','index.html'));
 assert.equal((await request('/themes/active')).data.console_frontend.short,'default','入口丢失时应明确回到内置前端');
 assert.ok((await ungated(`/${entrance}/`)).data.includes('id="app"'));
 assert.equal((await request('/themes/test-console','DELETE')).status,200);
 assert.equal((await request('/themes/active')).data.console_frontend.short,'default','卸载应清理完整前端选择');
 for(const [short,files] of [
  ['missing-index',{'theme.json':JSON.stringify({...frontendManifest,short:'missing-index'})}],
  ['unknown-api', {...frontendFiles,'theme.json':JSON.stringify({...frontendManifest,short:'unknown-api',console_frontend:{api_version:99}})}],
  ['escaping-frontend', {...frontendFiles,'../escape.txt':'x','theme.json':JSON.stringify({...frontendManifest,short:'escaping-frontend'})}],
 ]){
  assert.equal((await upload(`/${entrance}/api/v1/themes/console/package`,makeZip(files))).status,400,`${short} 应拒绝安装`);
 }
 // 仓库接口的输入拒绝在访问 GitHub 前完成，测试不依赖公网。
 for(const route of ['/themes/console/repository/resolve','/themes/console/repository/install']){
  const input=route.endsWith('/install')?{url:'https://localhost/private',tag:'v1',asset_id:1}:{url:'https://localhost/private'};
  const rejected=await request(route,'POST',input);
  assertHttpResponse(rejected,{status:400,route:'/api/v1'+route,method:'post',kind:'json',schema:responseSchema('/api/v1'+route,'post',400),contentType:'application/json'});
  assert.equal((await request(route,'POST',{...input,nope:true})).status,422,'未知字段不得进入仓库下载流程');
 }

 // 分享主题写入口已移除，不能回退到旧业务分支。
 assert.notEqual((await upload(`/${entrance}/api/v1/themes/share`,Buffer.from('retired'))).status,200);

 // ---- 堡垒机：文件变更走任务机制，并且必须留痕 ----
 // 未枚举的动作类型由 deny_unknown_fields 在反序列化阶段直接拒绝（422），
 // 根本不会进入执行路径——这比在执行前做字符串黑名单可靠得多。
 assert.equal(
  (await request(`/nodes/${node}/files`,'POST',{idempotency_key:'file-unknown',action:{type:'rm_rf',path:'/'}})).status,
  422,'未枚举的文件动作必须在反序列化阶段被拒绝',
 );
 // 缺少必需字段同样拒绝
 assert.equal(
  (await request(`/nodes/${node}/files`,'POST',{idempotency_key:'file-missing',action:{type:'chmod',path:'/etc/hosts'}})).status,
  422,'缺少 mode 字段应被拒绝',
 );
 // 合法的结构化动作会被受理成任务
 const fileTask=await request(`/nodes/${node}/files`,'POST',{
  idempotency_key:'file-mkdir-1',
  action:{type:'mkdir',path:'/tmp/opsd-integration'},
 });
 assert.equal(fileTask.status,200,'文件动作应受理为任务');
 assert.ok(fileTask.data.task_id);
 // 同一幂等键重复提交返回同一个任务，不会执行两次
 const replay=await request(`/nodes/${node}/files`,'POST',{
  idempotency_key:'file-mkdir-1',
  action:{type:'mkdir',path:'/tmp/opsd-integration'},
 });
 assert.equal(replay.data.task_id,fileTask.data.task_id,'幂等键应复用同一任务');
 // 同一幂等键换参数必须拒绝
 assert.equal(
  (await request(`/nodes/${node}/files`,'POST',{
   idempotency_key:'file-mkdir-1',
   action:{type:'mkdir',path:'/etc'},
  })).status,
  400,'幂等键不得用于不同参数',
 );
 // 审计里应当出现这次文件操作，且只记录对象与结果
 // 中文查询参数必须按 URL 编码，否则请求行里的原始字节无法按预期解析
 const FILE_CATEGORY=encodeURIComponent('文件');
 await wait(async()=>{
  const audit=await request(`/audit?category=${FILE_CATEGORY}`);
  return audit.status===200&&audit.data.records.length>0;
 },'文件操作写入审计');
 const audit=await request(`/audit?category=${FILE_CATEGORY}`);
 const entry=audit.data.records[0];
 // AuditRecord 用 serde(flatten)，因此字段是平铺的而不是嵌在 entry 下
 assert.equal(entry.category,'文件');
 assert.ok(entry.target.includes('/tmp/opsd-integration'),'审计应记录被操作的对象');
 assert.ok(entry.node_id,'审计应记录节点');
 assert.ok(entry.source,'审计应记录来源');
 assert.ok(entry.at>0&&entry.id,'审计应带时间与编号');
 // 审计绝不记录内容或秘密
 const auditText=JSON.stringify(audit.data);
 for(const secret of ['password','csrf','X-CSRF','BEGIN PRIVATE KEY']){
  assert.ok(!auditText.includes(secret),`审计不应包含 ${secret}`);
 }
 assert.ok(audit.data.retention_seconds>0,'应说明保留期');
 // 审计筛选逐项生效
 assert.ok(
  (await request(`/audit?result=${encodeURIComponent('失败')}`)).data.records.every(r=>r.result==='失败'),
 );
 assert.ok((await request(`/audit?node=${node}`)).data.records.every(r=>r.node_id===node));
 assert.equal((await request('/audit?from=1&to=2')).data.records.length,0,'超出范围的查询应为空');
 // 条数上限有硬顶
 assert.ok((await request('/audit?limit=1')).data.records.length<=1);

 // ---- 存储预检 ----
 // 尚未盘点时必须给出"待采集"，而不是判为不适合
 const readiness=await request('/storage/readiness');
 assert.equal(readiness.status,200);
 assert.equal(readiness.data.nodes.length,1,'应覆盖全部未撤销节点');
 const verdict=readiness.data.nodes[0];
 assert.equal(verdict.suitability,'pending','未盘点的节点应判为待采集');
 assert.ok(verdict.reasons.some(r=>r.includes('尚未采集')||r.includes('离线')),'待采集必须给出原因');
 assert.equal(readiness.data.ready,false);
 assert.equal(readiness.data.topology,null,'结论不完整时不应给出拓扑');
 // 每个节点都必须有原因，不能只有结论
 assert.ok(readiness.data.nodes.every(n=>n.reasons.length>0),'每个结论都要有原因');
 // 一致性检查逐条给出说明
 assert.ok(readiness.data.checks.length>=5);
 assert.ok(readiness.data.checks.every(c=>c.name&&c.detail!==undefined),'检查项要有名称与说明');
 const nodeCheck=readiness.data.checks.find(c=>c.name.includes('候选节点'));
 assert.equal(nodeCheck.passed,false);
 assert.ok(nodeCheck.detail.includes('尚未采集'),'未采集应说明结论不完整，而不是简单判不合格');
 // 可以触发盘点任务
 const inspectTask=await request(`/nodes/${node}/host-inspect`,'POST',{idempotency_key:'host-1'});
 assert.equal(inspectTask.status,200);
 assert.ok(inspectTask.data.task_id);
 // 同一幂等键不会重复下发
 const inspectAgain=await request(`/nodes/${node}/host-inspect`,'POST',{idempotency_key:'host-1'});
 assert.equal(inspectAgain.data.task_id,inspectTask.data.task_id);
  // 旧存储写入口退出，宿主盘点仍可独立使用。
  assert.notEqual((await request('/storage/clusters','PUT',{name:'retired'})).status,200);
  // ---- 数据库只读巡检 ----
  // 尚未巡检时必须是"未知"，而不是一份全 0 或全绿的报告
  const dbOverview=await request('/database/overview');
  assert.equal(dbOverview.status,200);
  assert.equal(dbOverview.data.nodes.length,1,'应覆盖全部未撤销节点');
  const dbNode=dbOverview.data.nodes[0];
  assert.equal(dbNode.collected_at,null,'从未巡检不得有采集时间');
  assert.equal(dbNode.tone,'ok','未知不参与严重程度排序');
  assert.ok(dbNode.unknown.some(u=>u.includes('尚未巡检')),'必须说明还没巡检过');
  assert.equal(dbNode.report,null);
  assert.ok(dbOverview.data.summary.includes('还没有任何节点'),'结论要如实说明"还没看"');
  // 界面据此显示"未知"，因此结论里必须带上未知项计数
  assert.ok(dbOverview.data.unknown_count>=1);
  // 巡检是只读操作，但仍要走任务机制并留痕
  const inspectDb=await request(`/nodes/${node}/db-inspect`,'POST',{idempotency_key:'db-1'});
  assert.equal(inspectDb.status,200);
  assert.ok(inspectDb.data.task_id);
  assert.equal(
   (await request(`/nodes/${node}/db-inspect`,'POST',{idempotency_key:'db-1'})).data.task_id,
   inspectDb.data.task_id,'同一幂等键不得重复下发',
  );
  await wait(async()=>{
   const audit=await request(`/audit?category=${encodeURIComponent('数据库')}`);
   return audit.status===200&&audit.data.records.length>0;
  },'只读巡检写入审计');
  const dbAudit=(await request(`/audit?category=${encodeURIComponent('数据库')}`)).data.records[0];
  assert.equal(dbAudit.category,'数据库');
  assert.equal(dbAudit.target,'只读巡检');
  assert.ok(dbAudit.node_id===node&&dbAudit.source,'审计要记录对象与来源');
  // 审计与接口响应都不得带出凭据或 SQL 正文
  const dbText=JSON.stringify(dbOverview.data)+JSON.stringify(dbAudit);
  for(const secret of ['MYSQL_PWD','password_file','password_key','EXPORTER_PASSWORD','SELECT ']){
   assert.ok(!dbText.includes(secret),`数据库巡检不得带出 ${secret}`);
  }
  if(process.platform==='win32'){assert.equal(result.result.firewall.state,'error');}
 await stop(hub);hub=undefined;
 await command('opsd-hub',['--data-dir',hubDir,'backup','--destination',path.join(run,'backup')]);
 const restored=path.join(run,'restored');await command('opsd-hub',['--data-dir',restored,'restore','--source',path.join(run,'backup')]);
 hub=launch('opsd-hub',serveArgs(restored));
 cookie='';csrf='';await wait(async()=> (await health()).status===200,'恢复后主控启动');
 // 备份恢复后安全入口保持不变，旧地址仍然有效。
 assert.equal((await request('/auth/me')).status,401);
 const again=await request('/auth/login','POST',{password});assert.equal(again.status,200);cookie=again.cookie.split(';')[0];csrf=again.data.csrf;
 await wait(async()=> (await request('/nodes')).data?.[0]?.connected,'恢复后原 Agent 重连');
 assert.equal((await request(`/tasks/${first.data.task_id}`)).data.status,'succeeded');
 assert.equal((await request('/plugins/et/api/console')).data.url,etUrl);
 assert.deepEqual((await request('/plugins/et/configuration')).data.secret_names,['password']);
 assert.equal((await request(`/nodes/${node}`,'DELETE')).status,200);
 await wait(async()=>!(await request('/nodes')).data?.[0]?.connected,'节点撤销');
 assert.equal((await ownStatus()).status,400,'已撤销节点不能继续读取自身状态');
 const nodesList=await request('/nodes');
 assertHttpResponse(nodesList,{status:200,route:'/api/v1/nodes',method:'get',kind:'json',schema:responseSchema('/api/v1/nodes','get',200),contentType:'application/json'});
 const tasksList=await request('/tasks');
 assertHttpResponse(tasksList,{status:200,route:'/api/v1/tasks',method:'get',kind:'json',schema:responseSchema('/api/v1/tasks','get',200),contentType:'application/json'});
 const exportKeys=await request('/export/keys');
 assertHttpResponse(exportKeys,{status:200,route:'/api/v1/export/keys',method:'get',kind:'json',schema:responseSchema('/api/v1/export/keys','get',200),contentType:'application/json'});
 const metricsOverview=await request('/metrics/overview');
 assertHttpResponse(metricsOverview,{status:200,route:'/api/v1/metrics/overview',method:'get',kind:'json',schema:responseSchema('/api/v1/metrics/overview','get',200),contentType:'application/json'});
 const dbOverviewSchema=await request('/database/overview');
 assertHttpResponse(dbOverviewSchema,{status:200,route:'/api/v1/database/overview',method:'get',kind:'json',schema:responseSchema('/api/v1/database/overview','get',200),contentType:'application/json'});
 const themeInstall=await request('/themes/console','POST',{short:'gate-check',name:'N',surfaces:['console'],tokens:{light:{'--bg':'#fff'},dark:{'--bg':'#000'}}});
 assertHttpResponse(themeInstall,{status:200,route:'/api/v1/themes/console',method:'post',kind:'json',schema:responseSchema('/api/v1/themes/console','post',200),contentType:'application/json'});
 // 真实响应副本注入额外字段，校验器必须拒绝（不声称服务器返回过该字段）
 const poisoned=JSON.parse(JSON.stringify(exportKeys.data));
 poisoned.__injected__=true;
 let rejected=false;
 try{assertSchema(poisoned,responseSchema('/api/v1/export/keys','get',200),'注入额外字段的输出密钥列表副本');}catch{rejected=true;}
 assert.ok(rejected,'校验器必须拒绝带 schema 外字段的响应副本');
 console.log('通过：核心认证、无 IP 注册、任务幂等、网络目录、公开字段与密钥范围、主题 v2、文件与数据库既有契约、备份恢复及节点撤销。');
}finally{
 if(portainerOwned){await command('docker',['rm','-f','opsd-portainer']).catch(()=>{});await command('docker',['volume','rm','opsd-portainer-data']).catch(()=>{});}
 if(fixtureOwned)await command('docker',['rm','-f','opsd-integration-existing']).catch(()=>{});
 etFixture?.close();await stop(agent);await stop(hub);await writeFile(path.join(run,'test.log'),logs.join(''));}
