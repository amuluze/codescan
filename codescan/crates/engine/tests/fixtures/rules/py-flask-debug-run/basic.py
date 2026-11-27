from flask import Flask

app = Flask(__name__)
# ruleid: py/flask-debug-run
app.run(debug=True)
app.run(debug=False)
