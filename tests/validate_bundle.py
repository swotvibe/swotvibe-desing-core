"""Independent ZIP validation for archives emitted by the project's CLI."""
import shutil
import subprocess
import sys
import zipfile
import os

path = sys.argv[1]
with zipfile.ZipFile(path, "r") as archive:
    assert archive.testzip() is None, "Python detected a bad member or CRC"
    assert archive.comment == b""
    assert archive.namelist()[0] == "document.json"
    assert all(info.compress_type == zipfile.ZIP_STORED for info in archive.infolist())

for tool, args in (
    ("unzip", ["-t", path]),
    ("zipinfo", ["-1", path]),
    ("7z", ["t", path]),
    ("7zz", ["t", path]),
):
    executable = shutil.which(tool)
    if executable:
        subprocess.run([executable, *args], check=True, stdout=subprocess.DEVNULL)
    elif os.environ.get("SWOTVIBE_REQUIRE_EXTERNAL_ZIP_TOOLS") and tool in ("unzip", "zipinfo", "7z"):
        raise SystemExit(f"CI requires independent tool: {tool}")
