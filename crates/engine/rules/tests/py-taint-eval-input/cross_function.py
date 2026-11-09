def producer():
    return input("payload")


def consumer():
    # ruleid: py/taint-eval-input
    # ruleid: py/no-eval-exec
    eval(producer())


def run(code):
    # ruleid: py/taint-eval-input
    # ruleid: py/taint-eval-param
    # ruleid: py/no-eval-exec
    eval(code)


def entry():
    value = input("payload")
    run(value)
