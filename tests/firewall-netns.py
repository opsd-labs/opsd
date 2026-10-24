"""隔离检验 Agent 真实白名单收敛；所有后端配置和规则仅存在于临时命名空间。"""
import json
import os
import pathlib
import select
import shutil
import subprocess
import sys
import tempfile
import time
import uuid

if os.environ.get("OPSD_LAB") != "1":
    raise SystemExit("仅允许显式实验室执行")
binary = str(pathlib.Path(sys.argv[1]).resolve())

def run(*args, env=None, timeout=30):
    return subprocess.check_output(args, stderr=subprocess.STDOUT, text=True, timeout=timeout, env=env)

# 挂载命名空间让 UFW 配置目录和默认配置也与主机隔离。
if os.environ.get("OPSD_MOUNT_LAB") != "1":
    env = dict(os.environ, OPSD_MOUNT_LAB="1")
    subprocess.run(["unshare", "--mount", "--propagation", "private", sys.executable, __file__, binary], env=env, check=True)
    raise SystemExit(0)

server = """
import socket,select
sockets=[]
for family,address in [(socket.AF_INET,'0.0.0.0'),(socket.AF_INET6,'::')]:
    sock=socket.socket(family);sock.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)
    if family==socket.AF_INET6:sock.setsockopt(socket.IPPROTO_IPV6,socket.IPV6_V6ONLY,1)
    sock.bind((address,19090));sock.listen();sockets.append(sock)
print('ready',flush=True)
while True:
    for sock in select.select(sockets,[],[])[0]:
        client,_=sock.accept();client.sendall(b'opsd');client.close()
"""
probe = """
import socket,sys
source,destination=sys.argv[1:]
sock=socket.socket(socket.AF_INET6 if ':' in source else socket.AF_INET)
sock.settimeout(.5);sock.bind((source,0))
try:
    sock.connect((destination,19090))
    if sock.recv(4)!=b'opsd':sys.exit(3)
except OSError:sys.exit(2)
"""

