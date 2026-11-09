def run_expr():
    # ruleid: py/taint-eval-input
    # ruleid: py/no-eval-exec
    eval(input("expr: "))


def sanitized():
    value = int(input("num: "))
    # ruleid: py/no-eval-exec
    eval(value)


def static_only():
    # ruleid: py/no-eval-exec
    eval("1 + 1")


def producer():
    data = input("payload: ")


def consumer():
    # ruleid: py/no-eval-exec
    eval(data)
