def check_admin(user):
    # ruleid: py/assert-in-auth
    assert user.is_admin, "admin only"


def verify_token(token):
    # ruleid: py/assert-in-auth
    assert token.valid


def check_secret(value, expected):
    # ruleid: py/assert-in-auth
    assert value == expected, "secret mismatch"

def add_numbers(a, b):
    assert a > 0, "positive input"
    return a + b
