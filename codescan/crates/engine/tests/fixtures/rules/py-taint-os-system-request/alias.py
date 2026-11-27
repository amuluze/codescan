from flask import request as req
import os


def alias_handler():
    # ruleid: py/taint-os-system-request-alias
    # ruleid: py/os-system
    os.system(req.args.get("cmd"))
