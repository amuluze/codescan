import os


def handler(request):
    # ruleid: py/taint-os-system-request
    # ruleid: py/os-system
    os.system(request.args.get("cmd"))


def static_handler(request):
    # ruleid: py/os-system
    os.system("echo ok")
