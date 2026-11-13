import os


def ping_host(host):
    # ruleid: py/os-system
    os.system("ping -c 1 " + host)


def read_output(cmd):
    # ruleid: py/os-system
    return os.popen(cmd).read()


def current_pid():
    return os.getpid()
