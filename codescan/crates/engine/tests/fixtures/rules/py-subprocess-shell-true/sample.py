import subprocess


def run_command(cmd):
    # ruleid: py/subprocess-shell-true
    subprocess.run(cmd, shell=True)


def spawn_worker(cmd):
    # ruleid: py/subprocess-shell-true
    proc = subprocess.Popen(cmd, shell=True)
    return proc


def safe_run(cmd):
    subprocess.run(cmd, shell=False)
    subprocess.run(["ls", "-la"])
    subprocess.check_output(cmd)