with tempfile.TemporaryDirectory(prefix="opsd-network-") as temporary:
    directory=pathlib.Path(temporary)
    ufw=directory/"ufw"
    shutil.copytree("/etc/ufw",ufw)
    run("mount","--bind",str(ufw),"/etc/ufw")
    defaults=directory/"ufw-default"
    shutil.copyfile("/etc/default/ufw",defaults)
    run("mount","--bind",str(defaults),"/etc/default/ufw")
    for backend in ["nftables","iptables","ufw","firewalld"]:
        prefix="opsd-"+uuid.uuid4().hex[:8]
        target,peer,sink=[prefix+s for s in ["t","p","s"]]
        created=[]; processes=[]; env=dict(os.environ,LC_ALL="C",OPSD_AGENT_MODE="host",DBUS_SYSTEM_BUS_ADDRESS="unix:path="+str(directory/"no-host-bus"))
        def inside(namespace,*args,timeout=30):
            return run("ip","netns","exec",namespace,*args,env=env,timeout=timeout)
        def verify(namespace,source,destination,expected):
            result=subprocess.run(["ip","netns","exec",namespace,"python3","-c",probe,source,destination],env=env,timeout=3)
            if result.returncode not in [0,2]:
                raise AssertionError("探测器异常，不得计作策略拒绝")
            assert (result.returncode==0)==expected,(backend,source,destination,expected)
        def start_server(namespace):
            process=subprocess.Popen(["ip","netns","exec",namespace,"python3","-u","-c",server],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)
            processes.append(process)
            assert select.select([process.stdout],[],[],5)[0] and process.stdout.readline().strip()=="ready","实验服务未就绪"
        def apply(revision,addresses):
            state=directory/backend;state.mkdir(exist_ok=True)
            desired={"revision":revision,"allowlist":{"revision":revision,"public_addresses":addresses,"overlay":{"interface":"opsd-et","network":"100.64.0.0/10"}},"hub_overlay_address":None,"probes":{"version":revision,"targets":[]}}
            source=state/"desired.json";source.write_text(json.dumps(desired))
            child_env=dict(env,OPSD_NETWORK_APPLY=str(source),OPSD_NETWORK_STATE=str(state))
            output=run("ip","netns","exec",target,"env",*[k+"="+v for k,v in child_env.items() if k.startswith("OPSD_") or k in ["LC_ALL","DBUS_SYSTEM_BUS_ADDRESS"]],binary,"--ignored","--exact","agent::firewall_backends::isolated_tcp_network_policy","--nocapture",timeout=120)
            actual=json.loads((state/"applied.json").read_text())
            return actual
        try:
            for namespace in [target,peer,sink]:
                run("ip","netns","add",namespace);created.append(namespace)
                run("ip","-n",namespace,"link","set","lo","up")
                inside(namespace,"sysctl","-qw","net.ipv4.ip_forward=1","net.ipv6.conf.all.forwarding=1","net.ipv4.conf.all.rp_filter=0","net.ipv4.conf.default.rp_filter=0")
            if backend=="firewalld":inside(target,"ufw","--force","disable")
            for a,b,namespace in [("opsd0","peer0",peer),("opsd-et","peer-et",peer),("dock0","sink0",sink)]:
                run("ip","-n",target,"link","add",a,"type","veth","peer","name",b,"netns",namespace)
                for n,interface in [(target,a),(namespace,b)]:run("ip","-n",n,"link","set",interface,"up")
            for n,interface,addresses in [(target,"opsd0",["11.0.0.1/24","2001:4860::1/64"]),(peer,"peer0",["11.0.0.2/24","11.0.0.3/24","2001:4860::2/64","2001:4860::3/64","100.100.0.3/32"]),(target,"opsd-et",["100.100.0.1/24"]),(peer,"peer-et",["100.100.0.2/24"]),(target,"dock0",["172.20.0.1/24"]),(sink,"sink0",["172.20.0.2/24"])]:
                for address in addresses:run("ip","-n",n,"address","add",address,"dev",interface,*( ["nodad"] if ":" in address else []))
            run("ip","-n",peer,"route","add","172.20.0.0/24","via","11.0.0.1")
            run("ip","-n",sink,"route","add","default","via","172.20.0.1")
            run("ip","-n",target,"route","add","100.100.0.3/32","dev","opsd0")
            for n in [target,peer,sink]:start_server(n)
            # 拒绝结果只能在同一拓扑原本可达的基础上认定。
            cases=[("11.0.0.2","11.0.0.1"),("11.0.0.3","11.0.0.1"),("2001:4860::2","2001:4860::1"),("2001:4860::3","2001:4860::1"),("100.100.0.2","100.100.0.1"),("100.100.0.3","11.0.0.1")]
            for source,destination in cases:verify(peer,source,destination,True)
            verify(peer,"11.0.0.2","172.20.0.2",True)
            if backend=="ufw":
                inside(target,"ufw","--force","reset")
                inside(target,"ufw","default","deny","incoming")
                inside(target,"ufw","default","deny","outgoing")
                inside(target,"ufw","--force","enable")
            elif backend=="firewalld":
                config=directory/"firewalld";config.mkdir()
                (config/"firewalld.conf").write_text("DefaultZone=drop\nFirewallBackend=nftables\n")
                bus=subprocess.Popen(["dbus-daemon","--session","--nofork","--print-address=1"],stdout=subprocess.PIPE,text=True)
                processes.append(bus)
                assert select.select([bus.stdout],[],[],5)[0],"私有 DBus 未启动"
                env["DBUS_SYSTEM_BUS_ADDRESS"]=bus.stdout.readline().strip()
                log=open(directory/"firewalld.log","w+")
                daemon=subprocess.Popen(["ip","netns","exec",target,"firewalld","--nofork","--nopid","--log-target=console","--system-config",str(config),"--default-config","/usr/lib/firewalld"],env=env,stdout=log,stderr=log)
                processes.append(daemon)
                for attempt in range(50):
                    try:inside(target,"firewall-cmd","--state");break
                    except subprocess.CalledProcessError:
                        if daemon.poll() is not None:log.seek(0);raise AssertionError(log.read())
                        time.sleep(.1)
                else:raise AssertionError("私有 firewalld 未就绪")
            if backend in ["iptables","ufw"]:
                for tool in ["iptables","ip6tables"]:
                    inside(target,tool,"-N","DOCKER-USER")
                    inside(target,tool,"-A","FORWARD","-j","DOCKER-USER")
                    inside(target,tool,"-A","INPUT","-p","tcp","--dport","19999","-m","comment","--comment","external-sentinel","-j","ACCEPT")
                    for chain in ["INPUT","OUTPUT","FORWARD"]:
                        inside(target,tool,"-A",chain,"-m","conntrack","--ctstate","ESTABLISHED,RELATED","-j","ACCEPT")
                        if tool=="ip6tables":inside(target,tool,"-A",chain,"-p","ipv6-icmp","-j","ACCEPT")
                        inside(target,tool,"-P",chain,"DROP")
            # 外部独立表中的 DROP 也不能使白名单失效。
            rules=directory/"external.nft"
            rules.write_text('table inet external {\n'+''.join('chain '+hook+' { type filter hook '+hook+' priority 20; policy drop; ct state established,related accept; meta l4proto ipv6-icmp accept; tcp dport 19999 counter accept comment "external-sentinel"; }\n' for hook in ['input','output','forward'])+'}\n')
            inside(target,"nft","-f",str(rules))
            for source,destination in cases:verify(peer,source,destination,False)
            actual=apply(1,["11.0.0.2","2001:4860::2"])
            if actual["error"]:
                if backend=="firewalld" and "独占规则表" in actual["error"]:
                    assert "opsd:peer" not in inside(target,"nft","list","ruleset")
                    print("限制：独占保护的 firewalld 拒绝接入，未降低保护；不计为后端白名单通过。",flush=True)
                    continue
                raise AssertionError(actual)
            assert actual["applied_revision"]==1 and actual["backend"]==backend,actual
            for source,destination in cases:verify(peer,source,destination,source.endswith(".2") or source.endswith("::2"))
            verify(peer,"11.0.0.2","172.20.0.2",True)
            verify(peer,"11.0.0.3","172.20.0.2",False)
            verify(target,"11.0.0.1","11.0.0.2",True)
            verify(target,"11.0.0.1","11.0.0.3",False)
            assert "external-sentinel" in inside(target,"nft","list","table","inet","external")
            # 新进程使用原数据库，收到旧版本不能退回；外部 reload 后重新收敛。
            assert apply(2,["11.0.0.3","2001:4860::3"])["applied_revision"]==2
            assert apply(1,["11.0.0.2","2001:4860::2"])["applied_revision"]==2
            verify(peer,"11.0.0.3","11.0.0.1",True)
            verify(peer,"11.0.0.2","11.0.0.1",False)
            inside(target,"nft","delete","table","inet","external")
            inside(target,"nft","-f",str(rules))
            assert apply(2,["11.0.0.3","2001:4860::3"])["applied_revision"]==2
            verify(peer,"11.0.0.3","11.0.0.1",True)
            if backend in ["iptables","ufw"]:
                for tool in ["iptables-save","ip6tables-save"]:
                    text=inside(target,tool)
                    assert "external-sentinel" in text and "-A DOCKER-USER -m comment --comment" in text
            print("通过："+backend+"，IPv4/IPv6、ET 入接口限定、双向和容器转发、旧版本拒绝与进程恢复、外部规则保留。",flush=True)
        finally:
            for process in reversed(processes):
                process.terminate()
                try:process.wait(timeout=3)
                except subprocess.TimeoutExpired:process.kill();process.wait()
            for namespace in reversed(created):subprocess.run(["ip","netns","delete",namespace],check=True)
