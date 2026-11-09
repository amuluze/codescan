def handle(user_input):
    # ruleid: py/taint-eval-param
    # ruleid: py/no-eval-exec
    eval(user_input)


def sanitizer_cuts(raw):
    number = int(raw)
    # ruleid: py/no-eval-exec
    eval(number)


def producer(src):
    print(src)


def consumer():
    # ruleid: py/no-eval-exec
    eval(external_code)
