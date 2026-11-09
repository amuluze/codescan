def run_code(user_code):
    # ruleid: py/taint-exec-param
    # ruleid: py/no-eval-exec
    exec(user_code)


def no_flow():
    # ruleid: py/no-eval-exec
    exec("print('ok')")
