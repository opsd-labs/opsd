"""仅在专用 opsd WSL 实验室构造离线测试镜像，不拉取外部镜像。"""
import io
import os
import pathlib
import re
import subprocess
import tarfile

if os.environ.get("OPSD_LAB") != "1":
    raise SystemExit("仅允许在明确指定的实验室运行：OPSD_LAB=1")
paths = set()
for binary in ["/bin/sh", "/bin/sleep"]:
    paths.add(binary)
    output = subprocess.check_output(["ldd", binary], text=True)
    paths.update(re.findall(r"(/[^\s()]+)", output))
archive = io.BytesIO()
with tarfile.open(fileobj=archive, mode="w") as target:
    target.dereference = True
    for item in sorted(paths):
        target.add(item, arcname=item.lstrip("/"), recursive=False)
archive.seek(0)
subprocess.run(["docker", "import", "-", "opsd-lab-fixture:v1"], input=archive.read(), check=True)
print("离线实验镜像已创建，仅包含测试所需 shell、sleep 与动态库。")
