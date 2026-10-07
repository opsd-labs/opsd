import {spawn} from 'node:child_process';
import {mkdir,writeFile,readFile,unlink} from 'node:fs/promises';
import path from 'node:path';
import https from 'node:https';
import http from 'node:http';
import zlib from 'node:zlib';
import {randomBytes} from 'node:crypto';
import assert from 'node:assert/strict';
import {loadContract} from './lib/http-assert.mjs';
const root=process.cwd(),run=path.join(root,'.data','integration-'+Date.now());await mkdir(run,{recursive:true});
const bin=path.join(root,'target','debug');const exe=name=>path.join(bin,name+(process.platform==='win32'?'.exe':''));
const hubDir=path.join(run,'hub'),agentDir=path.join(run,'agent');const password=randomBytes(24).toString('hex');const passwordFile=path.join(run,'password');await writeFile(passwordFile,password,{mode:0o600});
async function command(name,args){return new Promise((resolve,reject)=>{const child=spawn(exe(name),args,{cwd:root,windowsHide:true});let output='';child.stdout.on('data',b=>output+=b);child.stderr.on('data',b=>output+=b);child.on('error',reject);child.on('exit',code=>code===0?resolve(output):reject(new Error(output)));});}
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
function upload(route,bytes){return new Promise((resolve,reject)=>{const req=https.request({hostname:'localhost',port:base,path:route,method:'POST',ca,headers:{Origin:`https://localhost:${base}`,Cookie:cookie,'X-CSRF-Token':csrf,'Content-Type':'application/zip','Content-Length':bytes.length}},res=>{let text='';res.on('data',b=>text+=b);res.on('end',()=>{let value;try{value=JSON.parse(text)}catch{value=text}resolve({status:res.statusCode,data:value});});});req.on('error',reject);req.setTimeout(10000,()=>req.destroy(new Error('请求超时')));req.write(bytes);req.end();});}

/**
 * 生成一个「仅存储、不压缩」的 zip，用于在测试里构造主题包。
 * 只用到未压缩条目，因此不需要引入压缩库。
 */
function makeZip(files){
 const crc32=zlib.crc32;
 const localParts=[],central=[];
 let offset=0;
 for(const [name,content] of Object.entries(files)){
  const data=Buffer.from(content,'utf8');
  const nameBuf=Buffer.from(name,'utf8');
  const crc=crc32(data)>>>0;
  const local=Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50,0);local.writeUInt16LE(20,4);local.writeUInt16LE(0,6);
  local.writeUInt16LE(0,8);local.writeUInt16LE(0,10);local.writeUInt16LE(0,12);
  local.writeUInt32LE(crc,14);local.writeUInt32LE(data.length,18);local.writeUInt32LE(data.length,22);
  local.writeUInt16LE(nameBuf.length,26);local.writeUInt16LE(0,28);
  localParts.push(local,nameBuf,data);
  const dir=Buffer.alloc(46);
  dir.writeUInt32LE(0x02014b50,0);dir.writeUInt16LE(20,4);dir.writeUInt16LE(20,6);
  dir.writeUInt16LE(0,8);dir.writeUInt16LE(0,10);dir.writeUInt16LE(0,12);dir.writeUInt16LE(0,14);
  dir.writeUInt32LE(crc,16);dir.writeUInt32LE(data.length,20);dir.writeUInt32LE(data.length,24);
  dir.writeUInt16LE(nameBuf.length,28);dir.writeUInt16LE(0,30);dir.writeUInt16LE(0,32);
  dir.writeUInt16LE(0,34);dir.writeUInt16LE(0,36);dir.writeUInt32LE(0,38);dir.writeUInt32LE(offset,42);
  central.push(dir,nameBuf);
  offset+=local.length+nameBuf.length+data.length;
 }
 const centralBuf=Buffer.concat(central);
 const end=Buffer.alloc(22);
 end.writeUInt32LE(0x06054b50,0);end.writeUInt16LE(0,4);end.writeUInt16LE(0,6);
 end.writeUInt16LE(Object.keys(files).length,8);end.writeUInt16LE(Object.keys(files).length,10);
 end.writeUInt32LE(centralBuf.length,12);end.writeUInt32LE(offset,16);end.writeUInt16LE(0,20);
 return Buffer.concat([...localParts,centralBuf,end]);
}

