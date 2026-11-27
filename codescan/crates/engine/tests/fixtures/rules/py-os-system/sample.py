import os
import os as operating_system
from os import system
from os import popen as run_popen


def ping_host(host):
    # ruleid: py/os-system
    os.system("ping -c 1 " + host)


def read_output(cmd):
    # ruleid: py/os-system
    return os.popen(cmd).read()


def aliased_host(host):
    # ruleid: py/os-system
    operating_system.system("ping -c 1 " + host)


def imported_host(host):
    # ruleid: py/os-system
    system("ping -c 1 " + host)


def renamed_popen(cmd):
    # ruleid: py/os-system
    return run_popen(cmd).read()


def current_pid():
    return os.getpid()
