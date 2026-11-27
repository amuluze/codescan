import flask as f
import os


def flask_module_alias():
    # ruleid: py/taint-os-system-request-alias
    # ruleid: py/os-system
    os.system(f.request.args.get("cmd"))