let hub,agent;const logs=[];
function launch(name,args){const c=spawn(exe(name),args,{cwd:root,windowsHide:true});c.stdout.on('data',b=>logs.push(b.toString()));c.stderr.on('data',b=>logs.push(b.toString()));return c;}
async function wait(fn,label){for(let i=0;i<80;i++){try{if(await fn())return}catch{}await new Promise(r=>setTimeout(r,200));}throw new Error(`${label} 超时\n${logs.join('').slice(-4000)}`);}
async function stop(c){if(!c||c.exitCode!==null)return;const done=new Promise(r=>c.on('exit',r));c.kill();await done;}
const serveArgs=dir=>['--data-dir',dir,'serve','--listen',`127.0.0.1:${base}`,'--agent-listen','127.0.0.1:19444','--health-listen',`127.0.0.1:${healthPort}`,'--origin',`https://localhost:${base}`];
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
 const registration=await request('/enrollment-tokens','POST',{name:'测试节点',public_addresses:[],overlay_address:'127.0.0.1',ssh_port:22});assert.equal(registration.status,200);
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
 agent=launch('opsd-agent',['--data-dir',agentDir,'run']);
 await wait(async()=> (await request('/nodes')).data?.[0]?.connected,'Agent mTLS 注册连接');
 const node=registration.data.node_id;
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
 const rename=(overlay)=>request(`/nodes/${node}`,'PUT',{name:'修改名称',public_addresses:[],overlay_address:overlay,ssh_port:22});
 assert.equal((await rename('127.0.0.1')).status,200);
 assert.equal((await request('/peer-addresses')).data.version,peerVersion);
 // 探测目标随节点目录下发：目录没变就不重复下发，变了就换版本重发
 const probeSets=(await request('/tasks')).data.filter(t=>t.action?.type==='peer_probe_targets');
 assert.equal(probeSets.length,1,'探测目标应作为任务下发');
 const probeKey=probeSets[0].action.set.version;
 assert.equal(probeSets[0].action.set.targets.length,0,'只有一台节点时没有对端可测');
 // 同样的目录再改一次名字：版本不变，也就不产生新任务
 assert.equal((await rename('127.0.0.1')).status,200);
 const unchanged=(await request('/tasks')).data.filter(t=>t.action?.type==='peer_probe_targets');
 assert.equal(unchanged.length,1,'目录没变时不应重复下发探测目标');
 // 地址变了 → 版本必须换，且新任务要带上新版本号
 assert.equal((await rename('127.0.0.2')).status,200);
 await wait(async()=>{
  const sets=(await request('/tasks')).data.filter(t=>t.action?.type==='peer_probe_targets');
  return sets.some(t=>t.action.set.version!==probeKey);
 },'目录变化后探测目标换版本重发');
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

 // ---- 分享面 ----
 // 未开启时拒绝
 assert.equal((await request('/share/settings')).data.enabled,false);
 const beforeShare=await ungated('/share/anything/data/nodes');
 assert.equal(beforeShare.status,undefined,'关闭分享时分享路径应被丢弃');
 // 开启并创建令牌
 assert.equal((await request('/share/settings','PUT',{enabled:true,site:{name:'演示状态',description:'十二节点',footer:''}})).status,200);
 const created=await request('/share/tokens','POST',{label:'临时演示',expires_hours:1});
 assert.equal(created.status,200);
 const shareToken=created.data.token;
 assert.ok(shareToken&&shareToken.length>=16,'应返回一次性的明文令牌');
 assert.ok(created.data.record.digest&&!created.data.record.digest.includes(shareToken),'控制库只保存摘要');
 // 明文不再可取回
 const listed=await request('/share/tokens');
 assert.ok(listed.data.tokens.every(t=>!JSON.stringify(t).includes(shareToken)),'列表不应包含明文令牌');

 // 有效令牌：页面与数据接口都可用
 const page=await ungated(`/share/${shareToken}/`);
 assert.equal(page.status,200,'分享页应可访问');
 assert.ok(String(page.data).includes('<div id="share">'),'应返回分享页模板');
 const redirect=await ungated(`/share/${shareToken}`);
 assert.equal(redirect.status,308,'缺结尾斜杠时应补上，否则相对资源会丢前缀');
 const nodesPublic=await ungated(`/share/${shareToken}/data/nodes`);
 assert.equal(nodesPublic.status,200);
 assert.equal(nodesPublic.data.nodes.length,1,'只应看到未隐藏的节点');
 const publicNode=nodesPublic.data.nodes[0];
 // 白名单：绝不下发地址、端口与内部细节
 const publicText=JSON.stringify(publicNode);
 for(const secret of ['127.0.0.1','100.100.201','ssh_port','inventory','docker','public_addresses','overlay_address']){
  assert.ok(!publicText.includes(secret),`公开响应不应包含 ${secret}`);
 }
 assert.ok('cpu_usage' in publicNode&&'online' in publicNode);

 // 无效令牌：与错误入口一样被丢弃，不产生任何响应
 for(const bad of ['wrongtoken0000000', shareToken.slice(0,-1), shareToken.toUpperCase()]){
  const dropped=await ungated(`/share/${bad}/data/nodes`);
  assert.equal(dropped.status,undefined,`无效令牌 ${bad} 应被丢弃`);
  assert.ok(dropped.error,`无效令牌 ${bad} 应直接断开连接`);
 }
 // 分享前缀下不存在控制台接口：这里应得到 404，绝不能返回控制台数据
 const crossed=await ungated(`/share/${shareToken}/api/v1/nodes`);
 assert.equal(crossed.status,404,'分享前缀下不应挂载控制台接口');
 assert.ok(!JSON.stringify(crossed.data).includes('inventory'),'不得经由分享前缀泄漏控制台数据');

 // 机器接口需要分享令牌 + API Key
 assert.equal((await ungated(`/share/${shareToken}/api/v1/public/metrics`)).status,401,'缺少 API Key 应被拒绝');
 const keyCreated=await request('/share/keys','POST',{label:'面板抓取'});
 assert.equal(keyCreated.status,200);
 const apiKey=keyCreated.data.key;
 const withKey={headers:{Authorization:`Bearer ${apiKey}`}};
 const metrics=await ungated(`/share/${shareToken}/api/v1/public/metrics`,'GET',undefined,withKey);
 assert.equal(metrics.status,200);
 assert.ok(String(metrics.data).includes('# TYPE opsd_node_online gauge'),'应返回 Prometheus 文本格式');
 assert.ok(String(metrics.data).includes('opsd_node_online{'),'应包含样本行');
 for(const secret of ['127.0.0.1','100.100.201','ssh_port']){
  assert.ok(!String(metrics.data).includes(secret),`Prometheus 输出不应包含 ${secret}`);
 }
 // 撤销 API Key 后立即失效
 assert.equal((await request(`/share/keys/${keyCreated.data.record.id}`,'DELETE')).status,200);
 assert.equal((await ungated(`/share/${shareToken}/api/v1/public/metrics`,'GET',undefined,withKey)).status,401,'撤销后应拒绝');

 // 撤销分享令牌后，分享路径重新变成"被丢弃"
 assert.equal((await request(`/share/tokens/${created.data.record.id}`,'DELETE')).status,200);
 const revoked=await ungated(`/share/${shareToken}/data/nodes`);
 assert.equal(revoked.status,undefined,'撤销后应被丢弃');
 assert.ok(revoked.error);
 // 关闭分享面
 assert.equal((await request('/share/settings','PUT',{enabled:false,site:{name:'演示状态',description:'十二节点',footer:''}})).status,200);

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
 const frontendManifest={short:'test-console',name:'测试控制台',version:'1.0.0',surfaces:['console'],console_frontend:{api_version:1}};
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
 assert.equal(frontendActive.data.share.short,'default','完整前端切换不能影响分享页');
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
  ['unknown-api', {...frontendFiles,'theme.json':JSON.stringify({...frontendManifest,short:'unknown-api',console_frontend:{api_version:2}})}],
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

 // 分享页主题：包级，上传 zip
 const themePackage=makeZip({
  'theme.json':JSON.stringify({short:'paper',name:{'zh-CN':'纸白'},version:'1.0.0',surfaces:['share'],configuration:{type:'managed',data:[{key:'show_tags',name:'显示标签',type:'switch',default:true}]}}),
  'index.html':'<!doctype html><html><body><h1>纸白主题</h1><script src="./app.js"></script></body></html>',
  'app.js':'document.title="纸白";',
 });
 // 主题上传接口挂在安全入口之下，不带入口的请求会被门禁直接丢弃
 const themeUpload=await ungated('/api/v1/themes/share','POST',undefined,{headers:{'Content-Type':'application/zip','X-CSRF-Token':csrf}});
 assert.equal(themeUpload.status,undefined,'主题上传接口不应在入口之外可达');
 assert.ok(themeUpload.error);
 // 带入口时才会进入鉴权与解析：非 zip 的请求体必须被明确拒绝
 const notZip=await upload(`/${entrance}/api/v1/themes/share`,Buffer.from('not a zip'));
 assert.equal(notZip.status,400,'非 zip 的主题包应被拒绝');
 const installed=await upload(`/${entrance}/api/v1/themes/share`,themePackage);
 assert.equal(installed.status,200,`分享页主题应安装成功：${JSON.stringify(installed.data)}`);
 assert.equal(installed.data.short,'paper');
 assert.ok(installed.data.digest&&installed.data.digest.length===64,'应由主控自行计算摘要');
 // 越界路径的包必须被拒绝
 const slip=makeZip({'../escape.txt':'x','theme.json':JSON.stringify({short:'slip',name:'越界',surfaces:['share']}),'index.html':'<html></html>'});
 assert.equal((await upload(`/${entrance}/api/v1/themes/share`,slip)).status,400,'含越界路径的包应被拒绝');
 // 缺少 index.html 的包必须被拒绝
 const noIndex=makeZip({'theme.json':JSON.stringify({short:'noindex',name:'缺页',surfaces:['share']})});
 assert.equal((await upload(`/${entrance}/api/v1/themes/share`,noIndex)).status,400,'缺少 index.html 的包应被拒绝');

 // 启用分享面与分享页主题，验证包级替换与 CSP
 assert.equal((await request('/share/settings','PUT',{enabled:true,site:{name:'演示状态',description:'十二节点',footer:''}})).status,200);
 const themeToken=(await request('/share/tokens','POST',{label:'主题验证',expires_hours:1})).data.token;
 assert.equal((await request('/themes/active','PUT',{surface:'share',short:'paper',settings:{}})).status,200);
 const themed=await ungated(`/share/${themeToken}/`);
 assert.equal(themed.status,200);
 assert.ok(String(themed.data).includes('纸白主题'),'应返回主题包自带的页面');
 assert.ok(String(themed.headers['content-security-policy']||'').includes("default-src 'none'"),'主题页面应带独立 CSP');
 const asset=await ungated(`/share/${themeToken}/theme/app.js`);
 assert.equal(asset.status,200,'主题资源应可访问');
 assert.ok(String(asset.headers['content-security-policy']||'').includes("default-src 'none'"),'主题资源应带 CSP');
 assert.ok(!String(asset.headers['content-security-policy']||'').includes('https:'),'CSP 不应放行任意外部来源');
 // 主题资源不得越出主题目录
 assert.equal((await ungated(`/share/${themeToken}/theme/../share.html`)).status!==200,true,'主题资源不应越界');
 // 公开的主题设置可读，且必须带有提醒用途
 const publicTheme=await ungated(`/share/${themeToken}/data/theme`);
 assert.equal(publicTheme.status,200);
 assert.equal(publicTheme.data.short,'paper');
 assert.equal(publicTheme.data.settings.show_tags,true,'公开设置应补齐默认值');

 // 删除主题后自动回落到内置页面
 assert.equal((await request('/themes/paper','DELETE')).status,200);
 const fallback=await ungated(`/share/${themeToken}/`);
 assert.equal(fallback.status,200);
 assert.ok(String(fallback.data).includes('id="share"'),'删除后应回落到内置分享页');
 assert.equal((await request('/themes/default','DELETE')).status,400,'内置主题不可删除');
 assert.equal((await request('/share/settings','PUT',{enabled:false,site:{name:'演示状态',description:'十二节点',footer:''}})).status,200);

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
  // 未盘点的节点不能被纳入部署计划：宁可拒绝，也不能按猜测去格式化
  const specBody={
   name:'lab',nodes:['a','b','c','d'],image_tag:'RELEASE.2026-09-03T13-18-01Z',
   endpoint:'http://10.0.0.1:9000',access_key:'opsdadmin',secret_key:'integration-secret',
  };
  assert.equal((await request('/storage/readiness')).data.nodes[0].suitability,'pending','盘点结果未回来前仍是待采集');
  // 保存定义本身不碰节点，因此四个编号不同的节点即使还不存在也能先存下来
  const saved=await request('/storage/clusters','PUT',specBody);
  assert.equal(saved.status,200,'定义只做形态校验，不要求节点已存在');
  assert.equal(saved.data.cluster.name,'lab');
  // 形态不对的定义必须当场拒绝：重复节点会让拓扑算错，不足四台无法组成纠删码
  assert.equal(
   (await request('/storage/clusters','PUT',{...specBody,nodes:[node,node,node,node]})).status,
   400,'重复节点不能定义成集群',
  );
  assert.equal(
   (await request('/storage/clusters','PUT',{...specBody,name:'small',nodes:['a','b','c']})).status,
   400,'不足四台不能定义成集群',
  );
  assert.equal(
   (await request('/storage/clusters','PUT',{...specBody,name:'bad',nodes:['a','b','c','d'],image_tag:'latest'})).status,
   400,'滚动标签 latest 必须拒绝',
  );
  // 生成计划时才核对现实：节点不存在／未盘点都必须拒绝，而不是给出一份乐观计划
  const badPlan=await request('/storage/plans','POST',{name:'lab'});
  assert.equal(badPlan.status,400,'不存在的节点不能生成部署计划');
  assert.ok(badPlan.data.error,'拒绝时必须说明原因');
  // 真实存在但尚未盘点的节点同样不能纳入
  const realSpec=await request('/storage/clusters','PUT',{
   ...specBody,name:'lab-real',nodes:[node,'x','y','z'],
  });
  assert.equal(realSpec.status,200);
  assert.equal(
   (await request('/storage/plans','POST',{name:'lab-real'})).status,400,
   '未盘点的节点不能生成部署计划',
  );
  assert.equal(
   (await request('/storage/plans','POST',{name:'not-defined'})).status,400,
   '未定义的集群不能生成计划',
  );
  // 不存在的计划与不存在的集群都只给出"没有"，绝不隐式创建
  assert.equal((await request('/storage/plans/nope')).status,400);
  assert.equal(
   (await request('/storage/plans/apply','POST',{plan_id:'nope',acknowledge_destructive:true})).status,
   400,'执行不存在的计划必须拒绝',
  );
  // 集群列表只返回定义，绝不带出 S3 密钥
  const clusters=await request('/storage/clusters');
  assert.equal(clusters.status,200);
  assert.ok(clusters.data.min_nodes>=4,'界面需要知道最少节点数');
  assert.equal(clusters.data.clusters.length,2);
  assert.ok(
   clusters.data.clusters.every(c=>c.secret_key===undefined),
   '集群列表不得回传密钥',
  );
  assert.ok(
   clusters.data.clusters.every(c=>c.has_secret_key===true),
   '只说明密钥已设置，供界面判断',
  );
  assert.ok(!JSON.stringify(clusters.data).includes('integration-secret'),'密钥不得出现在任何响应里');
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
  if(process.platform==='win32'){assert.equal(result.result.docker.state,'error');assert.equal(result.result.firewall.state,'error');}
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
 assert.equal((await request(`/nodes/${node}`,'DELETE')).status,200);
 await wait(async()=>!(await request('/nodes')).data?.[0]?.connected,'节点撤销');
 const nodesList=await request('/nodes');
 assertHttpResponse(nodesList,{status:200,route:'/api/v1/nodes',method:'get',kind:'json',schema:responseSchema('/api/v1/nodes','get',200),contentType:'application/json'});
 const tasksList=await request('/tasks');
 assertHttpResponse(tasksList,{status:200,route:'/api/v1/tasks',method:'get',kind:'json',schema:responseSchema('/api/v1/tasks','get',200),contentType:'application/json'});
 const shareSettings=await request('/share/settings');
 assertHttpResponse(shareSettings,{status:200,route:'/api/v1/share/settings',method:'get',kind:'json',schema:responseSchema('/api/v1/share/settings','get',200),contentType:'application/json'});
 const metricsOverview=await request('/metrics/overview');
 assertHttpResponse(metricsOverview,{status:200,route:'/api/v1/metrics/overview',method:'get',kind:'json',schema:responseSchema('/api/v1/metrics/overview','get',200),contentType:'application/json'});
 const dbOverviewSchema=await request('/database/overview');
 assertHttpResponse(dbOverviewSchema,{status:200,route:'/api/v1/database/overview',method:'get',kind:'json',schema:responseSchema('/api/v1/database/overview','get',200),contentType:'application/json'});
 const themeInstall=await request('/themes/console','POST',{short:'gate-check',name:'N',surfaces:['console'],tokens:{light:{'--bg':'#fff'},dark:{'--bg':'#000'}}});
 assertHttpResponse(themeInstall,{status:200,route:'/api/v1/themes/console',method:'post',kind:'json',schema:responseSchema('/api/v1/themes/console','post',200),contentType:'application/json'});
 const clusterList=await request('/storage/clusters');
 assertHttpResponse(clusterList,{status:200,route:'/api/v1/storage/clusters',method:'get',kind:'json',schema:responseSchema('/api/v1/storage/clusters','get',200),contentType:'application/json'});
 // 真实响应副本注入额外字段，校验器必须拒绝（不声称服务器返回过该字段）
 const poisoned=JSON.parse(JSON.stringify(shareSettings.data));
 poisoned.__injected__=true;
 let rejected=false;
 try{assertSchema(poisoned,responseSchema('/api/v1/share/settings','get',200),'注入额外字段的分享设置副本');}catch{rejected=true;}
 assert.ok(rejected,'校验器必须拒绝带 schema 外字段的响应副本');
 console.log(`通过：安全入口门禁与丢弃语义、登录、CSRF、一次性注册、证书续期与 mTLS、幂等冲突、任务序号、目录改名不增加地址版本、对端探测目标随目录版本化下发、指标接口区间校验与分层保留、分享面令牌与 API Key 双重校验及隔离、主题清单校验与包级分享页主题替换及 CSP、文件动作反序列化拒绝与幂等及审计留痕、存储预检待采集语义与盘点幂等、存储集群定义校验与计划拒绝路径及密钥不回传、数据库只读巡检的未知语义与审计留痕、错误状态、备份恢复与节点撤销。入口 ${entrance}`);
}finally{await stop(agent);await stop(hub);await writeFile(path.join(run,'test.log'),logs.join(''));}
