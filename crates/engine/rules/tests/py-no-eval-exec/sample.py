import os


def run_dynamic(code_text):
    # ruleid: py/no-eval-exec
    # ruleid: py/taint-eval-param
    return eval(code_text)


def exec_dynamic(code_text):
    # ruleid: py/no-eval-exec
    # ruleid: py/taint-exec-param
    exec(code_text)


def safe_calls(values):
    total = len(values)
    getter = getattr(values, "evaluate", None)
    return total, getter, os.getcwd()
