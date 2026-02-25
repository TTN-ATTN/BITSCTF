from flask import Flask, jsonify, request
import cheroot.wsgi
import os

app = Flask(__name__)

FLAG = os.getenv("FLAG", "BITSCTF{fake_flag}")


@app.route('/', methods=['GET', 'POST'])
def index():
    if request.method == 'POST':
        _ = request.get_data()
    return jsonify({
        "version": "1.0.0",
        "endpoints": ["/", "/api/status"],
    })


@app.route('/api/status')
def status():
    return jsonify({
        "status": "running",
        "uptime": "operational",
    })


@app.route('/admin/flag')
def vault():
    return jsonify({"flag": FLAG})


if __name__ == '__main__':
    bind_addr = ('0.0.0.0', 8080)
    server = cheroot.wsgi.Server(bind_addr, app)
    print(f"Backend running on {bind_addr[0]}:{bind_addr[1]}")
    try:
        server.start()
    except KeyboardInterrupt:
        server.stop()
