"""在两个临时网络命名空间中检验真实 TCP 流量，不修改 WSL 默认防火墙。"""
import os
import pathlib
import subprocess
import sys
import uuid

if os.environ.get("OPSD_LAB") != "1":
    raise SystemExit("仅允许显式实验室执行")
directory = pathlib.Path(sys.argv[1])
prefix = "opsd-test-" + uuid.uuid4().hex[:10]
target, peer = prefix + "-t", prefix + "-p"
created, servers = [], []

def run(*args):
    return subprocess.check_output(args, stderr=subprocess.STDOUT, text=True, timeout=15)

def inside(namespace, *args):
    return run("ip", "netns", "exec", namespace, *args)

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
sock.settimeout(1);sock.bind((source,0))
try:
    sock.connect((destination,19090));assert sock.recv(4)==b'opsd'
except (OSError,AssertionError):sys.exit(2)
"""

def verify(namespace, source, destination, expected):
    result = subprocess.run(["ip", "netns", "exec", namespace, "python3", "-c", probe, source, destination], timeout=5)
    if result.returncode not in (0, 2):
        raise AssertionError("探测器异常，不得当作策略拒绝")
    assert (result.returncode == 0) == expected, (source, destination, expected)

try:
    for namespace in [target, peer]:
        run("ip", "netns", "add", namespace)
        created.append(namespace)
        run("ip", "-n", namespace, "link", "set", "lo", "up")
    run("ip", "-n", target, "link", "add", "opsd0", "type", "veth", "peer", "name", "opsd1", "netns", peer)
    for namespace, interface, addresses in [
        (target, "opsd0", ["11.0.0.1/24", "2001:4860::1/64"]),
        (peer, "opsd1", ["11.0.0.2/24", "11.0.0.3/24", "2001:4860::2/64", "2001:4860::3/64"]),
    ]:
        for address in addresses:
            run("ip", "-n", namespace, "address", "add", address, "dev", interface, *(["nodad"] if ":" in address else []))
        run("ip", "-n", namespace, "link", "set", interface, "up")
        process = subprocess.Popen(["ip", "netns", "exec", namespace, "python3", "-u", "-c", server], stdout=subprocess.PIPE, text=True)
        servers.append(process)
        import select
        if not select.select([process.stdout], [], [], 5)[0] or process.stdout.readline().strip() != "ready":
            raise AssertionError("实验服务未就绪")
    # 先证明正反探测源本来都可达，避免把坏拓扑计作拒绝成功。
    for source in ["11.0.0.2", "11.0.0.3", "2001:4860::2", "2001:4860::3"]:
        destination = "2001:4860::1" if ":" in source else "11.0.0.1"
        verify(peer, source, destination, True)
    for tool in ["iptables", "ip6tables"]:
        inside(target, tool, "-A", "INPUT", "-p", "tcp", "--dport", "19999", "-m", "comment", "--comment", "external-sentinel", "-j", "ACCEPT")
    for iteration in range(2):
        for tool, family in [("iptables", "4"), ("ip6tables", "6")]:
            # 事务只提交一次；随后重复使用全新连接验证，不复用已建立连接。
            if iteration == 0:
                inside(target, tool + "-restore", "--test", "--noflush", str(directory / (family + ".rules")))
                inside(target, tool + "-restore", "--noflush", str(directory / (family + ".rules")))
                assert "external-sentinel" in inside(target, tool + "-save")
        for source, expected in [("11.0.0.2", True), ("11.0.0.3", False), ("2001:4860::2", True), ("2001:4860::3", False)]:
            destination = "2001:4860::1" if ":" in source else "11.0.0.1"
            verify(peer, source, destination, expected)
            verify(target, destination, source, expected)
    print("通过：真实隔离 IPv4/IPv6 新 TCP 连接、入站/出站节点优先放行、非节点拒绝、外部规则保留。")
finally:
    for process in servers:
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
    for namespace in reversed(created):
        subprocess.run(["ip", "netns", "delete", namespace], check=True)
